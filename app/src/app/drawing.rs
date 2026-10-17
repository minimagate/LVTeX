//! Present the page, selection, carets and status in the native window.
use super::*;
use std::num::NonZeroU32;

impl App {
    pub(super) fn draw(&mut self) {
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
