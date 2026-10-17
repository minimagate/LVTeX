use crate::{
    buffer::Document,
    config::*,
    export, file,
    keymap::{MathInput, MathKey, Outcome},
    layout::{Layout, Typesetter},
    markdown,
    math::{FormulaImage, Preview, Reply, Worker},
    menu::{self, Action},
    render,
    structure::MathTree,
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
    event::{ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy},
    keyboard::{Key, ModifiersState, NamedKey},
    platform::{
        macos::{WindowAttributesExtMacOS, WindowExtMacOS},
        modifier_supplement::KeyEventExtModifierSupplement,
    },
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
        let manual_preview_has_caret = self.doc.manual_id().is_some_and(|id| {
            self.doc
                .formulas
                .values()
                .find(|formula| formula.id == id)
                .is_some_and(|formula| formula.image.is_some())
        });
        if self.focused
            && self.doc.selection().is_empty()
            && (self.math.is_none() || !manual_preview_has_caret)
        {
            render::rect(
                &mut pixels,
                width,
                height,
                (left + x) * scale,
                (TOP + line_top + 4.0 - self.scroll) * scale,
                scale.max(1.0),
                (line_height - 8.0) * scale,
                CARET_COLOR,
            );
        }
        if let Err(error) = pixels.present() {
            eprintln!("Present: {error}");
        }
        self.window.as_ref().unwrap().set_ime_cursor_area(
            LogicalPosition::new(
                (left + x) as f64,
                (TOP + line_top + line_height - self.scroll) as f64,
            ),
            LogicalSize::new(1.0, line_height as f64),
        );
    }
    fn save(&mut self, save_as: bool) -> bool {
        self.doc.break_group();
        let path = if save_as || self.path.is_none() {
            file::choose_save(self.path.as_deref())
        } else {
            self.path.clone()
        };
        let Some(path) = path else { return false };
        match file::write(&path, self.doc.markdown()) {
            Ok(()) => {
                self.path = Some(path);
                self.doc.mark_saved();
                file::remember(self.path.as_deref());
                self.changed(false);
                true
            }
            Err(error) => {
                file::error("Could not save the document.", error);
                false
            }
        }
    }
    fn export_pdf(&mut self) {
        self.doc.break_group();
        let n = export::unrendered(&self.doc);
        if n > 0 && !file::confirm_unrendered(n) {
            return;
        }
        let Some(path) = file::choose_export(self.path.as_deref()) else {
            return;
        };
        if let Err(error) = export::pdf(&self.doc)
            .and_then(|bytes| file::write(&path, bytes).map_err(|e| e.to_string()))
        {
            file::error("Could not export the PDF.", error);
        }
    }
    fn may_discard(&mut self) -> bool {
        if !self.doc.dirty() {
            return true;
        }
        use rfd::{MessageButtons, MessageDialog, MessageDialogResult};
        match MessageDialog::new()
            .set_title("Save changes?")
            .set_description("Your changes will be lost if you don’t save them.")
            .set_buttons(MessageButtons::YesNoCancelCustom(
                "Save".into(),
                "Don’t Save".into(),
                "Cancel".into(),
            ))
            .show()
        {
            MessageDialogResult::Custom(s) if s == "Save" => self.save(false),
            MessageDialogResult::Custom(s) if s == "Don’t Save" => true,
            MessageDialogResult::Yes => self.save(false),
            MessageDialogResult::No => true,
            _ => false,
        }
    }
    fn action(&mut self, event_loop: &ActiveEventLoop, action: Action) {
        self.doc.break_group();
        self.goal_x = None;
        if self.math.is_some() {
            match action {
                Action::MathReady => {}
                Action::Undo | Action::Redo => {
                    let key = if matches!(action, Action::Undo) {
                        MathKey::Undo
                    } else {
                        MathKey::Redo
                    };
                    self.math_input([key]);
                    return;
                }
                Action::InsertFormula
                | Action::AskAiFormula
                | Action::Copy
                | Action::Cut
                | Action::Paste
                | Action::SelectAll => return,
                _ => self.finish_math(),
            }
        }
        match action {
            Action::MathReady => {
                let mut changed = false;
                while let Ok(reply) = self.worker.rx.try_recv() {
                    match reply {
                        Reply::Connection(online) => {
                            self.online = online;
                            changed = true;
                        }
                        Reply::Done(id, result) => {
                            if let Ok(image) = &result
                                && let Some(prompt) = self
                                    .doc
                                    .formulas
                                    .values()
                                    .find(|f| f.id == id)
                                    .and_then(|f| f.prompt.clone())
                            {
                                if self.math_cache.len() >= MATH_CACHE_LIMIT {
                                    self.math_cache.clear();
                                }
                                self.math_cache.insert(prompt, image.clone());
                            }
                            changed |= self.doc.complete(id, result);
                        }
                        Reply::PreviewDone(id, revision, result) => {
                            changed |= self.doc.complete_manual_preview(id, revision, result);
                        }
                    }
                }
                if changed {
                    self.changed(true);
                    self.reveal = false;
                }
            }
            Action::InsertFormula => {
                let in_prompt = self
                    .doc
                    .open_range()
                    .is_some_and(|r| r.start <= self.doc.caret && self.doc.caret <= r.end);
                if !in_prompt {
                    self.doc.open_manual();
                    self.math = Some(MathInput::new(MathTree::new()));
                    self.preview();
                }
            }
            Action::AskAiFormula => {
                let in_prompt = self
                    .doc
                    .open_range()
                    .is_some_and(|r| r.start <= self.doc.caret && self.doc.caret <= r.end);
                if !in_prompt {
                    self.doc.open_ai_prompt();
                    self.changed(true);
                }
            }
            Action::Quit => {
                if self.may_discard() {
                    event_loop.exit();
                }
            }
            Action::New => {
                if self.may_discard() {
                    self.doc = Document::new("");
                    self.path = None;
                    self.scroll = 0.0;
                    file::remember(None);
                    self.changed(true);
                }
            }
            Action::Open => {
                if !self.may_discard() {
                    return;
                }
                if let Some(path) = file::choose_open() {
                    match file::read(&path) {
                        Ok(text) => {
                            self.doc = Document::new(&text);
                            self.path = Some(path);
                            self.scroll = 0.0;
                            file::remember(self.path.as_deref());
                            self.changed(true);
                        }
                        Err(error) => file::error("Could not open the document.", error),
                    }
                }
            }
            Action::Save => {
                self.save(false);
            }
            Action::SaveAs => {
                self.save(true);
            }
            Action::ExportPdf => self.export_pdf(),
            Action::Undo | Action::Redo => {
                self.doc.undo(matches!(action, Action::Redo));
                self.changed(true);
            }
            Action::SelectAll => {
                self.doc.anchor = 0;
                self.doc.caret = self.doc.rope.len_bytes();
                self.changed(false);
            }
            Action::Copy | Action::Cut => {
                if !self.doc.selection().is_empty() {
                    let result = self
                        .clipboard
                        .as_mut()
                        .ok_or_else(|| "Clipboard unavailable".to_owned())
                        .and_then(|c| c.set_text(self.doc.selected()).map_err(|e| e.to_string()));
                    match result {
                        Ok(()) => {
                            if matches!(action, Action::Cut) {
                                self.doc.replace("", false);
                                self.changed(true);
                            }
                        }
                        Err(error) => file::error("Could not copy text.", error),
                    }
                }
            }
            Action::Paste => {
                let text = self.clipboard.as_mut().and_then(|c| c.get_text().ok());
                if let Some(text) = text {
                    self.doc
                        .paste(&text.replace("\r\n", "\n").replace('\r', "\n"));
                    self.changed(true);
                }
            }
        }
    }
    fn key(&mut self, event: KeyEvent) {
        if event.state != ElementState::Pressed || self.composing {
            return;
        }
        if self.math.is_some() {
            let keys = math_keys(&event, self.modifiers);
            self.math_input(keys);
            return;
        }
        self.refresh_layout();
        let shift = self.modifiers.shift_key();
        let command = self.modifiers.super_key();
        let option = self.modifiers.alt_key();
        let (row, x) = self.layout.caret_on_line(self.doc.caret, self.line_hint);
        let mut destination = None;
        let mut destination_row = None;
        match event.logical_key {
            Key::Named(key @ (NamedKey::ArrowLeft | NamedKey::ArrowRight)) => {
                let forward = key == NamedKey::ArrowRight;
                destination = Some(
                    if !shift && !command && !option && !self.doc.selection().is_empty() {
                        if forward {
                            self.doc.selection().end
                        } else {
                            self.doc.selection().start
                        }
                    } else if command {
                        destination_row = Some(row);
                        if forward {
                            self.layout.lines[row].end
                        } else {
                            self.layout.lines[row].start
                        }
                    } else {
                        self.doc.adjacent(forward, option)
                    },
                );
                self.goal_x = None;
            }
            Key::Named(key @ (NamedKey::ArrowUp | NamedKey::ArrowDown)) => {
                let down = key == NamedKey::ArrowDown;
                destination = Some(if command {
                    if down { self.doc.rope.len_bytes() } else { 0 }
                } else {
                    let goal = *self.goal_x.get_or_insert(x);
                    if down && row + 1 >= self.layout.lines.len() {
                        self.doc.rope.len_bytes()
                    } else if !down && row == 0 {
                        0
                    } else {
                        let target_row = if down { row + 1 } else { row - 1 };
                        destination_row = Some(target_row);
                        self.layout.hit(target_row, goal)
                    }
                });
            }
            Key::Named(NamedKey::Home) => {
                destination = Some(0);
                self.goal_x = None;
            }
            Key::Named(NamedKey::End) => {
                destination = Some(self.doc.rope.len_bytes());
                self.goal_x = None;
            }
            Key::Named(NamedKey::Escape) => {
                if self.doc.cancel() {
                    self.changed(true);
                }
            }
            Key::Named(key @ (NamedKey::Backspace | NamedKey::Delete)) => {
                if key == NamedKey::Backspace
                    && !command
                    && !option
                    && let Some(pos) = self.doc.formula_at_caret(true)
                {
                    self.edit_math(pos);
                    return;
                }
                if self.doc.selection().is_empty() {
                    let forward = key == NamedKey::Delete;
                    self.doc.anchor = if command {
                        if forward {
                            self.layout.lines[row].end
                        } else {
                            self.layout.lines[row].start
                        }
                    } else {
                        self.doc.adjacent(forward, option)
                    };
                }
                self.doc.replace("", false);
                self.goal_x = None;
                self.changed(true);
            }
            Key::Named(NamedKey::Enter) if !command => {
                if let Some(pos) = self.doc.formula_at_caret(false) {
                    let formula = &self.doc.formulas[&pos];
                    if formula.prompt.is_some() && formula.failed && formula.image.is_none() {
                        self.doc.retry(pos);
                        self.changed(true);
                    } else {
                        self.edit_math(pos);
                    }
                    return;
                }
                if self.doc.submit_prompt() {
                    self.changed(true);
                    return;
                }
                match self
                    .doc
                    .selection()
                    .is_empty()
                    .then(|| markdown::enter_edit(&self.doc.text(), self.doc.caret))
                    .flatten()
                {
                    Some((range, value)) => {
                        self.doc.anchor = range.start;
                        self.doc.caret = range.end;
                        self.doc.replace(&value, false);
                    }
                    None => self.doc.replace("\n", false),
                }
                self.goal_x = None;
                self.changed(true);
            }
            Key::Named(NamedKey::Tab) if !command => {
                self.doc.replace("    ", false);
                self.goal_x = None;
                self.changed(true);
            }
            _ if !command && !self.modifiers.control_key() => {
                if let Some(text) = event.text.filter(|s| !s.chars().any(char::is_control)) {
                    self.text_input(&text);
                }
            }
            _ => {}
        }
        if let Some(destination) = destination {
            self.doc.move_to(destination, shift);
            self.line_hint = destination_row;
            self.changed(false);
        }
    }
    fn text_input(&mut self, text: &str) {
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        if text.is_empty() {
            return;
        }
        if self.math.is_some() {
            self.math_input(text.chars().map(MathKey::Text));
            return;
        }
        if self
            .doc
            .open_range()
            .is_some_and(|range| range.start <= self.doc.caret && self.doc.caret <= range.end)
        {
            self.doc.replace(&text, true);
            self.goal_x = None;
            self.changed(true);
            return;
        }

        let mut prose = String::new();
        let mut changed = false;
        for c in text.chars() {
            if self.math.is_some() {
                self.math_input([MathKey::Text(c)]);
            } else if c == '\\' {
                if !prose.is_empty() {
                    self.doc.replace(&prose, true);
                    prose.clear();
                }
                self.doc.open_manual();
                self.math = Some(MathInput::new(MathTree::new()));
                self.math_input([MathKey::Text('\\')]);
                changed = true;
            } else {
                prose.push(c);
            }
        }
        if !prose.is_empty() {
            self.doc.replace(&prose, true);
            changed = true;
        }
        if changed {
            self.goal_x = None;
            self.changed(true);
        }
    }
    fn edit_math(&mut self, pos: usize) {
        if self.doc.reopen(pos) {
            self.changed(true);
        } else if let Some(latex) = self.doc.reopen_manual(pos) {
            self.math = Some(MathInput::from_latex(&latex));
            self.preview();
        }
    }
    /// Feeds keys to the open formula. Edits and moves refresh the live image.
    fn math_input(&mut self, keys: impl IntoIterator<Item = MathKey>) {
        for key in keys {
            let Some(input) = self.math.as_mut() else {
                return;
            };
            match input.key(key) {
                Outcome::Edited | Outcome::Moved => self.preview(),
                Outcome::Close => {
                    self.finish_math();
                    return;
                }
                Outcome::Cancel => {
                    if let Some(id) = self.doc.manual_id() {
                        self.worker.cancel_preview(id);
                    }
                    self.math = None;
                    self.doc.cancel_manual();
                    self.changed(true);
                    return;
                }
                Outcome::Ignored => {}
            }
        }
    }
    /// Stores the source immediately and queues a latest-wins render of its live preview.
    fn preview(&mut self) {
        let Some(input) = &self.math else {
            return;
        };
        let latex = input.latex();
        let display_latex = input.display_latex();
        if let Some((id, revision)) = self.doc.update_manual(latex) {
            self.worker.preview(Preview {
                id,
                revision,
                display_latex,
            });
        }
        self.changed(true);
    }
    /// Stores the formula's LaTeX, lets the worker render it, and closes the edit.
    fn finish_math(&mut self) {
        let Some(input) = self.math.take() else {
            return;
        };
        if let Some(id) = self.doc.manual_id() {
            self.worker.cancel_preview(id);
        }
        self.doc.update_manual(input.finish());
        self.doc.finish_manual();
        self.changed(true);
    }
    fn mouse_caret(&mut self, extend: bool) {
        self.refresh_layout();
        let left = self.dimensions().3;
        let row = self.layout.row_at(self.mouse.1 + self.scroll - TOP);
        self.doc
            .move_to(self.layout.hit(row, self.mouse.0 - left), extend);
        self.line_hint = Some(row.min(self.layout.lines.len() - 1));
        self.goal_x = None;
        self.changed(false);
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

/// The keys a key press means inside a formula. Command shortcuts other than the ones listed
/// are menu actions and map to nothing.
fn math_keys(event: &KeyEvent, modifiers: ModifiersState) -> Vec<MathKey> {
    let shift = modifiers.shift_key();
    let command = modifiers.super_key();
    let option = modifiers.alt_key();
    let control = modifiers.control_key();
    // Option types brackets and symbols on some layouts, so only the keys the map binds become Alt.
    let base = match event.key_without_modifiers() {
        Key::Character(base) => base.chars().next().map(|c| c.to_ascii_lowercase()),
        _ => None,
    };
    if option
        && !command
        && !control
        && let Some(key @ ('f' | 's')) = base
    {
        return vec![MathKey::Alt { key, shift }];
    }
    let key = match event.logical_key {
        Key::Named(NamedKey::Backspace) if command && option => MathKey::ExitLeft,
        Key::Named(NamedKey::Delete) if command && option => MathKey::ExitRight,
        Key::Named(NamedKey::ArrowLeft) if command && option => MathKey::PreviousArgument,
        Key::Named(NamedKey::ArrowRight) if command && option => MathKey::NextArgument,
        Key::Named(_) if command => return Vec::new(),
        Key::Named(NamedKey::Enter) => MathKey::Return,
        Key::Named(NamedKey::Escape) => MathKey::Escape,
        Key::Named(NamedKey::Tab) if shift => MathKey::ShiftTab,
        Key::Named(NamedKey::Tab) => MathKey::Tab,
        Key::Named(NamedKey::Backspace) if option => MathKey::StructuredBackspace,
        Key::Named(NamedKey::Backspace) if control => MathKey::Unwrap,
        Key::Named(NamedKey::Backspace) => MathKey::Backspace,
        Key::Named(NamedKey::Delete) => MathKey::Delete,
        Key::Named(NamedKey::ArrowLeft) => MathKey::Left,
        Key::Named(NamedKey::ArrowRight) => MathKey::Right,
        Key::Named(NamedKey::ArrowUp) => MathKey::Up,
        Key::Named(NamedKey::ArrowDown) => MathKey::Down,
        Key::Named(NamedKey::F5) if shift => MathKey::SymbolPrefix,
        Key::Named(NamedKey::Space) => MathKey::Text(' '),
        Key::Named(_) => return Vec::new(),
        _ if command || control => return Vec::new(),
        _ => {
            return event
                .text
                .as_deref()
                .unwrap_or("")
                .chars()
                .filter(|c| !c.is_control())
                .map(MathKey::Text)
                .collect();
        }
    };
    vec![key]
}
