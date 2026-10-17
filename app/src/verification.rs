use crate::{
    buffer::{ATOM, Document},
    layout::Typesetter,
    markdown, math, render,
};
use mathstral_render::math::Renderer;
use std::{sync::Arc, time::Instant};

fn finish(doc: &mut Document, renderer: &mut Renderer) {
    for job in doc.jobs() {
        let latex = job
            .latex
            .as_deref()
            .unwrap_or_else(|| match job.prompt.as_deref().unwrap() {
                "integral from 0 to 1 of x^2 dx" => r"\int_0^1 x^2\,dx",
                "1 over 3" => r"\frac{1}{3}",
                "matrix rows 1 2 and 3 4" => r"\begin{pmatrix}1&2\\3&4\end{pmatrix}",
                "sum of 1 over n squared from 1 to infinity equals pi squared over 6" => {
                    r"\sum_{n=1}^{\infty}\frac{1}{n^2}=\frac{\pi^2}{6}"
                }
                "f of x is x squared for nonnegative x, minus x otherwise" => {
                    r"f(x)=\begin{cases}x^2&x\geq0\\-x&x<0\end{cases}"
                }
                "align x plus y equals 3 and x minus y equals 1" => {
                    r"\begin{aligned}x+y&=3\\x-y&=1\end{aligned}"
                }
                _ => "x^2",
            });
        doc.complete(job.id, Ok(Arc::new(math::render(renderer, latex).unwrap())));
    }
}
#[test]
fn mixed_page_baselines_roundtrip_pixels_and_atomic_editing() {
    let source = "# A page of mathematics\n\nThe area is \\integral from 0 to 1 of x^2 dx\\ which gives us \\1 over 3\\.\nWe can keep writing while the next expression takes shape.\n\n\\sum of 1 over n squared from 1 to infinity equals pi squared over 6\\\n\nA matrix \\matrix rows 1 2 and 3 4\\ fits in the same sentence.\n\n\\f of x is x squared for nonnegative x, minus x otherwise\\\n\n\\align x plus y equals 3 and x minus y equals 1\\\n\nand/or, https://example.com/a/b and C:\\dir\\file remain ordinary text.";
    let mut doc = Document::new(source);
    let mut renderer = Renderer::with_prelude(mathstral_latex::PRELUDE);
    finish(&mut doc, &mut renderer);
    let mut t = Typesetter::new();
    let layout = t.document(&doc, 680.0);
    assert_eq!(layout.math.len(), 6);
    for placed in &layout.math {
        let im = if placed.display {
            &placed.image.display
        } else {
            &placed.image.inline
        };
        let line = &layout.lines[layout.row_at(placed.y)];
        assert!(
            placed.y >= line.top
                && placed.y + im.height() * placed.scale <= line.top + line.height + 0.01
        );
        if placed.display {
            assert!((placed.x + im.width() * placed.scale / 2.0 - 340.0).abs() < 0.01);
        } else {
            let glyph = layout
                .glyphs
                .iter()
                .find(|g| g.baseline >= line.top && g.baseline <= line.top + line.height)
                .unwrap();
            assert!((glyph.baseline - (placed.y + im.baseline * placed.scale)).abs() < 0.01);
        }
    }
    let pixels = render::page(
        &layout,
        &mut t,
        &mut cosmic_text::SwashCache::new(),
        1920,
        1520,
        2.0,
        140.0,
        0.0,
        0..0,
    );
    let saved = doc.markdown();
    let mut loaded = Document::new(&saved);
    finish(&mut loaded, &mut renderer);
    assert_eq!(saved, loaded.markdown());
    assert!(!loaded.dirty());
    let after = t.document(&loaded, 680.0);
    let reloaded = render::page(
        &after,
        &mut t,
        &mut cosmic_text::SwashCache::new(),
        1920,
        1520,
        2.0,
        140.0,
        0.0,
        0..0,
    );
    assert_eq!(pixels, reloaded, "saved/reopened page is pixel-identical");
    let pos = *doc.formulas.keys().next().unwrap();
    let latex = doc.formulas[&pos].latex.clone();
    doc.move_to(pos, false);
    assert_eq!(doc.adjacent(true, false), pos + ATOM.len());
    doc.move_to(pos + ATOM.len(), true);
    let copied = doc.selected();
    assert!(copied.starts_with('$'));
    let mut pasted = Document::new("");
    pasted.paste(&copied);
    assert_eq!(pasted.formulas.len(), 1);
    assert!(!pasted.text().contains('\\'));
    doc.move_to(pos + ATOM.len(), false);
    assert_eq!(
        doc.formula_at_caret(false),
        None,
        "Enter after a formula must make a newline"
    );
    assert!(doc.reopen(pos));
    doc.replace("\\", true);
    assert_eq!(doc.formulas[&pos].latex, latex);
    assert!(doc.jobs().is_empty());
    if let Ok(path) = std::env::var("MATHSTRAL_SNAPSHOT") {
        let mut image = tiny_skia::Pixmap::new(1920, 1520).unwrap();
        for (rgba, rgb) in image
            .data_mut()
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(pixels)
        {
            rgba.copy_from_slice(&[(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255]);
        }
        image.save_png(path).unwrap();
        std::fs::write("../artifacts/math-demo.md", saved).unwrap();
    }
}
#[test]
fn late_results_deleted_atoms_and_undo_never_target_other_text() {
    let mut d = Document::new(r"\first\ \second\");
    let jobs = d.jobs();
    let mut renderer = Renderer::with_prelude(mathstral_latex::PRELUDE);
    let image = Arc::new(math::render(&mut renderer, "x^2").unwrap());
    d.move_to(0, false);
    d.replace("prefix ", false);
    assert!(d.complete(jobs[1].id, Ok(image.clone())));
    assert_eq!(
        d.formulas[&(7 + ATOM.len() + 1)].latex.as_deref(),
        Some("x^2")
    );
    d.move_to(7, false);
    d.move_to(7 + ATOM.len(), true);
    d.replace("gone", false);
    assert!(!d.complete(jobs[0].id, Ok(image.clone())));
    assert!(d.text().contains("gone"));
    d.undo(false);
    assert!(d.formulas[&7].image.is_some());
    d.undo(true);
    assert!(d.text().contains("gone"));
    let mut other = Document::new(r"\first\");
    assert!(!other.complete(jobs[0].id, Ok(image)));
}
#[test]
fn ten_pending_prompts_do_not_block_typing() {
    let mut t = Typesetter::new();
    for pending in [0, 10] {
        let mut doc = Document::new(&format!(
            "{}\n",
            "A paragraph of prose to keep writing. ".repeat(20)
        ));
        doc.move_to(doc.rope.len_bytes(), false);
        for i in 0..pending {
            doc.replace(&format!("\\integral number {i}\\ "), false);
        }
        doc.replace("\n", false);
        let mut timings = Vec::new();
        for _ in 0..100 {
            let start = Instant::now();
            doc.replace("a", true);
            let _ = t.document(&doc, 680.0);
            let _ = doc.dirty();
            timings.push(start.elapsed());
        }
        timings.sort();
        eprintln!(
            "{pending} pending prompts: median {:?}, p95 {:?}",
            timings[50], timings[95]
        );
        assert_eq!(doc.formulas.len(), pending);
        if !cfg!(debug_assertions) {
            assert!(timings[95].as_millis() < 16, "typing must fit in a frame");
        }
    }
}

/// Applies Enter the way the app's Enter branch does.
fn press_enter(doc: &mut Document) {
    match markdown::enter_edit(&doc.text(), doc.caret) {
        Some((range, value)) if doc.selection().is_empty() => {
            doc.anchor = range.start;
            doc.caret = range.end;
            doc.replace(&value, false);
        }
        _ => doc.replace("\n", false),
    }
}
#[test]
fn enter_continues_and_ends_list_items_as_undo_steps() {
    let mut d = Document::new("");
    for c in "- a".chars() {
        d.replace(&c.to_string(), true);
    }
    press_enter(&mut d);
    d.replace("b", true);
    press_enter(&mut d);
    press_enter(&mut d);
    assert_eq!(d.text(), "- a\n- b\n");
    d.undo(false);
    assert_eq!(d.text(), "- a\n- b\n- ");
}
