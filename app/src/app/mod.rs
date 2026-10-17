//! The app: window state, the event loop and drawing. Input, menu commands and the formula session live in child modules.
mod commands;
mod input;
mod math_session;

use crate::{
    config::*,
    document::Document,
    file,
    math::input::MathInput,
    math::{render::FormulaImage, worker::Worker},
    menu::{self, Action},
    view::{
        caret,
        layout::{Layout, Typesetter},
        render,
    },
};
use cosmic_text::SwashCache;
use std::{
    collections::{HashMap, HashSet},
    num::NonZeroU32,
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize},
    event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy},
    keyboard::ModifiersState,
    platform::macos::{WindowAttributesExtMacOS, WindowExtMacOS},
    window::{Window, WindowId},
};

type Surface = softbuffer::Surface<Rc<Window>, Rc<Window>>;
pub struct App {
    proxy: EventLoopProxy<Action>,
    menu: Option<muda::Menu>,
    window: Option<Rc<Window>>,
    surface: Option<Surface>,
    doc: Document,
    path: Option<PathBuf>,
    typesetter: Typesetter,
    cache: SwashCache,
    layout: Layout,
    pixels: Vec<u32>,
    page_dirty: bool,
    layout_dirty: bool,
    reveal: bool,
    modifiers: ModifiersState,
    mouse: (f32, f32),
    dragging: bool,
    focused: bool,
    scroll: f32,
    goal_x: Option<f32>,
    line_hint: Option<usize>,
    clipboard: Option<arboard::Clipboard>,
    composing: bool,
    window_geometry: file::WindowGeometry,
    size_save_at: Option<Instant>,
    worker: Worker,
    submitted: HashSet<u64>,
    online: bool,
    math: Option<MathInput>,
    math_cache: HashMap<String, Arc<FormulaImage>>,
}
impl App {
    pub fn new(proxy: EventLoopProxy<Action>) -> Self {
        let path = std::env::args_os()
            .nth(1)
            .filter(|arg| !arg.to_string_lossy().starts_with("-psn_"))
            .map(PathBuf::from)
            .or_else(file::last);
        let (doc, path) = match path {
            Some(path) => match file::read(&path) {
                Ok(text) => (Document::new(&text), Some(path)),
                Err(error) => {
                    eprintln!("Cannot open {}: {error}", path.display());
                    (Document::new(""), None)
                }
            },
            None => (Document::new(""), None),
        };
        let wake = proxy.clone();
        let worker = Worker::new(move || {
            let _ = wake.send_event(Action::MathReady);
        });
        Self {
            worker,
            submitted: Default::default(),
            online: true,
            math: None,
            math_cache: Default::default(),
            proxy,
            menu: None,
            window: None,
            surface: None,
            doc,
            path,
            typesetter: Typesetter::new(),
            cache: SwashCache::new(),
            layout: Layout::default(),
            pixels: vec![],
            page_dirty: true,
            layout_dirty: true,
            reveal: true,
            modifiers: ModifiersState::empty(),
            mouse: (0.0, 0.0),
            dragging: false,
            focused: true,
            scroll: 0.0,
            goal_x: None,
            line_hint: None,
            clipboard: arboard::Clipboard::new().ok(),
            composing: false,
            window_geometry: file::window_geometry(),
            size_save_at: None,
        }
    }
    fn capture_window_geometry(&mut self) {
        let Some(window) = &self.window else {
            return;
        };
        // Native zoom is still a normal window. Only a separate full-screen
        // space or minimization should leave the last normal geometry intact.
        if window.fullscreen().is_some() || window.is_minimized().unwrap_or(false) {
            return;
        }
        let size = window.inner_size().to_logical::<f64>(window.scale_factor());
        if size.width >= 300.0 && size.height >= 240.0 {
            self.window_geometry.width = size.width;
            self.window_geometry.height = size.height;
            if let Ok(pos) = window.outer_position() {
                let pos = pos.to_logical::<f64>(window.scale_factor());
                self.window_geometry.position = Some((pos.x, pos.y));
            }
            self.size_save_at = Some(Instant::now() + Duration::from_millis(400));
        }
    }
    fn dimensions(&self) -> (usize, usize, f32, f32, f32) {
        let window = self.window.as_ref().unwrap();
        let size = window.inner_size();
        let scale = window.scale_factor() as f32;
        let width = size.width as f32 / scale;
        let column = (width - 64.0).clamp(40.0, COLUMN_WIDTH);
        (
            size.width as usize,
            size.height as usize,
            scale,
            (width - column) / 2.0,
            column,
        )
    }
    fn changed(&mut self, text: bool) {
        if text {
            for job in self.doc.jobs() {
                if let Some(image) = job
                    .prompt
                    .as_ref()
                    .and_then(|p| self.math_cache.get(p))
                    .filter(|_| job.latex.is_none())
                {
                    self.doc.complete(job.id, Ok(image.clone()));
                } else if self.submitted.insert(job.id) {
                    self.worker.submit(job);
                }
            }
        }
        self.layout_dirty |= text;
        if text {
            self.line_hint = None;
        }
        self.page_dirty = true;
        self.reveal = true;
        if let Some(w) = &self.window {
            w.set_document_edited(self.doc.dirty());
            w.set_title(&format!(
                "{}{} — Mathstral",
                self.path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Untitled".to_owned()),
                if self.doc.dirty() { " •" } else { "" }
            ));
            w.request_redraw();
        }
    }
    fn refresh_layout(&mut self) {
        if self.layout_dirty {
            self.layout = self.typesetter.document(&self.doc, self.dimensions().4);
            self.layout_dirty = false;
            self.page_dirty = true;
        }
    }
    fn draw(&mut self) {
        let (width, height, scale, left, _) = self.dimensions();
        if width == 0 || height == 0 {
            return;
        }
        self.refresh_layout();
        let (row, x) = self.layout.caret_on_line(self.doc.caret, self.line_hint);
        let line_top = self.layout.lines[row].top;
        let line_height = self.layout.lines[row].height;
        let view_height = height as f32 / scale;
        if self.reveal {
            let y = TOP + line_top - self.scroll;
            if y < 48.0 {
                self.scroll = (self.scroll + y - 48.0).max(0.0);
                self.page_dirty = true;
            }
            if y + line_height > view_height - 48.0 {
                self.scroll += (y + line_height - (view_height - 48.0)).max(0.0);
                self.page_dirty = true;
            }
            self.reveal = false;
        }
        let max_scroll = (TOP + self.layout.height() + TOP - view_height).max(0.0);
        let scroll = self.scroll.clamp(0.0, max_scroll);
        if scroll != self.scroll {
            self.scroll = scroll;
            self.page_dirty = true;
        }
        if self.page_dirty || self.pixels.len() != width * height {
            self.pixels = render::page(
                &self.layout,
                &mut self.typesetter,
                &mut self.cache,
                width,
                height,
                scale,
                left,
                self.scroll,
                self.doc.selection(),
            );
            if !self.online {
                let mut status = self.typesetter.layout(
                    "Ollama unavailable · math will resume when it connects",
                    width as f32 / scale - 40.0,
                );
                for glyph in &mut status.glyphs {
                    glyph.color = PENDING_COLOR;
                }
                let band = render::page(
                    &status,
                    &mut self.typesetter,
                    &mut self.cache,
                    width,
                    height,
                    scale,
                    20.0,
                    TOP - height as f32 / scale + 36.0,
                    0..0,
                );
                let start = height.saturating_sub((44.0 * scale) as usize) * width;
                self.pixels[start..].copy_from_slice(&band[start..]);
            }
            self.page_dirty = false;
        }
        let manual_caret = self.manual_caret(left);
        let surface = self.surface.as_mut().unwrap();
        if let Err(error) = surface.resize(
            NonZeroU32::new(width as u32).unwrap(),
            NonZeroU32::new(height as u32).unwrap(),
        ) {
            eprintln!("Resize: {error}");
            return;
        }
        let Ok(mut pixels) = surface.buffer_mut() else {
            return;
        };
        pixels.copy_from_slice(&self.pixels);
        let style = if self.math.is_some() {
            caret::CaretStyle::Math
        } else if self
            .doc
            .open_range()
            .is_some_and(|range| range.start <= self.doc.caret && self.doc.caret <= range.end)
        {
            caret::CaretStyle::Prompt
        } else {
            caret::CaretStyle::Prose
        };
        let (layout_top, layout_height) = self.layout.caret_extent(row, self.doc.caret);
        let (caret_x, caret_top, caret_height) =
            manual_caret.unwrap_or((left + x, TOP + layout_top - self.scroll, layout_height));
        if self.focused && self.doc.selection().is_empty() {
            caret::caret(
                &mut pixels,
                width,
                height,
                caret_x,
                caret_top,
                caret_height,
                scale,
                style,
            );
        }
        if let Err(error) = pixels.present() {
            eprintln!("Present: {error}");
        }
        self.window.as_ref().unwrap().set_ime_cursor_area(
            LogicalPosition::new(caret_x as f64, caret_top as f64),
            LogicalSize::new(1.0, caret_height as f64),
        );
    }
    /// The hand-written caret in window units (x, top, height), from the preview of the formula
    /// being edited. None until that preview has a caret box, so the layout caret is drawn instead.
    fn manual_caret(&self, left: f32) -> Option<(f32, f32, f32)> {
        let id = self.doc.manual_id()?;
        let image = self
            .doc
            .formulas
            .values()
            .find(|formula| formula.id == id)?
            .image
            .as_ref()?;
        caret::formula_caret(&self.layout, image, left, TOP - self.scroll)
    }
}

impl ApplicationHandler<Action> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let mut attrs = Window::default_attributes()
            .with_title("Mathstral")
            .with_inner_size(LogicalSize::new(
                self.window_geometry.width,
                self.window_geometry.height,
            ))
            .with_min_inner_size(LogicalSize::new(300.0, 240.0))
            .with_titlebar_transparent(true)
            .with_title_hidden(true)
            .with_titlebar_hidden(false)
            .with_titlebar_buttons_hidden(false)
            .with_fullsize_content_view(true);
        if let Some((x, y)) = self.window_geometry.position {
            // Ignore an off-screen position after a display has been disconnected.
            let visible = event_loop.available_monitors().any(|monitor| {
                let pos = monitor.position().to_logical::<f64>(monitor.scale_factor());
                let size = monitor.size().to_logical::<f64>(monitor.scale_factor());
                x + 100.0 > pos.x
                    && x < pos.x + size.width
                    && y + 32.0 > pos.y
                    && y < pos.y + size.height
            });
            if visible {
                attrs = attrs.with_position(LogicalPosition::new(x, y));
            }
        }
        let window = Rc::new(event_loop.create_window(attrs).expect("native window"));
        window.set_ime_allowed(true);
        let context = softbuffer::Context::new(window.clone()).expect("CPU drawing context");
        self.surface =
            Some(softbuffer::Surface::new(&context, window.clone()).expect("CPU drawing surface"));
        self.window = Some(window);
        self.menu = Some(menu::install(self.proxy.clone()));
        self.changed(true);
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, action: Action) {
        self.action(event_loop, action);
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.finish_math();
                if self.may_discard() {
                    event_loop.exit();
                }
            }
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.capture_window_geometry();
                self.layout_dirty = true;
                self.line_hint = None;
                self.changed(false);
            }
            WindowEvent::Moved(_) => self.capture_window_geometry(),
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::KeyboardInput { event, .. } => self.key(event),
            WindowEvent::Ime(Ime::Preedit(text, _)) => {
                self.composing = !text.is_empty();
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.composing = false;
                self.text_input(&text);
            }
            WindowEvent::Ime(Ime::Disabled) => self.composing = false,
            WindowEvent::Focused(focused) => {
                self.focused = focused;
                if !focused {
                    self.doc.break_group();
                    self.dragging = false;
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self.window.as_ref().unwrap().scale_factor();
                self.mouse = ((position.x / scale) as f32, (position.y / scale) as f32);
                if self.dragging {
                    self.mouse_caret(true);
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                if state == ElementState::Pressed {
                    if self.modifiers.super_key() || self.mouse.1 < 32.0 {
                        let _ = self.window.as_ref().unwrap().drag_window();
                    } else {
                        self.finish_math();
                        self.dragging = true;
                        self.mouse_caret(self.modifiers.shift_key());
                    }
                } else {
                    self.dragging = false;
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let delta = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y * LINE_HEIGHT,
                    MouseScrollDelta::PixelDelta(p) => {
                        (p.y / self.window.as_ref().unwrap().scale_factor()) as f32
                    }
                };
                self.scroll = (self.scroll - delta).max(0.0);
                self.page_dirty = true;
                self.reveal = false;
                self.window.as_ref().unwrap().request_redraw();
            }
            _ => {}
        }
    }
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.capture_window_geometry();
        file::remember_window_geometry(self.window_geometry);
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self
            .size_save_at
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            file::remember_window_geometry(self.window_geometry);
            self.size_save_at = None;
        }
        event_loop.set_control_flow(
            self.size_save_at
                .map_or(ControlFlow::Wait, ControlFlow::WaitUntil),
        );
    }
}
