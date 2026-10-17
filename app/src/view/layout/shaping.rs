//! Font selection, glyph shaping and cached caret ink measurements.
use super::*;
use cosmic_text::{
    Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Style, SwashCache, SwashContent, Weight,
    Wrap,
};
use std::collections::HashMap;

// Match the prose x-height; measure ink at quarter-point precision.
const MONO_SCALE: f32 = 0.85;
const INK_SCALE: f32 = 4.0;

#[derive(Clone)]
pub(super) struct Word {
    pub(super) glyphs: Vec<LayoutGlyph>,
    pub(super) width: f32,
    pub(super) baseline: f32,
    pub(super) stops: Vec<(usize, f32)>,
    // Caret extent (ascent, descent) at each stop: the ink of the grapheme that ends there.
    // The first entry is the cap height of the face, for a caret with no letter before it.
    pub(super) inks: Vec<(f32, f32)>,
}
// Each face selects one embedded font file.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(super) enum Face {
    Regular,
    Italic,
    Bold,
    BoldItalic,
    Mono,
}
pub(super) fn face_of(bold: bool, italic: bool) -> Face {
    match (bold, italic) {
        (false, false) => Face::Regular,
        (false, true) => Face::Italic,
        (true, false) => Face::Bold,
        (true, true) => Face::BoldItalic,
    }
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
            include_bytes!("../../../../assets/fonts/LibertinusSerif-Regular.otf").to_vec(),
        );
        fonts.db_mut().load_font_data(
            include_bytes!("../../../../assets/fonts/LibertinusSerif-Italic.otf").to_vec(),
        );
        fonts.db_mut().load_font_data(
            include_bytes!("../../../../assets/fonts/LibertinusSerif-Bold.otf").to_vec(),
        );
        fonts.db_mut().load_font_data(
            include_bytes!("../../../../assets/fonts/LibertinusSerif-BoldItalic.otf").to_vec(),
        );
        fonts
            .db_mut()
            .load_font_data(include_bytes!("../../../../assets/fonts/DejaVuSansMono.ttf").to_vec());
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
    pub(super) fn references(&mut self, face: Face) -> (f32, f32) {
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
    pub(super) fn word(&mut self, text: &str, face: Face) -> Word {
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
}
