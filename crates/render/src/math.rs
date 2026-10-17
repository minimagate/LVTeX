//! In-memory Typst world: embedded fonts only, no filesystem or package access.
use typst::diag::{FileError, FileResult, SourceDiagnostic};
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
            include_bytes!("../../../assets/fonts/NewCMMath-Regular.otf").as_slice(),
            include_bytes!("../../../assets/fonts/LibertinusSerif-Regular.otf").as_slice(),
            include_bytes!("../../../assets/fonts/LibertinusSerif-Italic.otf").as_slice(),
            include_bytes!("../../../assets/fonts/LibertinusSerif-Bold.otf").as_slice(),
            include_bytes!("../../../assets/fonts/LibertinusSerif-BoldItalic.otf").as_slice(),
            include_bytes!("../../../assets/fonts/DejaVuSansMono.ttf").as_slice(),
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

    pub fn compile_pdf(&mut self, source: String) -> Result<Vec<u8>, String> {
        let document = self.compile(source)?;
        typst_pdf::pdf(&document, &typst_pdf::PdfOptions::default()).map_err(|e| join_messages(&e))
    }

    fn compile(&mut self, source: String) -> Result<PagedDocument, String> {
        self.source = Source::detached(source);
        let compiled = typst::compile::<PagedDocument>(self);
        let document = compiled.output.map_err(|e| join_messages(&e))?;
        if !compiled.warnings.is_empty() {
            return Err(join_messages(&compiled.warnings));
        }
        Ok(document)
    }

    pub fn render_source(
        &mut self,
        source: String,
        scale: f64,
    ) -> Result<tiny_skia::Pixmap, String> {
        if !scale.is_finite() || !(0.5..=4.0).contains(&scale) {
            return Err("scale must be finite and between 0.5 and 4".into());
        }
        let document = self.compile(source)?;
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
        // Bound raster allocation before asking Typst to create the pixel buffer.
        if size.x.to_pt() * scale > 4096.0 || size.y.to_pt() * scale > 4096.0 {
            return Err("formula exceeds the 4096-pixel limit".into());
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

fn join_messages(diagnostics: &[SourceDiagnostic]) -> String {
    diagnostics
        .iter()
        .map(|e| e.message.to_string())
        .collect::<Vec<_>>()
        .join("; ")
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
