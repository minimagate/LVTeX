//! Application state and coordination of document, layout and background work.
mod chart;
mod commands;
mod drawing;
mod events;
mod input;
mod math_session;
mod mls;
mod settings;

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
    ops::Range,
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    dpi::{LogicalPosition, LogicalSize},
    event_loop::EventLoopProxy,
    keyboard::ModifiersState,
    platform::macos::WindowExtMacOS,
    window::Window,
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
    mls: lvtex_mls::Mls,
    mls_worker: lvtex_mls::Worker,
    // Whether Lean checking runs. Off, no job is submitted and no dot or chart row is shown.
    mls_on: bool,
    // Each paragraph's byte range, indexed like the paragraphs last given to MLS.
    mls_lines: Vec<Range<usize>>,
    dots: Vec<lvtex_mls::Dot>,
    fades: HashMap<u64, mls::Fade>,
    // True until the first sync, so a new or opened document checks every line, the caret's too.
    mls_fresh: bool,
    panel: Option<mls::Panel>,
    chart: Option<chart::Chart>,
    settings: Option<settings::Settings>,
    pointer: bool,
    frame_due: Option<Instant>,
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
        let mls_wake = proxy.clone();
        let mls_worker = lvtex_mls::Worker::new(mls::lean_project(), move || {
            let _ = mls_wake.send_event(Action::MlsReady);
        });
        let mut app = Self {
            worker,
            mls: lvtex_mls::Mls::new(),
            mls_worker,
            mls_on: file::mls_enabled(),
            mls_lines: Vec::new(),
            dots: Vec::new(),
            fades: HashMap::new(),
            mls_fresh: true,
            panel: None,
            chart: None,
            settings: None,
            pointer: false,
            frame_due: None,
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
        };
        app.mls_worker.set_key(settings::effective_key());
        if let Some(path) = app.path.clone() {
            app.restore_mls(&path);
        }
        app
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
            // Writing hides the panel.
            self.panel = None;
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
        self.sync_mls();
        if let Some(w) = &self.window {
            w.set_document_edited(self.doc.dirty());
            w.set_title(&format!(
                "{}{} — LVTeX",
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
}
