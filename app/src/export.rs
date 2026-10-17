//! PDF export: typesets the document with Typst, formulas as vector math.
use crate::buffer::{Document, Formula};
use crate::markdown::{self, Block};
use mathstral_render::math::Renderer;

const PREAMBLE: &str = "#set page(paper: \"a4\", margin: 2.5cm)\n\
#set text(font: \"Libertinus Serif\", size: 11pt, fill: black)\n\
#set par(justify: true)\n\
#show heading: set text(size: 11pt, weight: \"bold\", style: \"italic\")\n\
#show heading.where(level: 1): set text(style: \"normal\")\n\
#show heading.where(level: 2): set text(weight: \"regular\")\n\
#show raw: set text(font: \"DejaVu Sans Mono\", size: 0.85em)\n\
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
        blocks.push(block(doc, &text, line, start));
    }
    // Consecutive list items form one tight list; every other block is its own paragraph.
    let mut body = String::new();
    let mut prev_list = None;
    for (typst, list) in blocks {
        if let Some(prev) = prev_list {
            body.push_str(if prev && list { "\n" } else { "\n\n" });
        }
        body.push_str(&typst);
        prev_list = Some(list);
    }
    format!("{PREAMBLE}{}\n{body}\n", mathstral_latex::PRELUDE)
}

/// Typesets one line; returns the Typst block and whether it is a list item.
/// `start` is the byte offset of `line` in `text`.
fn block(doc: &Document, text: &str, line: &str, start: usize) -> (String, bool) {
    if let Some(math) = display(doc, text, line, start) {
        return (math, false);
    }
    let l = markdown::line(line);
    let body = inline(doc, &line[l.content()..], start + l.content());
    let indent = &line[..l.marker.start];
    match l.block {
        Block::Paragraph => (body, false),
        Block::Heading(level) => (format!("#heading(level: {level})[{body}]"), false),
        Block::Bullet => (format!("{indent}- {body}"), true),
        Block::Ordered(n) => (format!("{indent}{n}. {body}"), true),
        Block::Task(checked) => {
            let fill = if checked { ", fill: black" } else { "" };
            let content = if checked {
                format!("#text(fill: gray)[{body}]")
            } else {
                body
            };
            let square =
                format!("#box(width: 0.7em, height: 0.7em, stroke: 0.5pt, baseline: 0.05em{fill})");
            (format!("{indent}- {square} {content}"), true)
        }
        Block::Quote => (format!("#quote(block: true)[{body}]"), false),
        Block::Rule => ("#line(length: 100%, stroke: 0.5pt)".into(), false),
    }
}

/// `$ body $` when the line is a rendered display formula.
fn display(doc: &Document, text: &str, line: &str, start: usize) -> Option<String> {
    line.char_indices().find_map(|(i, _)| {
        let pos = start + i;
        let body = formula_source(doc.formulas.get(&pos)?)?;
        Document::is_display_in(text, pos).then(|| format!("$ {body} $"))
    })
}

/// Typesets the inline content of a line; `base` is the byte offset of `content` in the document.
fn inline(doc: &Document, content: &str, base: usize) -> String {
    let spans = markdown::inline(content);
    let closers: Vec<&str> = spans
        .iter()
        .filter(|s| s.marker && s.style.link)
        .flat_map(|s| link_marks(&content[s.range.clone()]))
        .flatten()
        .collect();
    let mut opened = 0;
    let mut out = String::new();
    for span in &spans {
        let slice = &content[span.range.clone()];
        if span.marker {
            if span.style.link {
                opened += link_marks(slice).iter().filter(|m| m.is_none()).count();
            }
            continue;
        }
        let style = span.style;
        let mut body = runs(doc, base + span.range.start, slice, style.code);
        if style.strike {
            body = format!("#strike[{body}]");
        }
        if style.italic {
            body = format!("#emph[{body}]");
        }
        if style.bold {
            body = format!("#strong[{body}]");
        }
        let link = style
            .link
            .then(|| opened.checked_sub(1).and_then(|k| closers.get(k)))
            .flatten();
        if let Some(url) = link {
            let url = mathstral_latex::typst_string_literal(url);
            body = format!("#link({url})[{body}]");
        }
        out.push_str(&body);
    }
    out
}

/// Link syntax inside one link-styled marker span: `Some(url)` closes a link, `None` opens one.
fn link_marks(marker: &str) -> Vec<Option<&str>> {
    let mut marks = vec![];
    let mut rest = marker;
    while let Some(i) = rest.find(['[', ']']) {
        let tail = &rest[i..];
        if let Some(after) = tail.strip_prefix("](") {
            let Some(end) = after.find(')') else {
                break;
            };
            marks.push(Some(&after[..end]));
            rest = &after[end + 1..];
        } else {
            if tail.starts_with('[') {
                marks.push(None);
            }
            rest = &tail[1..];
        }
    }
    marks
}

/// Typesets `slice`, which starts at byte `base` of the document: prose as string literals
/// (raw when `code`), formulas as math, unrendered prompts as their label.
fn runs(doc: &Document, base: usize, slice: &str, code: bool) -> String {
    let mut parts = String::new();
    let mut run = String::new();
    for (i, c) in slice.char_indices() {
        let Some(formula) = doc.formulas.get(&(base + i)) else {
            run.push(c);
            continue;
        };
        parts.push_str(&prose(&run, code));
        run.clear();
        match formula_source(formula) {
            Some(body) => parts.push_str(&format!("${body}$")),
            None => parts.push_str(&prose(&formula.label(), code)),
        }
    }
    parts + &prose(&run, code)
}

fn prose(text: &str, code: bool) -> String {
    if text.is_empty() {
        return String::new();
    }
    let literal = mathstral_latex::typst_string_literal(text);
    if code {
        format!("#raw({literal})")
    } else {
        format!("#{literal}")
    }
}

pub fn pdf(doc: &Document) -> Result<Vec<u8>, String> {
    Renderer::new().compile_pdf(typst_source(doc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math;
    use std::sync::Arc;

    fn render_formulas(doc: &mut Document) {
        let mut renderer = Renderer::with_prelude(mathstral_latex::PRELUDE);
        for job in doc.jobs() {
            let Some(latex) = job.latex.as_deref() else {
                continue;
            };
            let image = math::render(&mut renderer, latex).unwrap();
            doc.complete(job.id, Ok(Arc::new(image)));
        }
    }

    #[test]
    fn exports_typeset_pdf_with_prompts_and_escaped_prose() {
        let mut doc = Document::new(
            "# Title\n\nA line with # * _ \\$ \\\\ @ < [ characters.\n\nInline $x^2$ here.\n\n$$\n\\int_0^1 x dx\n$$\n\n\\half of x\\",
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
        let checked =
            "#box(width: 0.7em, height: 0.7em, stroke: 0.5pt, baseline: 0.05em, fill: black)";
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
}
