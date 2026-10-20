//! Lean checking: syncing lines to MLS, applying its replies, and the dots and panel.
use super::*;
use mathstral_mls::{Mls, Paragraph, Status};
use std::path::{Path, PathBuf};
use winit::window::CursorIcon;

/// Time between frames while a dot fades.
const FRAME: Duration = Duration::from_millis(16);
/// How far from a dot's centre the pointer still counts as on it, in logical pixels.
const HIT_RADIUS: f32 = 10.0;
/// Space between a dot's row and the panel beneath it, and between panel rows.
const GAP: f32 = 6.0;
/// Space above the Lean toggle and above the Lean.
const SECTION_GAP: f32 = 12.0;
/// Smallest distance the panel keeps from the window's top and bottom edges.
const EDGE: f32 = 12.0;
/// How far the panel's area reaches past its top and bottom while the pointer crosses to a dot.
const BRIDGE: f32 = 12.0;

/// A dot's colour on its way from one state to the next. `None` is no dot.
pub(super) struct Fade {
    paragraph: usize,
    from: Option<u32>,
    to: Option<u32>,
    start: Instant,
}

/// The open panel, for the dot with `key`. Areas are (x, y, w, h) in logical window coordinates,
/// as the last draw left them.
pub(super) struct Panel {
    key: u64,
    pinned: bool,
    show_lean: bool,
    area: (f32, f32, f32, f32),
    toggle: Option<(f32, f32, f32, f32)>,
}

impl Panel {
    fn new(key: u64, pinned: bool) -> Self {
        Self {
            key,
            pinned,
            show_lean: false,
            area: (0.0, 0.0, 0.0, 0.0),
            toggle: None,
        }
    }
}

/// One line of the panel, the space above it, and how it is set.
struct PanelRow {
    text: String,
    mono: bool,
    scale: f32,
    color: u32,
    gap: f32,
}

fn row(text: String, mono: bool, scale: f32, color: u32, gap: f32) -> PanelRow {
    PanelRow {
        text,
        mono,
        scale,
        color,
        gap,
    }
}

/// The Lake project for the REPL. `MATHSTRAL_LEAN_PROJECT` overrides the one in the repository.
pub(super) fn lean_project() -> PathBuf {
    std::env::var_os("MATHSTRAL_LEAN_PROJECT")
        .map_or_else(|| PathBuf::from(LEAN_PROJECT), PathBuf::from)
}

impl App {
    /// Gives MLS the document's paragraphs after an edit or a caret move.
    pub(super) fn sync_mls(&mut self) {
        let caret = self.doc.caret;
        let mut paragraphs = Vec::new();
        let mut ranges = Vec::new();
        for (range, text, ready) in self.doc.paragraphs() {
            let open = !ready || (!self.mls_fresh && range.start <= caret && caret <= range.end);
            paragraphs.push(Paragraph { text, open });
            ranges.push(range);
        }
        self.mls_lines = ranges;
        self.mls.sync(&paragraphs);
        self.mls_fresh = false;
        self.pump_mls();
        self.refresh_dots();
    }

    fn pump_mls(&mut self) {
        if let Some(job) = self.mls.next_job() {
            self.mls_worker.submit(job);
        }
    }

    /// Applies the MLS worker's replies, then queues the next job.
    pub(super) fn apply_mls_replies(&mut self) {
        while let Ok(reply) = self.mls_worker.rx.try_recv() {
            self.mls.apply(reply);
        }
        self.pump_mls();
        self.refresh_dots();
        if self.panel.is_some() {
            self.page_dirty = true;
        }
        self.redraw();
    }

    /// Rebuilds the dots from MLS, starting a fade wherever a dot's colour or presence changed.
    fn refresh_dots(&mut self) {
        let now = Instant::now();
        let dots = self.mls.dots();
        let mut started = false;
        for dot in &dots {
            let to = Some(status_color(dot.status));
            if let Some(fade) = self.fades.get_mut(&dot.key)
                && fade.to == to
            {
                fade.paragraph = dot.paragraph;
                continue;
            }
            let from = self.fades.get(&dot.key).and_then(|fade| shown(fade, now).0);
            self.fades.insert(
                dot.key,
                Fade {
                    paragraph: dot.paragraph,
                    from,
                    to,
                    start: now,
                },
            );
            started = true;
        }
        let live: HashSet<u64> = dots.iter().map(|dot| dot.key).collect();
        for (key, fade) in &mut self.fades {
            if fade.to.is_some() && !live.contains(key) {
                fade.from = shown(fade, now).0;
                fade.to = None;
                fade.start = now;
                started = true;
            }
        }
        self.fades
            .retain(|_, fade| fade.to.is_some() || progress(fade, now) < 1.0);
        if self
            .panel
            .as_ref()
            .is_some_and(|panel| !live.contains(&panel.key))
        {
            self.panel = None;
            self.page_dirty = true;
        }
        self.dots = dots;
        if started {
            self.redraw();
        }
    }

    /// True while a dot is mid-fade.
    pub(super) fn fading(&self) -> bool {
        let now = Instant::now();
        self.fades.values().any(|fade| progress(fade, now) < 1.0)
    }

    /// Requests a frame at most every FRAME while a dot fades. Returns when the next one is due.
    pub(super) fn animate(&mut self) -> Option<Instant> {
        if !self.fading() {
            self.frame_due = None;
            return None;
        }
        let now = Instant::now();
        let due = *self.frame_due.get_or_insert(now);
        if now >= due {
            self.frame_due = Some(now + FRAME);
            self.redraw();
        }
        self.frame_due
    }

    fn redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn repaint(&mut self) {
        self.page_dirty = true;
        self.redraw();
    }

    fn show(&mut self, panel: Option<Panel>) {
        self.panel = panel;
        self.repaint();
    }

    /// The top and height, in logical window coordinates, of a paragraph's first visual row.
    fn first_row(&self, paragraph: usize) -> Option<(f32, f32)> {
        let start = self.mls_lines.get(paragraph)?.start;
        let lines = &self.layout.lines;
        let line = lines.get(lines.partition_point(|line| line.start < start))?;
        (line.start == start).then_some((TOP + line.top - self.scroll, line.height))
    }

    /// The centre of a paragraph's dot, in logical window coordinates.
    fn dot_center(&self, paragraph: usize, left: f32) -> Option<(f32, f32)> {
        let (top, height) = self.first_row(paragraph)?;
        // The minimum keeps the dot on screen in a very narrow window.
        Some((
            (left - DOT_GUTTER).max(DOT_RADIUS + 6.0),
            top + height / 2.0,
        ))
    }

    /// Paints the dots that are visible, or fading, over the frame.
    pub(super) fn draw_dots(
        &self,
        pixels: &mut [u32],
        width: usize,
        height: usize,
        scale: f32,
        left: f32,
    ) {
        let now = Instant::now();
        let view = height as f32 / scale;
        for fade in self.fades.values() {
            let (Some(color), alpha) = shown(fade, now) else {
                continue;
            };
            if alpha <= 0.0 {
                continue;
            }
            let Some((x, y)) = self.dot_center(fade.paragraph, left) else {
                continue;
            };
            if y + DOT_RADIUS < 0.0 || y - DOT_RADIUS > view {
                continue;
            }
            render::dot(
                pixels,
                width,
                height,
                x * scale,
                y * scale,
                DOT_RADIUS * scale,
                color,
                alpha,
            );
        }
    }

    /// The key of the dot whose centre is within HIT_RADIUS of (x, y), in logical window coordinates.
    fn dot_at(&self, x: f32, y: f32) -> Option<u64> {
        let left = self.dimensions().3;
        self.dots
            .iter()
            .find(|dot| {
                self.dot_center(dot.paragraph, left)
                    .is_some_and(|(cx, cy)| (cx - x).hypot(cy - y) <= HIT_RADIUS)
            })
            .map(|dot| dot.key)
    }

    /// The panel's rows in order, and the index of the Lean toggle among them.
    fn panel_rows(&self, key: u64, show_lean: bool) -> Option<(Vec<PanelRow>, Option<usize>)> {
        let details = self.mls.details(key)?;
        let status = self.dots.iter().find(|dot| dot.key == key)?.status;
        if details.service
            && let Some(error) = &details.error
        {
            let text = row(error.clone(), false, PANEL_TEXT_SCALE, TEXT_COLOR, 0.0);
            return Some((vec![text], None));
        }
        let mut rows = Vec::new();
        if details.assumptions.is_empty() {
            let text = if status == Status::Checking {
                "Checking…"
            } else {
                "No assumptions."
            };
            rows.push(row(
                text.to_owned(),
                false,
                PANEL_TEXT_SCALE,
                MARKER_COLOR,
                0.0,
            ));
        }
        for assumption in &details.assumptions {
            rows.push(row(
                assumption.clone(),
                false,
                PANEL_TEXT_SCALE,
                QUOTE_COLOR,
                GAP,
            ));
        }
        let mut toggle = None;
        if details.lean.is_some() || details.error.is_some() {
            let failed = status == Status::Failed && details.error.is_some();
            let label = match (show_lean, failed) {
                (false, false) => "Show Lean",
                (true, false) => "Hide Lean",
                (false, true) => "Show Lean and error",
                (true, true) => "Hide Lean and error",
            };
            toggle = Some(rows.len());
            rows.push(row(
                label.to_owned(),
                false,
                PANEL_CODE_SCALE,
                MARKER_COLOR,
                SECTION_GAP,
            ));
        }
        if show_lean {
            let mut gap = SECTION_GAP;
            if let Some(lean) = &details.lean {
                rows.push(row(lean.clone(), true, PANEL_CODE_SCALE, CODE_COLOR, gap));
                gap = GAP;
            }
            if let Some(error) = &details.error {
                rows.push(row(
                    error.clone(),
                    true,
                    PANEL_CODE_SCALE,
                    PANEL_ERROR_COLOR,
                    gap,
                ));
            }
        }
        Some((rows, toggle))
    }

    /// Paints the open panel over the cached page, beside the dot's row, and records its areas.
    pub(super) fn draw_panel(
        &mut self,
        width: usize,
        height: usize,
        scale: f32,
        left: f32,
        column: f32,
    ) {
        let Some(panel) = self.panel.as_ref() else {
            return;
        };
        let (key, show_lean) = (panel.key, panel.show_lean);
        let Some(paragraph) = self
            .dots
            .iter()
            .find(|dot| dot.key == key)
            .map(|dot| dot.paragraph)
        else {
            return;
        };
        let Some((row_top, row_height)) = self.first_row(paragraph) else {
            return;
        };
        let Some((rows, toggle)) = self.panel_rows(key, show_lean) else {
            return;
        };
        let inner = column.clamp(120.0, PANEL_MAX_WIDTH - 2.0 * PANEL_PADDING);
        // Lay every row out before painting, so the page and the typesetter are not borrowed together.
        let mut laid = Vec::with_capacity(rows.len());
        for row in &rows {
            laid.push(
                self.typesetter
                    .plain(&row.text, inner, row.mono, row.scale, row.color),
            );
        }
        let mut tops = Vec::with_capacity(rows.len());
        let mut content = 0.0;
        for (row, layout) in rows.iter().zip(&laid) {
            if !tops.is_empty() {
                content += row.gap;
            }
            tops.push(content);
            content += layout.height();
        }
        let (w, h) = (inner + 2.0 * PANEL_PADDING, content + 2.0 * PANEL_PADDING);
        let view = height as f32 / scale;
        let mut y = row_top + row_height + GAP;
        if y + h > view - EDGE {
            y = (row_top - GAP - h).max(EDGE);
        }
        let x = left - PANEL_PADDING;
        render::rounded_rect(
            &mut self.pixels,
            width,
            height,
            x * scale,
            y * scale,
            w * scale,
            h * scale,
            PANEL_RADIUS * scale,
            PANEL_COLOR,
        );
        for (layout, top) in laid.iter().zip(&tops) {
            render::paint(
                &mut self.pixels,
                width,
                height,
                scale,
                layout,
                &mut self.typesetter,
                &mut self.cache,
                x + PANEL_PADDING,
                y + PANEL_PADDING + top,
            );
        }
        let toggle = toggle.map(|index| {
            (
                x + PANEL_PADDING,
                y + PANEL_PADDING + tops[index],
                inner,
                laid[index].height(),
            )
        });
        if let Some(panel) = self.panel.as_mut() {
            panel.area = (x, y, w, h);
            panel.toggle = toggle;
        }
    }

    fn set_pointer(&mut self, pointer: bool) {
        if pointer != self.pointer {
            self.pointer = pointer;
            if let Some(window) = &self.window {
                window.set_cursor(if pointer {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::Default
                });
            }
        }
    }

    /// Hover opens the panel for a dot, and closes an unpinned one once the pointer leaves it.
    pub(super) fn mls_hover(&mut self) {
        self.refresh_layout();
        let (x, y) = self.mouse;
        let dot = self.dot_at(x, y);
        let open = self
            .panel
            .as_ref()
            .map(|panel| (panel.key, panel.pinned, contains(bridged(panel.area), x, y)));
        match (open, dot) {
            (Some((_, true, _)), _) => {}
            (Some((key, false, _)), Some(dot)) if key == dot => {}
            (_, Some(dot)) => self.show(Some(Panel::new(dot, false))),
            (Some((_, false, false)), None) => self.show(None),
            _ => {}
        }
        let over_toggle = self
            .panel
            .as_ref()
            .and_then(|panel| panel.toggle)
            .is_some_and(|rect| contains(rect, x, y));
        self.set_pointer(dot.is_some() || over_toggle);
    }

    /// A click on a dot pins its panel. A click on the toggle flips the Lean. Returns true when the
    /// click was used and should not move the caret.
    pub(super) fn mls_click(&mut self) -> bool {
        self.refresh_layout();
        let (x, y) = self.mouse;
        if let Some(key) = self.dot_at(x, y) {
            let closing = self
                .panel
                .as_ref()
                .is_some_and(|panel| panel.pinned && panel.key == key);
            self.show(if closing {
                None
            } else {
                Some(Panel::new(key, true))
            });
            return true;
        }
        let Some(panel) = self.panel.as_mut() else {
            return false;
        };
        if panel.toggle.is_some_and(|rect| contains(rect, x, y)) {
            panel.show_lean = !panel.show_lean;
            self.repaint();
            return true;
        }
        if contains(panel.area, x, y) {
            return true;
        }
        if panel.pinned {
            self.show(None);
        }
        false
    }

    /// Closes the panel, pinned or not. Returns false when none was open.
    pub(super) fn close_panel(&mut self) -> bool {
        if self.panel.is_none() {
            return false;
        }
        self.show(None);
        true
    }

    pub(super) fn reset_mls(&mut self) {
        self.mls = Mls::new();
        self.dots.clear();
        self.fades.clear();
        self.panel = None;
        self.mls_lines.clear();
        self.mls_fresh = true;
    }

    /// Writes the Lean beside the saved document, when any block has Lean.
    pub(super) fn save_lean(&self, path: &Path) {
        let lean = self.mls.lean_file();
        if !lean.contains("-- mathstral:block") {
            return;
        }
        if let Err(error) = file::write(&path.with_extension("lean"), lean) {
            file::error("Could not save the Lean file.", error);
        }
    }
}

fn status_color(status: Status) -> u32 {
    match status {
        Status::Checking => DOT_CHECKING,
        Status::Sorry => DOT_SORRY,
        Status::Verified => DOT_VERIFIED,
        Status::Failed => DOT_FAILED,
    }
}

/// How far a fade has run: 0 at its start, 1 once DOT_FADE_MS has passed.
fn progress(fade: &Fade, now: Instant) -> f32 {
    let elapsed = now.saturating_duration_since(fade.start).as_secs_f32();
    (elapsed * 1000.0 / DOT_FADE_MS as f32).min(1.0)
}

/// The colour and alpha a dot shows at `now`. A change of colour crossfades; a dot that appears
/// or goes away fades its alpha.
fn shown(fade: &Fade, now: Instant) -> (Option<u32>, f32) {
    let t = 1.0 - (1.0 - progress(fade, now)).powi(3);
    match (fade.from, fade.to) {
        (Some(from), Some(to)) => (Some(mix(from, to, t)), 1.0),
        (None, Some(to)) => (Some(to), t),
        (Some(from), None) => (Some(from), 1.0 - t),
        (None, None) => (None, 0.0),
    }
}

/// Blends two colours channel by channel: `t` 0 is `from`, 1 is `to`.
fn mix(from: u32, to: u32, t: f32) -> u32 {
    let channel = |shift: u32| {
        let a = ((from >> shift) & 255) as f32;
        let b = ((to >> shift) & 255) as f32;
        (a + (b - a) * t).round() as u32
    };
    channel(16) << 16 | channel(8) << 8 | channel(0)
}

fn contains((left, top, w, h): (f32, f32, f32, f32), x: f32, y: f32) -> bool {
    x >= left && x < left + w && y >= top && y < top + h
}

/// A panel's area extended up and down, so the pointer can cross the gap from a dot to its panel.
fn bridged((left, top, w, h): (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
    (left, top - BRIDGE, w, h + 2.0 * BRIDGE)
}

#[cfg(test)]
#[path = "../../tests/unit/app/mls.rs"]
mod tests;
