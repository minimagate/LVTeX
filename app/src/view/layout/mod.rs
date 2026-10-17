//! Document typesetting and visual rows, in logical pixels.
mod geometry;
mod paragraph;
mod shaping;

use crate::{
    config::*,
    document::{ATOM, Document},
    markdown::{self, Block, Span},
    math::render::FormulaImage,
};
use cosmic_text::LayoutGlyph;
use paragraph::{Para, prompt_alone};
pub use shaping::Typesetter;
use shaping::{Face, Word};
use std::ops::Range;
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;

// Strikethrough height above the baseline, in font sizes.
const STRIKE_LIFT: f32 = 0.28;

pub struct Placed {
    pub glyph: LayoutGlyph,
    pub x: f32,
    pub baseline: f32,
    pub color: u32,
}
#[derive(Default)]
pub struct Line {
    pub start: usize,
    pub end: usize,
    pub stops: Vec<(usize, f32)>,
    pub top: f32,
    pub height: f32,
    baseline: f32,
    // Caret extent (ascent, descent) after each grapheme or formula that ends at a byte.
    carets: Vec<(usize, (f32, f32))>,
    // Caret extent at the line start, before any letter.
    start_caret: (f32, f32),
}
#[derive(Default)]
pub struct Layout {
    pub glyphs: Vec<Placed>,
    pub lines: Vec<Line>,
    pub math: Vec<PlacedMath>,
    pub marks: Vec<Mark>,
}
pub struct PlacedMath {
    pub image: Arc<FormulaImage>,
    pub display: bool,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
}
pub struct Mark {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub color: u32,
}
struct BoxItem {
    start: usize,
    end: usize,
    width: f32,
    ascent: f32,
    descent: f32,
    word: Option<Word>,
    math: Option<Arc<FormulaImage>>,
    scale: f32,
    display: bool,
    color: u32,
    mark: Option<u32>,
    strike: bool,
}
impl Typesetter {
    pub fn layout(&mut self, text: &str, width: f32) -> Layout {
        self.document(&Document::new(text), width)
    }
    pub fn document(&mut self, doc: &Document, width: f32) -> Layout {
        let text = doc.text();
        let open = doc.open_range();
        let mut out = Layout::default();
        let normal_baseline = self.word("M", Face::Regular).baseline;
        let cap = (self.references(Face::Regular).1, 0.0);
        let mut offset = 0;
        for paragraph in text.split('\n') {
            let para = Para::new(paragraph, offset, open.clone());
            let mut boxes = Vec::new();
            let mut local = 0;
            while local < paragraph.len() {
                let pos = offset + local;
                if let Some(formula) = doc.formulas.get(&pos) {
                    let display = Document::is_display_in(&text, pos);
                    if let Some(image) = &formula.image {
                        let bitmap = if display {
                            &image.display
                        } else {
                            &image.inline
                        };
                        let scale = (width / bitmap.width()).min(1.0);
                        boxes.push(BoxItem {
                            start: pos,
                            end: pos + ATOM.len(),
                            width: bitmap.width() * scale,
                            ascent: bitmap.baseline * scale,
                            descent: (bitmap.height() - bitmap.baseline) * scale,
                            word: None,
                            math: Some(image.clone()),
                            scale,
                            display,
                            color: TEXT_COLOR,
                            mark: None,
                            strike: false,
                        });
                    } else {
                        let label = formula.label().replace('\n', " ");
                        let word = self.word(&label, Face::Regular);
                        // Pending prompts remain readable and wrap as text inside a single atom.
                        let pending = if word.width > width {
                            Some(self.layout_plain_label(&label, width))
                        } else {
                            None
                        };
                        if let Some(nested) = pending {
                            let height = nested.height();
                            boxes.push(BoxItem {
                                start: pos,
                                end: pos + ATOM.len(),
                                width,
                                ascent: normal_baseline,
                                descent: height - normal_baseline,
                                word: None,
                                math: None,
                                scale: 1.0,
                                display,
                                color: PENDING_COLOR,
                                mark: formula.failed.then_some(FAILED_MARK_COLOR),
                                strike: false,
                            });
                            // Reuse the word container for wrapped glyphs with a measured baseline.
                            let item = boxes.last_mut().unwrap();
                            let glyphs = nested
                                .glyphs
                                .into_iter()
                                .map(|p| {
                                    let mut g = p.glyph;
                                    g.x += p.x;
                                    g.y += p.baseline - normal_baseline;
                                    g
                                })
                                .collect();
                            // Wrapped labels take the caret of the regular cap height.
                            item.word = Some(Word {
                                glyphs,
                                width,
                                baseline: normal_baseline,
                                stops: vec![(0, 0.0), (ATOM.len(), width)],
                                inks: vec![cap; 2],
                            });
                        } else {
                            let (ascent, descent) = (word.baseline, LINE_HEIGHT - word.baseline);
                            // The caret after a one-line label follows its last letter, like text.
                            let inks = vec![word.inks[0], word.inks[word.inks.len() - 1]];
                            boxes.push(BoxItem {
                                start: pos,
                                end: pos + ATOM.len(),
                                width: word.width,
                                ascent,
                                descent,
                                word: Some(Word {
                                    stops: vec![(0, 0.0), (ATOM.len(), word.width)],
                                    inks,
                                    ..word
                                }),
                                math: None,
                                scale: 1.0,
                                display,
                                color: PENDING_COLOR,
                                mark: formula.failed.then_some(FAILED_MARK_COLOR),
                                strike: false,
                            });
                        }
                    }
                    local += ATOM.len();
                    continue;
                }
                let rest = &paragraph[local..];
                let end = rest.find(ATOM).unwrap_or(rest.len());
                if end == 0 {
                    local += ATOM.len();
                    continue;
                }
                self.text_boxes(
                    &para,
                    offset + local,
                    &rest[..end],
                    width,
                    normal_baseline,
                    &mut boxes,
                );
                local += end;
            }
            // A formula alone on its line, or an open prompt that is all the line's text, is centered.
            let display =
                boxes.iter().any(|b| b.display) || prompt_alone(paragraph, offset, open.as_ref());
            // Display rows stay centered, so only text paragraphs hang.
            let hang: f32 = if display || !para.hangs() {
                0.0
            } else {
                boxes
                    .iter()
                    .filter(|b| b.start < para.marker)
                    .map(|b| b.width)
                    .sum()
            };
            let mut row = Vec::new();
            let mut x = 0.0;
            let mut indent = 0.0;
            let mut start = offset;
            for item in boxes {
                if !row.is_empty() && x + item.width > width {
                    Self::line(
                        &mut out,
                        std::mem::take(&mut row),
                        start,
                        normal_baseline,
                        width,
                        display,
                        indent,
                        cap,
                    );
                    start = item.start;
                    indent = hang;
                    x = hang;
                }
                x += item.width;
                row.push(item);
            }
            Self::line(
                &mut out,
                row,
                start,
                normal_baseline,
                width,
                display,
                indent,
                cap,
            );
            out.lines.last_mut().unwrap().end = offset + paragraph.len();
            if para.block == Block::Rule {
                let last = out.lines.last().unwrap();
                let (top, height) = (last.top, last.height);
                let from = last.stops.last().map_or(0.0, |p| p.1);
                if from < width {
                    out.marks.push(Mark {
                        x: from,
                        y: top + height / 2.0,
                        width: width - from,
                        color: MARKER_COLOR,
                    });
                }
            }
            offset += paragraph.len() + 1;
        }
        out
    }
    // Text boxes for `text`, which starts at byte `start`. Styling changes only at `para.cut`
    // points, so each segment has one face and color; words split inside a segment.
    fn text_boxes(
        &mut self,
        para: &Para,
        start: usize,
        text: &str,
        width: f32,
        normal_baseline: f32,
        boxes: &mut Vec<BoxItem>,
    ) {
        let end = start + text.len();
        let mut at = start;
        while at < end {
            let next = para.cut(at).min(end);
            let (face, color, strike) = para.look(at);
            for (i, token) in text[at - start..next - start].split_word_bound_indices() {
                let parts: Vec<_> = if self.word(token, face).width > width {
                    token.grapheme_indices(true).collect()
                } else {
                    vec![(0, token)]
                };
                for (gi, piece) in parts {
                    let word = self.word(piece, face);
                    // Clamp to the normal metrics so a code span never makes a line taller.
                    let ascent = word.baseline.min(normal_baseline);
                    let descent = LINE_HEIGHT - word.baseline.max(normal_baseline);
                    boxes.push(BoxItem {
                        start: at + i + gi,
                        end: at + i + gi + piece.len(),
                        width: word.width,
                        ascent,
                        descent,
                        word: Some(word),
                        math: None,
                        scale: 1.0,
                        display: false,
                        color,
                        mark: None,
                        strike,
                    });
                }
            }
            at = next;
        }
    }
    fn layout_plain_label(&mut self, label: &str, width: f32) -> Layout {
        // No parsing here: a prompt may contain literal dollar signs and backslashes.
        let mut out = Layout::default();
        let mut row = Vec::new();
        let mut x = 0.0;
        let baseline = self.word("M", Face::Regular).baseline;
        let cap = (self.references(Face::Regular).1, 0.0);
        for (start, part) in label.split_word_bound_indices() {
            let word = self.word(part, Face::Regular);
            if x > 0.0 && x + word.width > width {
                Self::line(
                    &mut out,
                    std::mem::take(&mut row),
                    0,
                    baseline,
                    width,
                    false,
                    0.0,
                    cap,
                );
                x = 0.0;
            }
            x += word.width;
            row.push(BoxItem {
                start,
                end: start + part.len(),
                width: word.width,
                ascent: word.baseline,
                descent: LINE_HEIGHT - word.baseline,
                word: Some(word),
                math: None,
                scale: 1.0,
                display: false,
                color: PENDING_COLOR,
                mark: None,
                strike: false,
            });
        }
        Self::line(&mut out, row, 0, baseline, width, false, 0.0, cap);
        out
    }
    // Places one visual row. `indent` is where the row's first box starts, used by hanging
    // rows; centered display rows ignore it.
    #[allow(clippy::too_many_arguments)] // `cap` is the row's only extra input; a struct would wrap one call shape
    fn line(
        out: &mut Layout,
        items: Vec<BoxItem>,
        start: usize,
        normal_baseline: f32,
        width: f32,
        display: bool,
        indent: f32,
        cap: (f32, f32),
    ) {
        let ascent = items
            .iter()
            .map(|i| i.ascent)
            .fold(normal_baseline, f32::max);
        let descent = items
            .iter()
            .map(|i| i.descent)
            .fold(LINE_HEIGHT - normal_baseline, f32::max);
        let padding = if display { 12.0 } else { 0.0 };
        let top = out.height();
        let baseline = top + padding + ascent;
        let mut x = if display {
            (width - items.iter().map(|i| i.width).sum::<f32>()) / 2.0
        } else {
            indent
        };
        let mut line = Line {
            start,
            end: start,
            stops: vec![(start, x)],
            top,
            height: ascent + descent + padding * 2.0,
            baseline,
            carets: Vec::new(),
            start_caret: items
                .first()
                .and_then(|item| item.word.as_ref())
                .map_or(cap, |word| word.inks[0]),
        };
        let mut struck: Option<f32> = None;
        for item in items {
            let y = top + padding + ascent - item.ascent;
            match item.word {
                Some(word) => {
                    out.glyphs
                        .extend(word.glyphs.into_iter().map(|glyph| Placed {
                            glyph,
                            x,
                            baseline,
                            color: item.color,
                        }));
                    for (k, ((i, dx), ink)) in word.stops.into_iter().zip(word.inks).enumerate() {
                        if line.stops.last().is_some_and(|p| p.0 == item.start + i) {
                            line.stops.pop();
                        }
                        line.stops.push((item.start + i, x + dx));
                        // The first stop has no letter before it; the line start covers it.
                        if k > 0 {
                            line.carets.push((item.start + i, ink));
                        }
                    }
                }
                None => line.carets.push((item.end, (item.ascent, item.descent))),
            }
            if let Some(image) = item.math {
                if line.stops.last().is_some_and(|p| p.0 == item.start) {
                    line.stops.pop();
                }
                line.stops.push((item.start, x));
                line.stops.push((item.end, x + item.width));
                out.math.push(PlacedMath {
                    image,
                    display: item.display,
                    x,
                    y,
                    scale: item.scale,
                });
            }
            if let Some(color) = item.mark {
                out.marks.push(Mark {
                    x,
                    y: top + line.height - 3.0,
                    width: item.width,
                    color,
                });
            }
            if item.strike {
                if struck.is_none() {
                    struck = Some(x);
                }
            } else if let Some(from) = struck.take() {
                out.marks.push(strike_mark(from, x, baseline));
            }
            x += item.width;
            line.end = item.end;
        }
        if let Some(from) = struck {
            out.marks.push(strike_mark(from, x, baseline));
        }
        out.lines.push(line);
    }
}
fn strike_mark(from: f32, to: f32, baseline: f32) -> Mark {
    Mark {
        x: from,
        y: baseline - FONT_SIZE * STRIKE_LIFT,
        width: to - from,
        color: MARKER_COLOR,
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/view/layout.rs"]
mod tests;
