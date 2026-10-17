use crate::{
    document::{ATOM, Document},
    keymap::{MathInput, MathKey, Outcome},
    layout::Typesetter,
    markdown, math, render,
    structure::MathTree,
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
    let saved = doc.markdown();
    let mut loaded = Document::new(&saved);
    finish(&mut loaded, &mut renderer);
    assert_eq!(saved, loaded.markdown());
    assert!(!loaded.dirty());
    let mut default_snapshot = None;
    for scale in [1.0_f32, 2.0] {
        for column_width in [280.0_f32, 680.0] {
            let layout = t.document(&doc, column_width);
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
                        && placed.y + im.height() * placed.scale <= line.top + line.height + 0.01,
                    "math stays within its line at {column_width}px, scale {scale}"
                );
                assert!(
                    placed.x >= 0.0 && placed.x + im.width() * placed.scale <= column_width + 0.01,
                    "math stays within its {column_width}px column, scale {scale}"
                );
                if placed.display {
                    assert!(
                        (placed.x + im.width() * placed.scale / 2.0 - column_width / 2.0).abs()
                            < 0.01,
                        "display math stays centered in its {column_width}px column"
                    );
                } else {
                    let glyph = layout
                        .glyphs
                        .iter()
                        .find(|g| g.baseline >= line.top && g.baseline <= line.top + line.height)
                        .unwrap();
                    assert!(
                        (glyph.baseline - (placed.y + im.baseline * placed.scale)).abs() < 0.01,
                        "inline baseline aligns at {column_width}px, scale {scale}"
                    );
                }
            }
            let viewport_width = if column_width == 280.0 { 344.0 } else { 960.0 };
            let width = (viewport_width * scale) as usize;
            let height = (760.0 * scale) as usize;
            let left = (viewport_width - column_width) / 2.0;
            let pixels = render::page(
                &layout,
                &mut t,
                &mut cosmic_text::SwashCache::new(),
                width,
                height,
                scale,
                left,
                0.0,
                0..0,
            );
            let after = t.document(&loaded, column_width);
            let reloaded = render::page(
                &after,
                &mut t,
                &mut cosmic_text::SwashCache::new(),
                width,
                height,
                scale,
                left,
                0.0,
                0..0,
            );
            assert_eq!(
                pixels, reloaded,
                "saved/reopened page is pixel-identical at {column_width}px, scale {scale}"
            );
            if scale == 2.0 && column_width == 680.0 {
                default_snapshot = Some(pixels);
            }
        }
    }
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
    assert!(doc.submit_prompt());
    assert_eq!(doc.formulas[&pos].latex, latex);
    assert!(doc.jobs().is_empty());
    if let Ok(path) = std::env::var("MATHSTRAL_SNAPSHOT") {
        let pixels = default_snapshot.expect("default snapshot configuration was rendered");
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

#[test]
fn hand_written_formula_saves_as_plain_math() {
    let mut doc = Document::new("a ");
    doc.move_to(doc.rope.len_bytes(), false);
    doc.open_manual();
    let mut input = MathInput::new(MathTree::new());
    for c in ['x', '^', '2'] {
        input.key(MathKey::Text(c));
    }
    assert_eq!(input.key(MathKey::Return), Outcome::Close);
    doc.set_manual(input.latex(), None);
    doc.finish_manual();
    assert_eq!(doc.markdown(), "a $x^{2}$");
    let reopened = Document::new(&doc.markdown());
    assert_eq!(reopened.markdown(), "a $x^{2}$");
}

/// Run with `cargo test --release --locked -p mathstral manual_preview_latency -- --ignored --nocapture`.
#[test]
#[ignore = "manual release timing benchmark; run with the command in the test doc comment"]
fn manual_preview_latency() {
    let scripts = [
        ("simple", "x^2⤴+1←←→"),
        ("fraction", r"\frac↵1↓2←→"),
        ("nested fraction", r"\frac↵1↓\frac↵x↓y←→"),
        ("sum", "⇧S_i=1⤴^n⤴i^2←←→"),
    ];
    let mut renderer = Renderer::with_prelude(mathstral_latex::PRELUDE);
    let mut preparation_timings = Vec::new();
    let mut render_timings = Vec::new();
    let mut cold = None;

    for _ in 0..25 {
        for (name, script) in scripts {
            let mut input = MathInput::new(MathTree::new());
            for c in script.chars() {
                let key_start = Instant::now();
                let key = match c {
                    '⇥' => MathKey::Tab,
                    '⇤' => MathKey::ShiftTab,
                    '↵' => MathKey::Return,
                    '⌫' => MathKey::Backspace,
                    '⌦' => MathKey::Delete,
                    '←' => MathKey::Left,
                    '→' => MathKey::Right,
                    '↑' => MathKey::Up,
                    '↓' => MathKey::Down,
                    '⇧' => MathKey::SymbolPrefix,
                    '⤴' => MathKey::ExitRight,
                    _ => MathKey::Text(c),
                };
                let outcome = input.key(key);
                if matches!(outcome, Outcome::Edited | Outcome::Moved) {
                    let _source_latex = input.latex();
                    let latex = input.display_latex();
                    preparation_timings.push(key_start.elapsed());
                    let render_start = Instant::now();
                    math::render(&mut renderer, &latex)
                        .unwrap_or_else(|error| panic!("{name} preview {latex:?}: {error}"));
                    let elapsed = render_start.elapsed();
                    cold.get_or_insert((name, elapsed));
                    render_timings.push(elapsed);
                }
            }
        }
    }

    assert!(
        render_timings.len() >= 100,
        "expected about 100 rendered edits, got {}",
        render_timings.len()
    );
    preparation_timings.sort_unstable();
    render_timings.sort_unstable();
    let preparation_median = preparation_timings[preparation_timings.len() / 2];
    let preparation_p95 = preparation_timings[preparation_timings.len() * 95 / 100];
    let render_median = render_timings[render_timings.len() / 2];
    let render_p95 = render_timings[render_timings.len() * 95 / 100];
    let (cold_name, cold_time) = cold.unwrap();
    eprintln!(
        "manual formula preview: {} edited/moved outcomes; input preparation median {:?}, p95 {:?}; cold first render ({cold_name}) {:?}, render median {:?}, p95 {:?}",
        render_timings.len(),
        preparation_median,
        preparation_p95,
        cold_time,
        render_median,
        render_p95
    );
}
