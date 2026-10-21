use lvtex_latex::{PRELUDE, convert, typst_string_literal};
use lvtex_render::math::{Renderer, SCALE};

#[path = "fixtures/formulas.rs"]
mod fixtures;
use fixtures::FORMULAS;

#[test]
fn all_latex_cases_render_without_clipping() {
    let mut renderer = Renderer::with_prelude(PRELUDE);
    for formula in FORMULAS {
        let body = convert(formula.latex).unwrap();
        for display in [false, true] {
            let image = renderer
                .render_math(&body, display, SCALE)
                .unwrap_or_else(|e| panic!("{}: {e}\n{body}", formula.name));
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
fn errors_and_helper_variations() {
    let mut renderer = Renderer::with_prelude(PRELUDE);
    for latex in [r"\notARealCommand{x}", r"\frac{", r"\begin{pmatrix}a&b"] {
        assert!(
            convert(latex)
                .and_then(|s| renderer.render_math(&s, true, SCALE))
                .is_err(),
            "accepted {latex}"
        );
    }
    for latex in [
        r"\sqrt[3]{x}",
        r"\text{diff angle.l}",
        r"\angle ABC + \partial_x f",
        r"\begin{vmatrix}a&b\\c&d\end{vmatrix}",
        r"\text{\{x\}} + \alpha",
        "x % ignored } \\end{matrix}\n + y",
    ] {
        let body = convert(latex).unwrap();
        renderer
            .render_math(&body, true, SCALE)
            .unwrap_or_else(|e| panic!("{latex}: {e}\n{body}"));
    }
}

#[test]
fn typst_string_literal_escapes_quotes_and_backslashes() {
    assert_eq!(typst_string_literal(r#"a"b\c"#), r#""a\"b\\c""#);
    assert_eq!(typst_string_literal("x\ny"), r#""x\ny""#);
}

#[test]
fn mitex_helpers_render() {
    let mut renderer = Renderer::with_prelude(PRELUDE);
    for latex in [
        r"\begin{cases}x\equiv a \pmod{32}\\x\equiv 1 \pmod{35}\end{cases}",
        r"a \pod{n}",
        r"\dbinom{n}{k}",
        r"\tfrac{1}{2}",
        r"\dfrac{a}{b}",
        r"\underbrace{a+b}_{n}",
        r"\overset{?}{=}",
        r"\xrightarrow{f}",
        r"\boxed{x}",
        r"\operatorname{rank}A",
        r"\begin{array}{cc}1&2\end{array}",
        r"\sum_{\substack{i\\j}}",
        r"\stackrel{a}{=}",
        r"x\phantom{x}y",
        r"\displaystyle x",
        r"\color{red}x",
        r"\mathbin{\star}",
        r"\big( x \big)",
    ] {
        let body = convert(latex).unwrap();
        for display in [false, true] {
            renderer
                .render_math(&body, display, SCALE)
                .unwrap_or_else(|e| panic!("{latex} display={display}: {e}\n{body}"));
        }
    }
}

#[test]
fn textcolor_renders_hex_named_and_malformed_colors_safely() {
    let mut renderer = Renderer::with_prelude(PRELUDE);
    let render = |renderer: &mut Renderer, latex: &str| {
        let body = convert(latex).unwrap();
        renderer.render_math(&body, true, SCALE).unwrap()
    };

    for (latex, expected) in [
        (r"\textcolor{#87CEEB}{|}", [135_u8, 206, 235]),
        (r"\textcolor{red}{x}", [255_u8, 0, 0]),
    ] {
        let image = render(&mut renderer, latex);
        let pixel = image
            .pixels()
            .iter()
            .filter(|pixel| pixel.alpha() > 0)
            .max_by_key(|pixel| pixel.alpha())
            .expect("colored glyph has visible pixels");
        let alpha = u32::from(pixel.alpha());
        for (actual, expected) in [pixel.red(), pixel.green(), pixel.blue()]
            .into_iter()
            .zip(expected)
        {
            let unpremultiplied = u32::from(actual) * 255 / alpha;
            assert!(
                unpremultiplied.abs_diff(u32::from(expected)) <= 2,
                "{latex}: expected RGB {expected:?}, got premultiplied pixel {pixel:?}"
            );
        }
    }

    let malformed = render(&mut renderer, r"\textcolor{not-a-color}{x}");
    let plain = render(&mut renderer, "x");
    assert_eq!(malformed.pixels(), plain.pixels());
}

#[test]
fn substituted_symbols_convert_and_render() {
    let mut renderer = Renderer::with_prelude(PRELUDE);
    for latex in [
        r"\iiiint",
        r"\preceqq\cdots\preceqq",
        r"\not\succeqq",
        r"\Prec\cdots\Prec",
        r"\leftsquigarrow\cdots\leftsquigarrow",
        r"\dotminus\dottimes",
        r"\iddots",
    ] {
        let body = convert(latex).unwrap_or_else(|e| panic!("{latex}: {e}"));
        renderer
            .render_math(&body, false, SCALE)
            .unwrap_or_else(|e| panic!("{latex}: {e}\n{body}"));
    }
}
