//! Lean checking: syncing lines to MLS, applying its replies, and the dots and panel.
use super::*;
use crate::view::layout::Line;
use mathstral_mls::{Details, Mls, Paragraph, Status};
use std::path::{Path, PathBuf};
use winit::window::CursorIcon;

/// A dot's colour on its way from one state to the next. `None` is no dot.
pub(super) struct Fade {
    paragraph: usize,
    from: Option<u32>,
    to: Option<u32>,
    start: Instant,
    /// The dot's centre height in document coordinates, kept from the last sync while the dot was
    /// live. A dot that goes away fades out there.
    anchor: Option<f32>,
}

/// The open panel, for the dot with `key`. Areas are (x, y, w, h) in logical window coordinates,
/// as the last draw left them.
pub(super) struct Panel {
    key: u64,
    pinned: bool,
    show_lean: bool,
    area: (f32, f32, f32, f32),
    /// The area that keeps an unpinned panel open: the dot's side of the paragraph, from its first
    /// row to the panel, with a margin.
    hold: (f32, f32, f32, f32),
    toggle: Option<(f32, f32, f32, f32)>,
}

impl Panel {
    fn new(key: u64, pinned: bool) -> Self {
        Self {
            key,
            pinned,
            show_lean: false,
            area: (0.0, 0.0, 0.0, 0.0),
            hold: (0.0, 0.0, 0.0, 0.0),
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

/// The heading of an item in a dot over several: its capitalised label and its status in words. An
/// item with no label, such as a failed classification, shows the status alone.
fn item_heading(item: &Details) -> String {
    let status = match item.status {
        Status::Checking => "checking",
        Status::Sorry => "not proved yet",
        Status::Verified => "verified",
        Status::Failed => "does not check",
    };
    match item.label {
        Some(label) => {
            let name = label.name();
            format!("{}{} · {status}", name[..1].to_uppercase(), &name[1..])
        }
        None => status.to_owned(),
    }
}

/// The Lake project for the REPL, chosen by `choose_lean_project`.
pub(super) fn lean_project() -> PathBuf {
    choose_lean_project(
        std::env::var_os("MATHSTRAL_LEAN_PROJECT").map(PathBuf::from),
        file::lean_dir(),
        PathBuf::from(LEAN_PROJECT),
        |path| path.is_dir(),
    )
}

/// Chooses the Lake project: `MATHSTRAL_LEAN_PROJECT` when set; else the per-user copy an installed
/// app uses, when it exists; else the checkout's `lean/`, when it exists. Otherwise the per-user path,
/// so the REPL's "No Lean project at …" message names where to put it. `user` is None only without
/// HOME, and then the checkout is the fallback.
fn choose_lean_project(
    env: Option<PathBuf>,
    user: Option<PathBuf>,
    checkout: PathBuf,
    exists: impl Fn(&Path) -> bool,
) -> PathBuf {
    match (env, user) {
        (Some(env), _) => env,
        (None, Some(user)) if exists(&user) => user,
        (None, _) if exists(&checkout) => checkout,
        (None, user) => user.unwrap_or(checkout),
    }
}

impl App {
    /// Gives MLS the document's paragraphs after an edit or a caret move.
    pub(super) fn sync_mls(&mut self) {
        if !self.mls_on {
            self.dots.clear();
            self.fades.clear();
            self.panel = None;
            return;
        }
        // The old paragraph indices still match `mls_lines` and the layout, so read the anchors now.
        // A dot that has already gone keeps its anchor: its index no longer names its paragraph.
        for fade in self.fades.values_mut().filter(|fade| fade.to.is_some()) {
            fade.anchor = dot_anchor(&self.mls_lines, &self.layout, fade.paragraph);
        }
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
        if !self.mls_on {
            return;
        }
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
        if self.panel.is_some() || self.chart.is_some() {
            self.page_dirty = true;
        }
        self.redraw();
    }

    /// Rebuilds the dots from MLS, starting a fade wherever a dot's colour or presence changed.
    fn refresh_dots(&mut self) {
        // Replies still arrive while off: they must not bring the dots back.
        if !self.mls_on {
            return;
        }
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
            let prior = self.fades.get(&dot.key);
            let from = prior.and_then(|fade| shown(fade, now).0);
            let anchor = prior.and_then(|fade| fade.anchor);
            self.fades.insert(
                dot.key,
                Fade {
                    paragraph: dot.paragraph,
                    from,
                    to,
                    start: now,
                    anchor,
                },
            );
            started = true;
        }
        let live: HashSet<u64> = dots.iter().map(|dot| dot.key).collect();
        for (key, fade) in &mut self.fades {
            if fade.to.is_some() && !live.contains(key) {
                // A dot started since the last sync has no anchor yet. Between syncs the layout still
                // matches `mls_lines`, so it can take one now; otherwise it would vanish without a fade.
                if fade.anchor.is_none() && !self.layout_dirty {
                    fade.anchor = dot_anchor(&self.mls_lines, &self.layout, fade.paragraph);
                }
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
            self.frame_due = Some(now + Duration::from_millis(DOT_FRAME_MS));
            self.redraw();
        }
        self.frame_due
    }

    fn redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    pub(super) fn repaint(&mut self) {
        self.page_dirty = true;
        self.redraw();
    }

    fn show(&mut self, panel: Option<Panel>) {
        self.panel = panel;
        self.repaint();
    }

    /// The top and height, in logical window coordinates, of a paragraph's first visual row.
    fn first_row(&self, paragraph: usize) -> Option<(f32, f32)> {
        let (top, height) = document_row(&self.mls_lines, &self.layout, paragraph)?;
        Some((TOP + top - self.scroll, height))
    }

    /// The top and height, in logical window coordinates, of a paragraph's last visual row.
    fn last_row(&self, paragraph: usize) -> Option<(f32, f32)> {
        let (top, height) = last_document_row(&self.mls_lines, &self.layout, paragraph)?;
        Some((TOP + top - self.scroll, height))
    }

    /// The x of every dot's centre, in logical window coordinates.
    fn dot_x(left: f32) -> f32 {
        // The minimum keeps the dot on screen in a very narrow window.
        (left - DOT_GUTTER).max(DOT_RADIUS + DOT_EDGE)
    }

    /// The centre of a paragraph's dot, in logical window coordinates.
    fn dot_center(&self, paragraph: usize, left: f32) -> Option<(f32, f32)> {
        let (top, height) = self.first_row(paragraph)?;
        Some((Self::dot_x(left), top + height / 2.0))
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
            let center = match fade.to {
                Some(_) => self.dot_center(fade.paragraph, left),
                // A dot that has gone stays where it was, whatever its index now names.
                None => fade
                    .anchor
                    .map(|anchor| (Self::dot_x(left), TOP + anchor - self.scroll)),
            };
            let Some((x, y)) = center else {
                continue;
            };
            if y + DOT_RADIUS < 0.0 || y - DOT_RADIUS > view {
                continue;
            }
            if self.dot_covered(x, y) {
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
    /// A dot hidden under the panel, the chart or Settings is not hit.
    fn dot_at(&self, x: f32, y: f32) -> Option<u64> {
        let left = self.dimensions().3;
        self.dots
            .iter()
            .find(|dot| {
                self.dot_center(dot.paragraph, left)
                    .is_some_and(|(cx, cy)| {
                        !self.dot_covered(cx, cy) && (cx - x).hypot(cy - y) <= DOT_HIT_RADIUS
                    })
            })
            .map(|dot| dot.key)
    }

    /// True when a dot centred at (x, y) lies under the open panel, the chart or Settings. Drawing and
    /// hit tests both use it, so a hidden dot neither shows nor takes the pointer.
    fn dot_covered(&self, x: f32, y: f32) -> bool {
        panel_covers(self.panel.as_ref(), x, y)
            || self.chart_covers(x, y)
            || self.settings_covers(x, y)
    }

    /// The panel's rows in order, and the index of the Lean toggle among them. A dot over several
    /// items lists each with a heading (its label and status); one item has none.
    fn panel_rows(&self, key: u64, show_lean: bool) -> Option<(Vec<PanelRow>, Option<usize>)> {
        let items = self.mls.details(key);
        if items.is_empty() {
            return None;
        }
        if let Some(error) = items
            .iter()
            .find(|item| item.service)
            .and_then(|item| item.error.as_ref())
        {
            let text = row(error.clone(), false, PANEL_TEXT_SCALE, TEXT_COLOR, 0.0);
            return Some((vec![text], None));
        }
        let several = items.len() > 1;
        let mut rows = Vec::new();
        for (index, item) in items.iter().enumerate() {
            // Under a heading the first row sits a line gap below it.
            let mut gap = 0.0;
            if several {
                let section = if index > 0 { PANEL_SECTION_GAP } else { 0.0 };
                rows.push(row(
                    item_heading(item),
                    false,
                    PANEL_TEXT_SCALE,
                    MARKER_COLOR,
                    section,
                ));
                gap = PANEL_GAP;
            }
            if let Some(note) = &item.note {
                rows.push(row(note.clone(), false, PANEL_TEXT_SCALE, TEXT_COLOR, gap));
            } else if item.assumptions.is_empty() {
                let text = if item.status == Status::Checking {
                    "Checking…"
                } else {
                    "No assumptions."
                };
                rows.push(row(
                    text.to_owned(),
                    false,
                    PANEL_TEXT_SCALE,
                    MARKER_COLOR,
                    gap,
                ));
            }
            for assumption in &item.assumptions {
                rows.push(row(
                    assumption.clone(),
                    false,
                    PANEL_TEXT_SCALE,
                    QUOTE_COLOR,
                    PANEL_GAP,
                ));
            }
            if !item.uses.is_empty() {
                rows.push(row(
                    format!("Uses {}.", item.uses.join(", ")),
                    false,
                    PANEL_TEXT_SCALE,
                    MARKER_COLOR,
                    PANEL_SECTION_GAP,
                ));
            }
        }
        let mut toggle = None;
        if items
            .iter()
            .any(|item| item.lean.is_some() || item.error.is_some())
        {
            let failed = items
                .iter()
                .any(|item| item.status == Status::Failed && item.error.is_some());
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
                PANEL_SECTION_GAP,
            ));
        }
        if show_lean {
            for item in &items {
                let mut gap = PANEL_SECTION_GAP;
                if let Some(lean) = &item.lean {
                    rows.push(row(lean.clone(), true, PANEL_CODE_SCALE, CODE_COLOR, gap));
                    gap = PANEL_GAP;
                }
                if let Some(error) = &item.error {
                    rows.push(row(
                        error.clone(),
                        true,
                        PANEL_CODE_SCALE,
                        PANEL_ERROR_COLOR,
                        gap,
                    ));
                }
            }
        }
        Some((rows, toggle))
    }

    /// Paints the open panel over the cached page, below the dot's paragraph, and records its areas.
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
        let Some((paragraph, last)) = self
            .dots
            .iter()
            .find(|dot| dot.key == key)
            .map(|dot| (dot.paragraph, dot.last))
        else {
            return;
        };
        let Some((row_top, row_height)) = self.first_row(paragraph) else {
            return;
        };
        let Some((last_top, last_height)) = self.last_row(last) else {
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
        let mut y = last_top + last_height + PANEL_GAP;
        let below = y + h <= view - PANEL_EDGE;
        if !below {
            y = (row_top - PANEL_GAP - h).max(PANEL_EDGE);
        }
        let x = left - PANEL_PADDING;
        // The hold runs from the dot's side of the paragraph to the panel, so the pointer can cross
        // the text between them. Below the paragraph it spans from the first row's top to the panel's
        // bottom; placed above, from the panel's top to the first row's bottom.
        let (hold_top, hold_bottom) = if below {
            (row_top, y + h)
        } else {
            (y, row_top + row_height)
        };
        let hold_left = Self::dot_x(left) - DOT_HIT_RADIUS;
        let hold = (
            hold_left,
            hold_top - PANEL_BRIDGE,
            x + w - hold_left,
            hold_bottom - hold_top + 2.0 * PANEL_BRIDGE,
        );
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
            panel.hold = hold;
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
        // While Settings is open, the pointer over the page does nothing.
        if self.settings.is_some() {
            self.set_pointer(false);
            return;
        }
        self.refresh_layout();
        let (x, y) = self.mouse;
        let dot = if self.chart_covers(x, y) {
            None
        } else {
            self.dot_at(x, y)
        };
        let open = self.panel.as_ref().map(|panel| {
            // A new panel has no area until it is first drawn, so the pointer that opened it is not
            // inside it yet. It is still opening, so it stays.
            let opening = panel.area.2 == 0.0;
            let keep = opening || contains(panel.hold, x, y);
            (panel.key, panel.pinned, keep)
        });
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
        let over_row = self.chart_paragraph_at(x, y).is_some();
        self.set_pointer(dot.is_some() || over_toggle || over_row);
    }

    /// A click on a dot pins its panel. A click on the toggle flips the Lean. Returns true when the
    /// click was used and should not move the caret.
    pub(super) fn mls_click(&mut self) -> bool {
        self.refresh_layout();
        let (x, y) = self.mouse;
        if self.chart_covers(x, y) {
            // A row scrolls its paragraph to 48 pt below the top, as the reveal does for the caret.
            if let Some((top, _)) = self
                .chart_paragraph_at(x, y)
                .and_then(|paragraph| document_row(&self.mls_lines, &self.layout, paragraph))
            {
                self.scroll = (TOP + top - 48.0).max(0.0);
                self.repaint();
            }
            return true;
        }
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

    /// Turns Lean checking on or off and remembers the choice. Off checks nothing and shows no dots.
    /// On checks every paragraph again, the caret's included.
    pub(super) fn set_mls_on(&mut self, on: bool) {
        self.mls_on = on;
        file::remember_mls_enabled(on);
        if on {
            self.mls_fresh = true;
            self.mls.retry();
        }
        self.sync_mls();
        self.repaint();
    }

    pub(super) fn reset_mls(&mut self) {
        self.mls.reset();
        self.dots.clear();
        self.fades.clear();
        self.panel = None;
        self.mls_lines.clear();
        self.mls_fresh = true;
    }

    /// Writes the document's Lean twin, at `lean_path`: the Lean that checked and what MLS
    /// remembers (see `Mls::twin`). It is written when there is something to remember, or when
    /// that file already exists, so it follows the last removal. Otherwise nothing is written.
    pub(super) fn save_lean(&self, document: &Path) {
        let path = twin_path(document);
        let twin = self.mls.twin();
        // An empty document's twin has no blocks and no memory.
        if twin == Mls::new().twin() && !path.exists() {
            return;
        }
        if let Err(error) = file::write(&path, twin) {
            file::error("Could not save the Lean file.", error);
        }
    }

    /// Restores what MLS remembers from the document's Lean twin, when there is one. Call it on a
    /// fresh or reset MLS, before the first sync. A missing or unreadable twin changes nothing.
    pub(super) fn restore_mls(&mut self, document: &Path) {
        if let Ok(twin) = file::read(&twin_path(document)) {
            self.mls.restore(&twin);
        }
    }
}

/// The document's Lean twin, beside the document as it is on disk.
fn twin_path(document: &Path) -> PathBuf {
    let document = std::fs::canonicalize(document).unwrap_or_else(|_| document.to_path_buf());
    lean_path(&document)
}

/// The Lean file beside a document: `notes.md` gives `notes.mls.lean`. The `.mls` infix keeps it
/// from being the document itself, or a `Main.lean` the user keeps beside `Main.md`.
fn lean_path(document: &Path) -> PathBuf {
    let stem = document
        .file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
    document.with_file_name(format!("{stem}.mls.lean"))
}

/// The layout's visual rows of a paragraph: those that start inside its byte range, in order.
/// `lines` are the paragraphs' byte ranges, and `layout` must be typeset from the same text.
fn paragraph_rows<'a>(lines: &[Range<usize>], layout: &'a Layout, paragraph: usize) -> &'a [Line] {
    let Some(range) = lines.get(paragraph) else {
        return &[];
    };
    let rows = &layout.lines;
    let first = rows.partition_point(|line| line.start < range.start);
    let count = rows[first..].partition_point(|line| line.start <= range.end);
    &rows[first..first + count]
}

/// The top and height, in document coordinates, of a paragraph's first visual row.
fn document_row(lines: &[Range<usize>], layout: &Layout, paragraph: usize) -> Option<(f32, f32)> {
    let start = lines.get(paragraph)?.start;
    let row = paragraph_rows(lines, layout, paragraph).first()?;
    (row.start == start).then_some((row.top, row.height))
}

/// A dot's anchor: the centre height, in document coordinates, of its paragraph's first visual row.
fn dot_anchor(lines: &[Range<usize>], layout: &Layout, paragraph: usize) -> Option<f32> {
    document_row(lines, layout, paragraph).map(|(top, height)| top + height / 2.0)
}

/// The top and height, in document coordinates, of a paragraph's last visual row.
fn last_document_row(
    lines: &[Range<usize>],
    layout: &Layout,
    paragraph: usize,
) -> Option<(f32, f32)> {
    let row = paragraph_rows(lines, layout, paragraph).last()?;
    Some((row.top, row.height))
}

pub(super) fn status_color(status: Status) -> u32 {
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

pub(super) fn contains((left, top, w, h): (f32, f32, f32, f32), x: f32, y: f32) -> bool {
    x >= left && x < left + w && y >= top && y < top + h
}

/// True when (x, y) lies in the open panel's drawn area. A panel that has not been drawn has none.
fn panel_covers(panel: Option<&Panel>, x: f32, y: f32) -> bool {
    panel.is_some_and(|panel| panel.area.2 > 0.0 && contains(panel.area, x, y))
}

#[cfg(test)]
#[path = "../../tests/unit/app/mls.rs"]
mod tests;
