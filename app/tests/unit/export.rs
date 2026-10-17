use super::*;
use crate::math;
use std::sync::Arc;

fn render_formulas(doc: &mut Document) {
    let mut renderer = Renderer::with_prelude(mathstral_latex::PRELUDE);
    for job in doc.jobs() {
        let Some(latex) = job.latex.as_deref() else {
            continue;
        };
        let image = math::render::render(&mut renderer, latex).unwrap();
        doc.complete(job.id, Ok(Arc::new(image)));
    }
}

#[test]
fn exports_typeset_pdf_with_prompts_and_escaped_prose() {
    let mut doc = Document::new(
        "# Title\n\nA line with # * _ \\$ \\\\ @ < [ characters.\n\nInline $x^2$ here.\n\n$$\n\\int_0^1 x dx\n$$\n\n<!--mathstral:pending:aGFsZiBvZiB4-->",
    );
    assert_eq!(unrendered(&doc), 3);
    render_formulas(&mut doc);
    let source = typst_source(&doc);
    assert!(source.contains("#heading(level: 1)[#\"Title\"]"));
    assert!(source.contains(&mathstral_latex::typst_string_literal(
        "A line with # * _ $ \\ @ < [ characters."
    )));
    assert!(source.contains("$ #eval("));
    assert!(source.contains(&mathstral_latex::typst_string_literal("half of x")));
    assert!(!source.contains(&mathstral_latex::typst_string_literal("\\half of x\\")));
    assert_eq!(unrendered(&doc), 1);
    assert!(pdf(&doc).unwrap().starts_with(b"%PDF-"));
}

#[test]
fn exports_markdown_styles_as_typst() {
    let mut doc = Document::new(
        "# Heading one\n## Heading two\n### Heading three\n\n**b** *i* ***bi*** `c` ~~s~~ [l](https://x.y) 2 * 3\n\n- outer\n  - nested\n- second\n\n1. first\n7. seventh\n\n- [ ] open\n- [x] done\n\n> quoted\n\n---\n\n**area $x^2$ here**",
    );
    render_formulas(&mut doc);
    assert_eq!(unrendered(&doc), 0);
    let source = typst_source(&doc);
    let unchecked = "#box(width: 0.7em, height: 0.7em, stroke: 0.5pt, baseline: 0.05em)";
    let checked = "#box(width: 0.7em, height: 0.7em, stroke: 0.5pt, baseline: 0.05em, fill: black)";
    let wants = [
        "#heading(level: 1)[#\"Heading one\"]",
        "#heading(level: 2)[#\"Heading two\"]",
        "#heading(level: 3)[#\"Heading three\"]",
        "#strong[#\"b\"]",
        "#emph[#\"i\"]",
        "#strong[#emph[#\"bi\"]]",
        "#raw(\"c\")",
        "#strike[#\"s\"]",
        "#link(\"https://x.y\")[#\"l\"]",
        "2 * 3\"",
        "- #\"outer\"\n  - #\"nested\"\n- #\"second\"",
        "1. #\"first\"\n7. #\"seventh\"",
        &format!("- {unchecked} #\"open\"\n- {checked} #text(fill: gray)[#\"done\"]"),
        "#quote(block: true)[#\"quoted\"]",
        "#line(length: 100%, stroke: 0.5pt)",
        "#strong[#\"area \"$",
    ];
    for want in wants {
        assert!(source.contains(want), "missing {want:?} in:\n{source}");
    }
    assert!(pdf(&doc).unwrap().starts_with(b"%PDF-"));
}
