use lvtex_render::math::{Renderer, SCALE};

#[path = "fixtures/formulas.rs"]
mod fixtures;

#[test]
fn fixture_ink_stays_inside_the_canvas() {
    let mut renderer = Renderer::new();
    for formula in fixtures::FORMULAS {
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
fn pdf_export_pages_and_sandbox() {
    let mut renderer = Renderer::new();
    let pdf = renderer
        .compile_pdf("Page one\n#pagebreak()\nPage two".into())
        .unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
    assert!(
        renderer
            .compile_pdf("#read(\"/etc/passwd\")".into())
            .is_err()
    );
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
