//! PDF export: typesets the document with Typst, formulas as vector math.
use crate::buffer::{Document, Formula};
use mathstral_render::math::Renderer;

const PREAMBLE: &str = "#set page(paper: \"a4\", margin: 2.5cm)\n\
#set text(font: \"Libertinus Serif\", size: 11pt, fill: black)\n\
#set par(justify: true)\n\
#show heading: set text(style: \"italic\", weight: \"regular\")\n\
#show math.equation: set text(font: \"New Computer Modern Math\")\n";

fn formula_source(f: &Formula) -> Option<String> {
    f.image.as_ref()?;
    mathstral_latex::convert(f.latex.as_deref()?).ok()
}

pub fn unrendered(doc: &Document) -> usize {
    doc.formulas
        .values()
        .filter(|f| formula_source(f).is_none())
        .count()
}

pub fn typst_source(doc: &Document) -> String {
    let text = doc.text();
    let mut blocks = vec![];
    let mut line_start = 0;
    for line in text.split('\n') {
        let start = line_start;
        line_start += line.len() + 1;
        if line.trim().is_empty() {
            continue;
        }
        blocks.push(match line.strip_prefix("# ") {
            Some(rest) => format!(
                "#heading[{}]",
                paragraph(doc, &text, rest, start + "# ".len())
            ),
            None => paragraph(doc, &text, line, start),
        });
    }
    format!(
        "{PREAMBLE}{}\n{}\n",
        mathstral_latex::PRELUDE,
        blocks.join("\n\n")
    )
}

/// Typesets one line; `start` is the byte offset of `line` in `text`.
fn paragraph(doc: &Document, text: &str, line: &str, start: usize) -> String {
    let mut parts = String::new();
    let mut run = String::new();
    for (i, c) in line.char_indices() {
        let pos = start + i;
        let Some(formula) = doc.formulas.get(&pos) else {
            run.push(c);
            continue;
        };
        if !run.is_empty() {
            parts.push_str(&format!("#{}", mathstral_latex::typst_string_literal(&run)));
            run.clear();
        }
        match formula_source(formula) {
            Some(body) if Document::is_display_in(text, pos) => return format!("$ {body} $"),
            Some(body) => parts.push_str(&format!("${body}$")),
            None => parts.push_str(&format!(
                "#{}",
                mathstral_latex::typst_string_literal(&formula.label())
            )),
        }
    }
    if !run.is_empty() {
        parts.push_str(&format!("#{}", mathstral_latex::typst_string_literal(&run)));
    }
    parts
}

pub fn pdf(doc: &Document) -> Result<Vec<u8>, String> {
    Renderer::new().compile_pdf(typst_source(doc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math;
    use std::sync::Arc;

    #[test]
    fn exports_typeset_pdf_with_prompts_and_escaped_prose() {
        let mut doc = Document::new(
            "# Title\n\nA line with # * _ \\$ \\\\ @ < [ characters.\n\nInline $x^2$ here.\n\n$$\n\\int_0^1 x dx\n$$\n\n/half of x/",
        );
        assert_eq!(unrendered(&doc), 3);
        let mut renderer = Renderer::with_prelude(mathstral_latex::PRELUDE);
        for job in doc.jobs() {
            let Some(latex) = job.latex.as_deref() else {
                continue;
            };
            let image = math::render(&mut renderer, latex).unwrap();
            doc.complete(job.id, Ok(Arc::new(image)));
        }
        let source = typst_source(&doc);
        assert!(source.contains("#heading[#\"Title\"]"));
        assert!(source.contains(&mathstral_latex::typst_string_literal(
            "A line with # * _ $ \\ @ < [ characters."
        )));
        assert!(source.contains("$ #eval("));
        assert!(source.contains("\"/half of x/\""));
        assert_eq!(unrendered(&doc), 1);
        assert!(pdf(&doc).unwrap().starts_with(b"%PDF-"));
    }
}
