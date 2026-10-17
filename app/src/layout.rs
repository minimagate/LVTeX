use crate::{
    buffer::{ATOM, Document},
    config::*,
    math::FormulaImage,
};
use cosmic_text::{
    Attrs, Buffer, Family, FontSystem, LayoutGlyph, Metrics, Shaping, Style, Weight, Wrap,
};
use std::collections::HashMap;
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone)]
struct Word {
    glyphs: Vec<LayoutGlyph>,
    width: f32,
    baseline: f32,
    stops: Vec<(usize, f32)>,
}
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
}
pub struct Typesetter {
    pub fonts: FontSystem,
    cache: HashMap<(String, bool), Word>,
}
impl Typesetter {
    pub fn new() -> Self {
        let mut fonts = FontSystem::new();
        fonts.db_mut().load_font_data(
            include_bytes!("../../assets/fonts/LibertinusSerif-Regular.otf").to_vec(),
        );
        fonts.db_mut().load_font_data(
            include_bytes!("../../assets/fonts/LibertinusSerif-Italic.otf").to_vec(),
        );
        Self {
            fonts,
            cache: HashMap::new(),
        }
    }
    fn word(&mut self, text: &str, italic: bool) -> Word {
        let key = (text.to_owned(), italic);
        if let Some(word) = self.cache.get(&key) {
            return word.clone();
        }
        let mut b = Buffer::new(&mut self.fonts, Metrics::new(FONT_SIZE, LINE_HEIGHT));
        b.set_wrap(Wrap::None);
        b.set_size(None, None);
        b.set_text(
            text,
            &Attrs::new()
                .family(Family::Name(FONT_FAMILY))
                .weight(Weight::NORMAL)
                .style(if italic { Style::Italic } else { Style::Normal }),
            Shaping::Advanced,
            None,
        );
        b.shape_until_scroll(&mut self.fonts, false);
        let (glyphs, line_w, baseline) = match b.layout_runs().next() {
            Some(run) => (run.glyphs.to_vec(), run.line_w, run.line_y),
            None => (vec![], 0.0, LINE_HEIGHT * 0.75),
        };
        // Ligatures share a cluster; interpolate only at Unicode grapheme boundaries.
        let mut stops = vec![];
        for (i, _) in text
            .grapheme_indices(true)
            .chain(std::iter::once((text.len(), "")))
        {
            let x = if i == text.len() {
                line_w
            } else {
                glyphs
                    .iter()
                    .find(|g| g.start <= i && i < g.end)
                    .map(|g| {
                        let cluster = &text[g.start..g.end];
                        let n = cluster.graphemes(true).count().max(1);
                        let k = text[g.start..i].graphemes(true).count();
                        g.x + g.w * (k as f32 / n as f32)
                    })
                    .unwrap_or(0.0)
            };
            stops.push((i, x));
        }
        let word = Word {
            glyphs,
            width: line_w,
            baseline,
            stops,
        };
        if self.cache.len() > 8192 {
            self.cache.clear();
        }
        self.cache.insert(key, word.clone());
        word
    }
    pub fn layout(&mut self, text: &str, width: f32) -> Layout {
        self.document(&Document::new(text), width)
    }
    pub fn document(&mut self, doc: &Document, width: f32) -> Layout {
        let text = doc.text();
        let open = doc.open_range();
        let mut out = Layout::default();
        let normal_baseline = self.word("M", false).baseline;
        let mut offset = 0;
        for paragraph in text.split('\n') {
            let (level, marker_end) = heading(paragraph);
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
                        });
                    } else {
                        let label = formula.label().replace('\n', " ");
                        let word = self.word(&label, false);
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
                            item.word = Some(Word {
                                glyphs,
                                width,
                                baseline: normal_baseline,
                                stops: vec![(0, 0.0), (ATOM.len(), width)],
                            });
                        } else {
                            boxes.push(BoxItem {
                                start: pos,
                                end: pos + ATOM.len(),
                                width: word.width,
                                ascent: word.baseline,
                                descent: LINE_HEIGHT - word.baseline,
                                word: Some(Word {
                                    stops: vec![(0, 0.0), (ATOM.len(), word.width)],
                                    ..word
                                }),
                                math: None,
                                scale: 1.0,
                                display,
                                color: PENDING_COLOR,
                                mark: formula.failed.then_some(FAILED_MARK_COLOR),
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
                for (i, token) in rest[..end].split_word_bound_indices() {
                    let word = self.word(token, level == 1);
                    let parts: Vec<_> = if word.width > width {
                        token.grapheme_indices(true).collect()
                    } else {
                        vec![(0, token)]
                    };
                    for (gi, piece) in parts {
                        let word = self.word(piece, level == 1);
                        let byte = pos + i + gi;
                        let active = open.as_ref().is_some_and(|r| r.contains(&byte));
                        boxes.push(BoxItem {
                            start: byte,
                            end: byte + piece.len(),
                            width: word.width,
                            ascent: word.baseline,
                            descent: LINE_HEIGHT - word.baseline,
                            word: Some(word),
                            math: None,
                            scale: 1.0,
                            display: false,
                            color: if active {
                                PENDING_COLOR
                            } else if local + i + gi < marker_end {
                                MARKER_COLOR
                            } else {
                                TEXT_COLOR
                            },
                            mark: None,
                        });
                    }
                }
                local += end;
            }
            let display = boxes.iter().any(|b| b.display);
            let mut row = Vec::new();
            let mut x = 0.0;
            let mut start = offset;
            for item in boxes {
                if x > 0.0 && x + item.width > width {
                    Self::line(
                        &mut out,
                        std::mem::take(&mut row),
                        start,
                        normal_baseline,
                        width,
                        display,
                    );
                    start = item.start;
                    x = 0.0;
                }
                x += item.width;
                row.push(item);
            }
            Self::line(&mut out, row, start, normal_baseline, width, display);
            out.lines.last_mut().unwrap().end = offset + paragraph.len();
            offset += paragraph.len() + 1;
        }
        out
    }
    fn layout_plain_label(&mut self, label: &str, width: f32) -> Layout {
        // No parsing here: a prompt may contain literal dollar signs and backslashes.
        let mut out = Layout::default();
        let mut row = Vec::new();
        let mut x = 0.0;
        let baseline = self.word("M", false).baseline;
        for (start, part) in label.split_word_bound_indices() {
            let word = self.word(part, false);
            if x > 0.0 && x + word.width > width {
                Self::line(
                    &mut out,
                    std::mem::take(&mut row),
                    0,
                    baseline,
                    width,
                    false,
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
            });
        }
        Self::line(&mut out, row, 0, baseline, width, false);
        out
    }
    fn line(
        out: &mut Layout,
        items: Vec<BoxItem>,
        start: usize,
        normal_baseline: f32,
        width: f32,
        display: bool,
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
        let mut x = if display {
            (width - items.iter().map(|i| i.width).sum::<f32>()) / 2.0
        } else {
            0.0
        };
        let mut line = Line {
            start,
            end: start,
            stops: vec![(start, x)],
            top,
            height: ascent + descent + padding * 2.0,
        };
        for item in items {
            let y = top + padding + ascent - item.ascent;
            if let Some(word) = item.word {
                out.glyphs
                    .extend(word.glyphs.into_iter().map(|glyph| Placed {
                        glyph,
                        x,
                        baseline: top + padding + ascent,
                        color: item.color,
                    }));
                for (i, dx) in word.stops {
                    if line.stops.last().is_some_and(|p| p.0 == item.start + i) {
                        line.stops.pop();
                    }
                    line.stops.push((item.start + i, x + dx));
                }
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
            x += item.width;
            line.end = item.end;
        }
        out.lines.push(line);
    }
}
impl Layout {
    #[cfg(test)]
    pub fn caret(&self, byte: usize) -> (usize, f32) {
        self.caret_on_line(byte, None)
    }
    pub fn caret_on_line(&self, byte: usize, preferred: Option<usize>) -> (usize, f32) {
        // A wrap boundary belongs to the following visual line.
        let row = preferred
            .filter(|&row| {
                self.lines
                    .get(row)
                    .is_some_and(|l| l.start <= byte && byte <= l.end)
            })
            .unwrap_or_else(|| {
                self.lines
                    .iter()
                    .rposition(|l| l.start <= byte)
                    .unwrap_or(0)
            });
        let line = &self.lines[row];
        let x = line
            .stops
            .iter()
            .find(|p| p.0 == byte)
            .or_else(|| line.stops.last())
            .map(|p| p.1)
            .unwrap_or(0.0);
        (row, x)
    }
    pub fn hit(&self, row: usize, x: f32) -> usize {
        let row = row.min(self.lines.len().saturating_sub(1));
        self.lines[row]
            .stops
            .iter()
            .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
            .map(|p| p.0)
            .unwrap_or(0)
    }
    pub fn row_at(&self, y: f32) -> usize {
        self.lines
            .iter()
            .rposition(|line| line.top <= y)
            .unwrap_or(0)
    }
    pub fn height(&self) -> f32 {
        self.lines.last().map_or(0.0, |line| line.top + line.height)
    }
}
// One title style: a single hash followed by a space or tab makes the line italic.
// Keep the marker editable and in the saved Markdown, but render it dimly.
fn heading(paragraph: &str) -> (usize, usize) {
    let indent = paragraph.bytes().take_while(|&c| c == b' ').count();
    if indent > 3 {
        return (0, 0);
    }
    let rest = &paragraph[indent..];
    let level = rest.bytes().take_while(|&c| c == b'#').count();
    if level == 1
        && rest
            .as_bytes()
            .get(level)
            .is_some_and(|c| *c == b' ' || *c == b'\t')
    {
        (level, indent + level)
    } else {
        (0, 0)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn titles_are_italic_at_body_size_and_preserve_editable_source() {
        let mut t = Typesetter::new();
        let source = "# A long title that wraps onto another line\nBody e\u{301} text\n## Subtitle\n#hashtag";
        let layout = t.layout(source, 160.0);
        assert!(layout.lines.iter().all(|line| line.height == LINE_HEIGHT));
        assert!(
            layout
                .glyphs
                .iter()
                .all(|placed| placed.glyph.font_size == FONT_SIZE)
        );
        let regular = t.word("Title", false);
        let italic = t.word("Title", true);
        assert_ne!(regular.glyphs[0].font_id, italic.glyphs[0].font_id);
        assert_eq!(layout.glyphs[0].glyph.font_id, italic.glyphs[0].font_id);
        let subtitle = t.layout("## Subtitle", 300.0);
        assert!(
            subtitle
                .glyphs
                .iter()
                .all(|placed| placed.glyph.font_id == regular.glyphs[0].font_id)
        );
        let body = source.find("Body").unwrap();
        let body_row = layout.caret(body).0;
        assert!(body_row > 1, "the heading must wrap");
        assert_eq!(layout.lines[body_row].height, LINE_HEIGHT);
        let hashtag_row = layout.caret(source.find("#hashtag").unwrap()).0;
        assert_eq!(layout.lines[hashtag_row].height, LINE_HEIGHT);
        for (row, line) in layout.lines.iter().enumerate() {
            assert_eq!(layout.row_at(line.top + line.height / 2.0), row);
            if row > 0 {
                assert_eq!(
                    line.top,
                    layout.lines[row - 1].top + layout.lines[row - 1].height
                );
            }
        }
        for (byte, _) in source.grapheme_indices(true) {
            let (row, x) = layout.caret(byte);
            assert_eq!(layout.hit(row, x), byte, "source byte {byte}");
        }
        assert_eq!(layout.glyphs[0].color, MARKER_COLOR);
        assert_eq!(heading("   # Title"), (1, 4));
        assert_eq!(heading("## Subtitle"), (0, 0));
        assert_eq!(heading("####### Title"), (0, 0));
        assert_eq!(heading("a # title"), (0, 0));
    }
    #[test]
    fn wraps_and_hits_unicode_without_splitting_clusters() {
        let mut t = Typesetter::new();
        let text = "Hello e\u{301} world\n\nlast";
        let l = t.layout(text, 65.0);
        assert!(l.lines.len() > 3);
        for (i, _) in text.grapheme_indices(true) {
            let (row, x) = l.caret(i);
            assert_eq!(l.hit(row, x), i, "byte {i}");
        }
        assert_eq!(l.caret(text.len()).0, l.lines.len() - 1);
    }
    #[test]
    fn stray_atoms_without_formulas_terminate() {
        let mut t = Typesetter::new();
        let layout = t.layout(&format!("{ATOM}a{ATOM}"), 300.0);
        assert!(!layout.lines.is_empty());
    }
}
