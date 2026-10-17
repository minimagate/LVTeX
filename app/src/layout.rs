use crate::{
    config::*,
    document::{ATOM, Document},
    markdown::{self, Block, Span},
    math::FormulaImage,
};
use cosmic_text::{
    Attrs, Buffer, Family, FontSystem, LayoutGlyph, Metrics, Shaping, Style, SwashCache,
    SwashContent, Weight, Wrap,
};
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;

// Monospace code is drawn smaller so its x-height matches Libertinus.
const MONO_SCALE: f32 = 0.85;
// Strikethrough sits this many font sizes above the baseline.
const STRIKE_LIFT: f32 = 0.28;
// Caret ink is measured at this multiple of the drawn size, for quarter-point precision.
const INK_SCALE: f32 = 4.0;

#[derive(Clone)]
struct Word {
    glyphs: Vec<LayoutGlyph>,
    width: f32,
    baseline: f32,
    stops: Vec<(usize, f32)>,
    // Caret extent (ascent, descent) at each stop: the ink of the grapheme that ends there.
    // The first entry is the cap height of the face, for a caret with no letter before it.
    inks: Vec<(f32, f32)>,
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
// Each face selects one embedded font file.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Face {
    Regular,
    Italic,
    Bold,
    BoldItalic,
    Mono,
}
fn face_of(bold: bool, italic: bool) -> Face {
    match (bold, italic) {
        (false, false) => Face::Regular,
        (false, true) => Face::Italic,
        (true, false) => Face::Bold,
        (true, true) => Face::BoldItalic,
    }
}
// One paragraph's Markdown styling. Ranges are absolute byte offsets in the document.
struct Para {
    block: Block,
    // End of the block marker; the content starts here.
    marker: usize,
    bold: bool,
    italic: bool,
    // Inline spans covering the content, in order.
    spans: Vec<Span>,
    open: Option<Range<usize>>,
}
impl Para {
    fn new(paragraph: &str, offset: usize, open: Option<Range<usize>>) -> Self {
        let line = markdown::line(paragraph);
        let content = line.content();
        let mut spans = if line.block == Block::Rule {
            Vec::new()
        } else {
            markdown::inline(&paragraph[content..])
        };
        for span in &mut spans {
            let range = span.range.clone();
            span.range = range.start + offset + content..range.end + offset + content;
        }
        let (bold, italic) = match line.block {
            Block::Heading(1) => (true, false),
            Block::Heading(2) => (false, true),
            Block::Heading(_) => (true, true),
            Block::Quote => (false, true),
            _ => (false, false),
        };
        Self {
            block: line.block,
            marker: offset + content,
            bold,
            italic,
            spans,
            open,
        }
    }
    // Wrapped rows of these blocks start under their content.
    fn hangs(&self) -> bool {
        matches!(
            self.block,
            Block::Bullet | Block::Ordered(_) | Block::Task(_) | Block::Quote
        )
    }
    // The next byte after `at` where the style may change.
    fn cut(&self, at: usize) -> usize {
        let mut cut = if at < self.marker {
            self.marker
        } else {
            self.spans
                .get(self.spans.partition_point(|s| s.range.end <= at))
                .map_or(usize::MAX, |s| s.range.end)
        };
        for edge in self.open.iter().flat_map(|r| [r.start, r.end]) {
            if edge > at {
                cut = cut.min(edge);
            }
        }
        cut
    }
    // Face, color and strikethrough for the bytes that start at `at`.
    fn look(&self, at: usize) -> (Face, u32, bool) {
        if self.open.as_ref().is_some_and(|r| r.contains(&at)) {
            return (Face::Regular, PENDING_COLOR, false);
        }
        let base = face_of(self.bold, self.italic);
        if at < self.marker {
            return (base, MARKER_COLOR, false);
        }
        let Some(span) = self
            .spans
            .get(self.spans.partition_point(|s| s.range.end <= at))
        else {
            return (base, TEXT_COLOR, false);
        };
        let style = span.style;
        let face = if style.code {
            Face::Mono
        } else {
            face_of(self.bold || style.bold, self.italic || style.italic)
        };
        let color = if span.marker {
            MARKER_COLOR
        } else if style.code {
            CODE_COLOR
        } else if style.link {
            LINK_COLOR
        } else if self.block == Block::Task(true) {
            MARKER_COLOR
        } else {
            TEXT_COLOR
        };
        (face, color, style.strike && !span.marker)
    }
}
// True when the open prompt starts in this paragraph and covers all of its non-whitespace text.
fn prompt_alone(paragraph: &str, offset: usize, open: Option<&Range<usize>>) -> bool {
    let Some(open) = open else {
        return false;
    };
    let Some(start) = open
        .start
        .checked_sub(offset)
        .filter(|&start| start <= paragraph.len())
    else {
        return false;
    };
    let end = open
        .end
        .saturating_sub(offset)
        .clamp(start, paragraph.len());
    paragraph[..start].trim().is_empty() && paragraph[end..].trim().is_empty()
}
pub struct Typesetter {
    pub fonts: FontSystem,
    swash: SwashCache,
    cache: HashMap<(String, Face), Word>,
    // Ink height of the x and the cap, per face.
    references: HashMap<Face, (f32, f32)>,
}
// Family, weight, style and size that each face is drawn with.
fn face_style(face: Face) -> (&'static str, Weight, Style, f32) {
    match face {
        Face::Regular => (FONT_FAMILY, Weight::NORMAL, Style::Normal, FONT_SIZE),
        Face::Italic => (FONT_FAMILY, Weight::NORMAL, Style::Italic, FONT_SIZE),
        Face::Bold => (FONT_FAMILY, Weight::BOLD, Style::Normal, FONT_SIZE),
        Face::BoldItalic => (FONT_FAMILY, Weight::BOLD, Style::Italic, FONT_SIZE),
        Face::Mono => (
            MONO_FONT_FAMILY,
            Weight::NORMAL,
            Style::Normal,
            FONT_SIZE * MONO_SCALE,
        ),
    }
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
        fonts
            .db_mut()
            .load_font_data(include_bytes!("../../assets/fonts/LibertinusSerif-Bold.otf").to_vec());
        fonts.db_mut().load_font_data(
            include_bytes!("../../assets/fonts/LibertinusSerif-BoldItalic.otf").to_vec(),
        );
        fonts
            .db_mut()
            .load_font_data(include_bytes!("../../assets/fonts/DejaVuSansMono.ttf").to_vec());
        Self {
            fonts,
            swash: SwashCache::new(),
            cache: HashMap::new(),
            references: HashMap::new(),
        }
    }
    fn shape(&mut self, text: &str, face: Face) -> (Vec<LayoutGlyph>, f32, f32) {
        let (family, weight, style, size) = face_style(face);
        let mut b = Buffer::new(&mut self.fonts, Metrics::new(size, LINE_HEIGHT));
        b.set_wrap(Wrap::None);
        b.set_size(None, None);
        b.set_text(
            text,
            &Attrs::new()
                .family(Family::Name(family))
                .weight(weight)
                .style(style),
            Shaping::Advanced,
            None,
        );
        b.shape_until_scroll(&mut self.fonts, false);
        match b.layout_runs().next() {
            Some(run) => (run.glyphs.to_vec(), run.line_w, run.line_y),
            None => (vec![], 0.0, LINE_HEIGHT * 0.75),
        }
    }
    // Ink (height above the baseline, depth below it) of one glyph's pixels, in logical units.
    // Rasterized at INK_SCALE times the size, so the ink is measured in quarter points.
    fn glyph_ink(&mut self, glyph: &LayoutGlyph) -> (f32, f32) {
        let physical = glyph.physical((0.0, 0.0), INK_SCALE);
        let Some(image) = self
            .swash
            .get_image_uncached(&mut self.fonts, physical.cache_key)
        else {
            return (0.0, 0.0);
        };
        let (w, h) = (
            image.placement.width as usize,
            image.placement.height as usize,
        );
        // Alpha is the last byte of each pixel (one byte for a mask).
        let stride = match image.content {
            SwashContent::Mask => 1,
            _ => 4,
        };
        let inked = |y: usize| {
            image.data[y * w * stride..(y + 1) * w * stride]
                .chunks(stride)
                .any(|px| px[stride - 1] > 0)
        };
        let rows: Vec<usize> = (0..h).filter(|&y| inked(y)).collect();
        let (Some(&first), Some(&last)) = (rows.first(), rows.last()) else {
            return (0.0, 0.0);
        };
        let top = image.placement.top as f32;
        (
            (top - first as f32) / INK_SCALE,
            ((last + 1) as f32 - top) / INK_SCALE,
        )
    }
    // Ink height of the x and of the cap in a face: the caret's floor and its space height.
    fn references(&mut self, face: Face) -> (f32, f32) {
        if let Some(&heights) = self.references.get(&face) {
            return heights;
        }
        let mut height = |text: &str| {
            let (glyphs, _, _) = self.shape(text, face);
            glyphs.first().map_or(0.0, |g| self.glyph_ink(g).0)
        };
        let heights = (height("e"), height("A"));
        self.references.insert(face, heights);
        heights
    }
    fn word(&mut self, text: &str, face: Face) -> Word {
        let key = (text.to_owned(), face);
        if let Some(word) = self.cache.get(&key) {
            return word.clone();
        }
        let (glyphs, line_w, baseline) = self.shape(text, face);
        let (x_height, cap) = self.references(face);
        // Ligatures share a cluster; interpolate only at Unicode grapheme boundaries.
        let mut stops = vec![];
        let mut inks = vec![(cap, 0.0)];
        let mut previous = None;
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
            // The grapheme that ends at this stop started at the previous one.
            if let Some(start) = previous.replace(i) {
                let grapheme = &text[start..i];
                let ink = if grapheme.chars().all(char::is_whitespace) {
                    (cap, 0.0)
                } else {
                    let (ascent, descent) = glyphs
                        .iter()
                        .find(|g| g.start <= start && start < g.end)
                        .map_or((0.0, 0.0), |g| self.glyph_ink(g));
                    // Never shorter than the x-height, nor deeper than the baseline.
                    (ascent.max(x_height), descent.max(0.0))
                };
                inks.push(ink);
            }
        }
        let word = Word {
            glyphs,
            width: line_w,
            baseline,
            stops,
            inks,
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
    // Top and height of the caret on `row` at `byte`: the ink of the letter that ends there,
    // else the cap height at the line start.
    pub fn caret_extent(&self, row: usize, byte: usize) -> (f32, f32) {
        let line = &self.lines[row];
        let (ascent, descent) = line
            .carets
            .iter()
            .find(|c| c.0 == byte)
            .map_or(line.start_caret, |c| c.1);
        (line.baseline - ascent, ascent + descent)
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
#[cfg(test)]
mod tests {
    use super::*;

    // The glyphs drawn on one visual row.
    fn row(layout: &Layout, index: usize) -> Vec<&Placed> {
        let line = &layout.lines[index];
        layout
            .glyphs
            .iter()
            .filter(|g| g.baseline >= line.top && g.baseline < line.top + line.height)
            .collect()
    }

    // Expected (face, color) per glyph, one glyph per character of each piece.
    fn expect(
        t: &mut Typesetter,
        table: &[(&str, Face, u32)],
    ) -> Vec<(cosmic_text::fontdb::ID, u32)> {
        let mut want = Vec::new();
        for &(piece, face, color) in table {
            let id = t.word("x", face).glyphs[0].font_id;
            want.extend(piece.chars().map(|_| (id, color)));
        }
        want
    }

    #[test]
    fn heading_levels_pick_their_face_and_dim_the_marker() {
        let mut t = Typesetter::new();
        let cases = [
            ("# Title", Face::Bold),
            ("## Title", Face::Italic),
            ("### Title", Face::BoldItalic),
            ("###### Title", Face::BoldItalic),
        ];
        for (text, face) in cases {
            let layout = t.layout(text, 680.0);
            let id = t.word("x", face).glyphs[0].font_id;
            let glyphs = row(&layout, 0);
            let marker = text.len() - "Title".len();
            assert!(glyphs.iter().all(|g| g.glyph.font_id == id), "{text:?}");
            assert!(
                glyphs[..marker].iter().all(|g| g.color == MARKER_COLOR),
                "{text:?} marker"
            );
            assert!(
                glyphs[marker..].iter().all(|g| g.color == TEXT_COLOR),
                "{text:?} content"
            );
            assert!(layout.lines.iter().all(|line| line.height == LINE_HEIGHT));
        }
    }

    #[test]
    fn inline_styles_pick_faces_and_colors() {
        let mut t = Typesetter::new();
        let table = [
            ("**", Face::Bold, MARKER_COLOR),
            ("b", Face::Bold, TEXT_COLOR),
            ("**", Face::Bold, MARKER_COLOR),
            (" ", Face::Regular, TEXT_COLOR),
            ("*", Face::Italic, MARKER_COLOR),
            ("i", Face::Italic, TEXT_COLOR),
            ("*", Face::Italic, MARKER_COLOR),
            (" ", Face::Regular, TEXT_COLOR),
            ("***", Face::BoldItalic, MARKER_COLOR),
            ("bi", Face::BoldItalic, TEXT_COLOR),
            ("***", Face::BoldItalic, MARKER_COLOR),
            (" ", Face::Regular, TEXT_COLOR),
            ("`", Face::Mono, MARKER_COLOR),
            ("code", Face::Mono, CODE_COLOR),
            ("`", Face::Mono, MARKER_COLOR),
            (" ", Face::Regular, TEXT_COLOR),
            ("~~", Face::Regular, MARKER_COLOR),
            ("s", Face::Regular, TEXT_COLOR),
            ("~~", Face::Regular, MARKER_COLOR),
            (" ", Face::Regular, TEXT_COLOR),
            ("[", Face::Regular, MARKER_COLOR),
            ("l", Face::Regular, LINK_COLOR),
            ("](u)", Face::Regular, MARKER_COLOR),
        ];
        let text: String = table.iter().map(|t| t.0).collect();
        assert_eq!(text, "**b** *i* ***bi*** `code` ~~s~~ [l](u)");
        let layout = t.layout(&text, 680.0);
        let got: Vec<_> = row(&layout, 0)
            .iter()
            .map(|g| (g.glyph.font_id, g.color))
            .collect();
        assert_eq!(got, expect(&mut t, &table));

        // One strike mark, exactly over the struck "s" and not over its markers.
        assert_eq!(layout.marks.len(), 1);
        let mark = &layout.marks[0];
        let index = table.iter().position(|p| p.0 == "s").unwrap();
        let s: usize = table[..index].iter().map(|p| p.0.chars().count()).sum();
        let glyphs = row(&layout, 0);
        assert_eq!(mark.color, MARKER_COLOR);
        assert!((mark.x - glyphs[s].x).abs() < 0.01);
        assert!((mark.x + mark.width - glyphs[s + 1].x).abs() < 0.01);
        assert!((mark.y - (glyphs[s].baseline - FONT_SIZE * STRIKE_LIFT)).abs() < 0.01);
    }

    #[test]
    fn code_is_smaller_mono_and_keeps_line_height() {
        let mut t = Typesetter::new();
        for text in ["`code`", "# `code`", "- `code` item", "a `x` b"] {
            let layout = t.layout(text, 680.0);
            assert!(
                layout.lines.iter().all(|line| line.height == LINE_HEIGHT),
                "{text:?}"
            );
        }
        let layout = t.layout("`code`", 680.0);
        assert_eq!(row(&layout, 0)[1].glyph.font_size, FONT_SIZE * MONO_SCALE);
    }

    #[test]
    fn caret_height_follows_the_last_letter_ink() {
        let mut t = Typesetter::new();
        // Bytes: A=0, l=1, e=2, y=3, '.'=4. The caret after byte b sits after the letter at b-1.
        let layout = t.layout("Aley.", 680.0);
        let (a, l, e, y, dot) = (
            layout.caret_extent(0, 1),
            layout.caret_extent(0, 2),
            layout.caret_extent(0, 3),
            layout.caret_extent(0, 4),
            layout.caret_extent(0, 5),
        );
        let bottom = |(top, height): (f32, f32)| top + height;
        let baseline = layout.lines[0].baseline;
        assert!(a.0 < e.0, "cap is above x-height");
        assert!(l.0 < e.0, "ascender is above x-height");
        // Measured at quarter-point precision: l 13.25, A 12.5.
        assert!(l.1 > a.1, "ascender is taller than the cap");
        assert!(
            bottom(y) > baseline + 1.0,
            "descender reaches below the baseline"
        );
        assert!(
            (bottom(e) - baseline).abs() < 1.0,
            "x-height ends at the baseline"
        );
        assert!(
            (dot.1 - e.1).abs() < 1.0,
            "a period is as tall as an x-height letter"
        );
        let empty = t.layout("", 680.0);
        let line = &empty.lines[0];
        let (top, height) = empty.caret_extent(0, 0);
        assert!((height - a.1).abs() < 0.01 && (top - a.0).abs() < 0.01);
        assert!(top >= line.top && top + height <= line.top + line.height);
        // A closed one-line prompt's caret follows its last letter, not the full line box.
        let doc = Document::new("a \\x squared\\ b");
        assert!(doc.formulas.contains_key(&2));
        let label = t.document(&doc, 680.0);
        let (_, label_height) = label.caret_extent(0, 2 + ATOM.len());
        assert!(
            label_height <= y.1 + 1.0,
            "label caret {label_height} vs y {}",
            y.1
        );
    }

    #[test]
    fn block_colors_and_faces() {
        let mut t = Typesetter::new();
        let quote = t.layout("> quoted", 680.0);
        let italic = t.word("x", Face::Italic).glyphs[0].font_id;
        assert!(row(&quote, 0).iter().all(|g| g.glyph.font_id == italic));
        let done = t.layout("- [x] done", 680.0);
        assert!(row(&done, 0).iter().all(|g| g.color == MARKER_COLOR));
        let todo = t.layout("- [ ] todo", 680.0);
        assert!(row(&todo, 0)[6..].iter().all(|g| g.color == TEXT_COLOR));
    }

    #[test]
    fn open_ai_prompt_is_dimmed_like_a_pending_prompt() {
        let mut t = Typesetter::new();
        let mut doc = Document::new("left right");
        doc.anchor = 4;
        doc.caret = 4;
        doc.open_ai_prompt();
        doc.replace("sum", true);
        let open = doc.open_range().expect("open prompt");
        let layout = t.document(&doc, 680.0);
        let (_, start) = layout.caret(open.start);
        let (_, end) = layout.caret(open.end);
        let glyphs = row(&layout, 0);
        let inside: Vec<_> = glyphs
            .iter()
            .filter(|g| g.x >= start && g.x < end)
            .collect();
        assert!(!inside.is_empty());
        assert!(inside.iter().all(|g| g.color == PENDING_COLOR));
        let before: Vec<_> = glyphs.iter().filter(|g| g.x < start).collect();
        assert!(!before.is_empty());
        assert!(before.iter().all(|g| g.color == TEXT_COLOR));
    }

    #[test]
    fn open_prompt_alone_on_its_line_is_centered_like_display_math() {
        let mut t = Typesetter::new();
        let mut doc = Document::new("before\n\nafter");
        doc.anchor = 7;
        doc.caret = 7;
        doc.open_ai_prompt();
        doc.replace("x squared", true);
        let open = doc.open_range().expect("open prompt");
        let layout = t.document(&doc, 680.0);
        let (row, _) = layout.caret(open.start);
        let line = &layout.lines[row];
        let left = line.stops.first().expect("stops").1;
        let right = 680.0 - line.stops.last().expect("stops").1;
        assert!(left > 0.0);
        assert!((left - right).abs() <= 1.0, "left {left} right {right}");
        let before = layout.lines[0].height;
        assert!(line.height > before, "display padding");
    }

    #[test]
    fn empty_open_prompt_on_an_empty_line_starts_centered() {
        let mut t = Typesetter::new();
        let mut doc = Document::new("before\n\nafter");
        doc.anchor = 7;
        doc.caret = 7;
        doc.open_ai_prompt();
        let open = doc.open_range().expect("open prompt");
        let layout = t.document(&doc, 680.0);
        let (row, _) = layout.caret(open.start);
        let line = &layout.lines[row];
        assert_eq!(line.stops.len(), 1);
        assert!(
            (line.stops[0].1 - 340.0).abs() <= 1.0,
            "x {}",
            line.stops[0].1
        );
    }

    #[test]
    fn open_prompt_inside_a_sentence_stays_inline() {
        let mut t = Typesetter::new();
        let mut doc = Document::new("left right");
        doc.anchor = 4;
        doc.caret = 4;
        doc.open_ai_prompt();
        doc.replace("sum", true);
        let open = doc.open_range().expect("open prompt");
        let layout = t.document(&doc, 680.0);
        let (row, _) = layout.caret(open.start);
        assert_eq!(layout.lines[row].stops[0].1, 0.0);
    }

    #[test]
    fn wrapped_items_hang_under_their_content_and_paragraphs_do_not() {
        let mut t = Typesetter::new();
        let words = "alpha beta gamma delta epsilon zeta eta theta";
        let cases: [(&str, Option<usize>); 5] = [
            ("- ", Some(2)),
            ("1. ", Some(3)),
            ("- [x] ", Some(6)),
            ("> ", Some(2)),
            ("", None),
        ];
        for (prefix, content) in cases {
            let text = format!("{prefix}{words}");
            let layout = t.layout(&text, 120.0);
            assert!(layout.lines.len() > 2, "{text:?} must wrap");
            let hang = content.map_or(0.0, |byte| layout.caret(byte).1);
            for line in &layout.lines[1..] {
                assert!((line.stops[0].1 - hang).abs() < 0.01, "{text:?}");
            }
            for line in &layout.lines {
                assert!(line.stops.last().unwrap().1 <= 120.0 + 0.01, "{text:?}");
            }
        }
    }

    #[test]
    fn rules_draw_a_mark_from_the_marker_to_the_column() {
        let mut t = Typesetter::new();
        for text in ["---", "***", "* * *", "___"] {
            let layout = t.layout(text, 300.0);
            assert_eq!(layout.marks.len(), 1, "{text:?}");
            let line = &layout.lines[0];
            let mark = &layout.marks[0];
            assert_eq!(mark.color, MARKER_COLOR);
            assert!(
                (mark.x - layout.caret(text.len()).1).abs() < 0.01,
                "{text:?}"
            );
            assert!((mark.x + mark.width - 300.0).abs() < 0.01, "{text:?}");
            assert!((mark.y - (line.top + line.height / 2.0)).abs() < 0.01);
        }
    }

    #[test]
    fn mixed_markdown_keeps_line_heights_and_editable_stops() {
        let mut t = Typesetter::new();
        let source = "# Title\n## Sub\n### Deep\nSome **bold** *it* ***both*** `code` ~~gone~~ [link](url) and a paragraph that wraps over several rows\n- item with words that wrap around the column\n- [x] done task\n1. first\n> quoted text that also wraps around\n---\nplain \\$5 end";
        let doc = Document::new(source);
        let text = doc.text();
        let layout = t.document(&doc, 160.0);
        assert!(layout.lines.len() > 12);
        assert!(layout.lines.iter().all(|line| line.height == LINE_HEIGHT));
        for (row, line) in layout.lines.iter().enumerate() {
            assert_eq!(layout.row_at(line.top + line.height / 2.0), row);
        }
        for (byte, _) in text.grapheme_indices(true) {
            let (row, x) = layout.caret(byte);
            assert_eq!(layout.hit(row, x), byte, "byte {byte} of {text:?}");
        }
    }

    #[test]
    fn every_face_loads_its_own_font() {
        let mut t = Typesetter::new();
        let faces = [
            Face::Regular,
            Face::Italic,
            Face::Bold,
            Face::BoldItalic,
            Face::Mono,
        ];
        let ids: Vec<_> = faces
            .iter()
            .map(|&face| t.word("Title", face).glyphs[0].font_id)
            .collect();
        for (i, id) in ids.iter().enumerate() {
            assert!(
                !ids[..i].contains(id),
                "{:?} reuses another face's font",
                faces[i]
            );
        }
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
