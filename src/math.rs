//! In-memory Typst world: embedded fonts only, no filesystem or package access.
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, Source};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;

pub const FONT_SIZE: f64 = 22.0;
pub const SCALE: f64 = 2.0;

pub struct Renderer {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    source: Source,
    prelude: &'static str,
    pub last_baseline: f64,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer {
    pub fn new() -> Self {
        Self::with_prelude("")
    }

    pub fn with_prelude(prelude: &'static str) -> Self {
        let fonts = [
            include_bytes!("../assets/fonts/NewCMMath-Regular.otf").as_slice(),
            include_bytes!("../assets/fonts/LibertinusSerif-Regular.otf").as_slice(),
        ]
        .into_iter()
        .map(|data| Font::new(Bytes::new(data), 0).expect("bundled font is valid"))
        .collect::<Vec<_>>();
        Self {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(FontBook::from_fonts(&fonts)),
            fonts,
            source: Source::detached(String::new()),
            prelude,
            last_baseline: 0.0,
        }
    }

    pub fn render_math(
        &mut self,
        body: &str,
        display: bool,
        scale: f64,
    ) -> Result<tiny_skia::Pixmap, String> {
        // A paragraph's line metrics deliberately consume some math overhang.
        // Use block bounds for the atom in both cases, retaining text-style math
        // for inline atoms so matrices, integrals and scripts are never clipped.
        let equation = if display {
            format!("$ {body} $")
        } else {
            format!("#math.equation(block: true, math.inline(${body}$))")
        };
        self.render_source(
            format!(
                "#set page(width: auto, height: auto, margin: 4pt, fill: none)\n\
             #set text(font: \"Libertinus Serif\", size: {FONT_SIZE}pt, fill: rgb(\"e8e6e3\"))\n\
             #show math.equation: set text(font: \"New Computer Modern Math\")\n\
             {}\n{equation}",
                self.prelude
            ),
            scale,
        )
    }

    pub fn render_source(
        &mut self,
        source: String,
        scale: f64,
    ) -> Result<tiny_skia::Pixmap, String> {
        if !scale.is_finite() || !(0.5..=4.0).contains(&scale) {
            return Err("scale must be finite and between 0.5 and 4".into());
        }
        self.source = Source::detached(source);
        let compiled = typst::compile::<PagedDocument>(self);
        let document = compiled.output.map_err(|errors| {
            errors
                .iter()
                .map(|e| e.message.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        })?;
        if !compiled.warnings.is_empty() {
            return Err(compiled
                .warnings
                .iter()
                .map(|e| e.message.to_string())
                .collect::<Vec<_>>()
                .join("; "));
        }
        let [page] = document.pages() else {
            return Err("expected exactly one page".into());
        };
        fn baseline(frame: &typst::layout::Frame) -> Option<f64> {
            if frame.has_baseline() {
                return Some(frame.baseline().to_pt());
            }
            frame.items().find_map(|(pos, item)| match item {
                typst::layout::FrameItem::Group(group) => {
                    baseline(&group.frame).map(|y| pos.y.to_pt() + y)
                }
                _ => None,
            })
        }
        self.last_baseline =
            baseline(&page.frame).unwrap_or(page.frame.height().to_pt() / 2.0 + 5.5);
        let size = page.frame.size();
        // Check before raster allocation. This spike is not yet a resource-limited
        // compiler for arbitrary model output; that belongs in the worker stage.
        if size.x.to_pt() * scale > 4096.0 || size.y.to_pt() * scale > 4096.0 {
            return Err("formula exceeds the 4096-pixel spike limit".into());
        }
        let pixmap = typst_render::render(
            page,
            &typst_render::RenderOptions {
                pixel_per_pt: typst::utils::Scalar::new(scale),
                ..Default::default()
            },
        );
        if !pixmap.pixels().iter().any(|p| p.alpha() > 0) {
            return Err("formula rendered no visible pixels".into());
        }
        Ok(pixmap)
    }
}

impl World for Renderer {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }
    fn main(&self) -> FileId {
        self.source.id()
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main() {
            Ok(self.source.clone())
        } else {
            Err(FileError::AccessDenied)
        }
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.main() {
            Ok(Bytes::from_string(self.source.text().to_owned()))
        } else {
            Err(FileError::AccessDenied)
        }
    }
    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }
    fn today(&self, _: Option<Duration>) -> Option<Datetime> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_ink_stays_inside_the_canvas() {
        let mut renderer = Renderer::new();
        for formula in crate::fixtures::FORMULAS {
            for display in [false, true] {
                let image = renderer.render_math(formula.typst, display, SCALE).unwrap();
                let w = image.width() as usize;
                let h = image.height() as usize;
                for (index, pixel) in image.pixels().iter().enumerate() {
                    let (x, y) = (index % w, index / w);
                    if x < 2 || y < 2 || x >= w - 2 || y >= h - 2 {
                        assert_eq!(
                            pixel.alpha(),
                            0,
                            "clipped {} display={display}",
                            formula.name
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn error_and_retina_paths() {
        let mut renderer = Renderer::new();
        assert!(renderer.render_math("frac(", true, SCALE).is_err());
        assert!(
            renderer
                .render_math("not_a_known_symbol", true, SCALE)
                .is_err()
        );
        assert!(renderer.render_math("x", false, f64::NAN).is_err());
        let one = renderer.render_math("frac(1, 2)", false, 1.0).unwrap();
        let two = renderer.render_math("frac(1, 2)", false, 2.0).unwrap();
        assert!((two.width() as i64 - 2 * one.width() as i64).abs() <= 1);
        assert!((two.height() as i64 - 2 * one.height() as i64).abs() <= 1);
        assert!(two.pixels().iter().any(|p| p.alpha() == 0));
        assert!(two.pixels().iter().any(|p| p.alpha() > 0));
        assert!(
            renderer
                .render_source("#read(\"/etc/passwd\")".into(), SCALE)
                .is_err()
        );
    }
}
