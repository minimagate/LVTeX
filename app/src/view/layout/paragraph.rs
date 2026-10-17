//! Markdown styling and prompt placement within one paragraph.
use super::shaping::face_of;
use super::*;

// One paragraph's Markdown styling. Ranges are absolute byte offsets in the document.
pub(super) struct Para {
    pub(super) block: Block,
    // End of the block marker; the content starts here.
    pub(super) marker: usize,
    pub(super) bold: bool,
    // Size of the block's lines, as a multiple of FONT_SIZE.
    pub(super) scale: f32,
    // Inline spans covering the content, in order.
    pub(super) spans: Vec<Span>,
    pub(super) open: Option<Range<usize>>,
}
impl Para {
    pub(super) fn new(paragraph: &str, offset: usize, open: Option<Range<usize>>) -> Self {
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
        let (bold, scale) = match line.block {
            Block::Heading(level) => (true, HEADING_SCALES[level as usize - 1]),
            _ => (false, 1.0),
        };
        Self {
            block: line.block,
            marker: offset + content,
            bold,
            scale,
            spans,
            open,
        }
    }
    // Wrapped rows of these blocks start under their content.
    pub(super) fn hangs(&self) -> bool {
        matches!(
            self.block,
            Block::Bullet | Block::Ordered(_) | Block::Task(_) | Block::Quote
        )
    }
    // The next byte after `at` where the style may change.
    pub(super) fn cut(&self, at: usize) -> usize {
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
    // Color of plain text in this block.
    fn text_color(&self) -> u32 {
        if self.block == Block::Quote {
            QUOTE_COLOR
        } else {
            TEXT_COLOR
        }
    }
    // Face, color and strikethrough for the bytes that start at `at`.
    pub(super) fn look(&self, at: usize) -> (Face, u32, bool) {
        if self.open.as_ref().is_some_and(|r| r.contains(&at)) {
            return (Face::Regular, PENDING_COLOR, false);
        }
        let base = face_of(self.bold, false);
        if at < self.marker {
            return (base, MARKER_COLOR, false);
        }
        let Some(span) = self
            .spans
            .get(self.spans.partition_point(|s| s.range.end <= at))
        else {
            return (base, self.text_color(), false);
        };
        let style = span.style;
        let face = if style.code {
            Face::Mono
        } else {
            face_of(self.bold || style.bold, style.italic)
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
            self.text_color()
        };
        (face, color, style.strike && !span.marker)
    }
}
// True when the open prompt is a single line alone on its line: it starts and ends inside this
// paragraph (so no line break lies in it) and the paragraph's text outside it is whitespace.
// A prompt spanning several lines is never centered; each of its lines stays inline.
pub(super) fn prompt_alone(paragraph: &str, offset: usize, open: Option<&Range<usize>>) -> bool {
    let Some(open) = open else {
        return false;
    };
    let (Some(start), Some(end)) = (open.start.checked_sub(offset), open.end.checked_sub(offset))
    else {
        return false;
    };
    if start > end || end > paragraph.len() {
        return false;
    }
    paragraph[..start].trim().is_empty() && paragraph[end..].trim().is_empty()
}
