use super::*;

#[test]
fn latex_with_caret_marks_caret_and_empty_arguments() {
    let frac = build(|t| {
        t.make_fraction();
    });
    assert_eq!(
        frac.latex_with_caret(),
        format!(
            r"\frac{{\mathord{{\textcolor{{#{CARET_SENTINEL:06X}}}{{|}}}}\textcolor{{#73716F}}{{\square}}}}{{\textcolor{{#73716F}}{{\square}}}}"
        )
    );
    assert_eq!(frac.latex(), r"\frac{}{}");
    assert_eq!(
        MathTree::new().latex_with_caret(),
        format!(r"\mathord{{\textcolor{{#{CARET_SENTINEL:06X}}}{{|}}}}")
    );
    let script = build(|t| {
        sym(t, "x");
        t.make_script(true);
        sym(t, "2");
    });
    assert_eq!(
        script.latex_with_caret(),
        format!(r"x^{{2\mathord{{\textcolor{{#{CARET_SENTINEL:06X}}}{{|}}}}}}")
    );
}

#[test]
fn rendered_caret_adds_no_relation_spacing_to_letter_runs() {
    let marker = format!(r"\mathord{{\textcolor{{#{CARET_SENTINEL:06X}}}{{|}}}}");
    let old_marker = format!(r"\textcolor{{#{CARET_SENTINEL:06X}}}{{|}}");
    let mut letters = build(|tree| {
        for c in "abcd".chars() {
            sym(tree, &c.to_string());
        }
    });
    let mut renderer = mathstral_render::math::Renderer::with_prelude(mathstral_latex::PRELUDE);

    for caret_pos in [0, 2, 4] {
        letters.pos = caret_pos;
        let with_caret = letters.latex_with_caret();
        let without_ordinary_class = with_caret.replace(&marker, &old_marker);
        let render = |renderer: &mut mathstral_render::math::Renderer, latex: &str| {
            let body = mathstral_latex::convert(latex).unwrap();
            renderer
                .render_math(&body, true, mathstral_render::math::SCALE)
                .unwrap()
        };
        let current = render(&mut renderer, &with_caret);
        let old = render(&mut renderer, &without_ordinary_class);
        assert!(
            current.pixels().iter().any(|pixel| {
                pixel.alpha() > 0 && pixel.green() <= 2 && pixel.red() > 0 && pixel.blue() > 0
            }),
            "caret at {caret_pos} lost its sentinel colour"
        );
        if caret_pos == 0 {
            assert_eq!(current.width(), old.width());
        } else {
            assert!(
                current.width() < old.width(),
                "caret at {caret_pos}: ordinary atom width {}, old width {}",
                current.width(),
                old.width()
            );
        }
    }

    for mut tree in [
        build(|tree| {
            sym(tree, "x");
            tree.make_script(true);
        }),
        build(|tree| tree.make_fraction()),
    ] {
        let body = mathstral_latex::convert(&tree.latex_with_caret()).unwrap();
        let image = renderer
            .render_math(&body, true, mathstral_render::math::SCALE)
            .unwrap();
        assert!(
            image
                .pixels()
                .iter()
                .any(|pixel| pixel.alpha() > 0 && pixel.green() == 0 && pixel.red() > 0)
        );
        tree.next_argument();
        let body = mathstral_latex::convert(&tree.latex_with_caret()).unwrap();
        let image = renderer
            .render_math(&body, true, mathstral_render::math::SCALE)
            .unwrap();
        assert!(
            image
                .pixels()
                .iter()
                .any(|pixel| pixel.alpha() > 0 && pixel.green() == 0 && pixel.red() > 0)
        );
    }
}

#[test]
fn built_trees_emit_expected_latex_and_round_trip() {
    let cases = [
        (
            r"\frac{a}{b}",
            build(|t| {
                t.make_fraction();
                sym(t, "a");
                t.down();
                sym(t, "b");
            }),
        ),
        (
            r"x^{\frac{1}{2}}",
            build(|t| {
                sym(t, "x");
                t.make_script(true);
                t.make_fraction();
                sym(t, "1");
                t.down();
                sym(t, "2");
            }),
        ),
        (
            r"\sqrt{x}",
            build(|t| {
                t.make_sqrt();
                sym(t, "x");
            }),
        ),
        (
            r"\sqrt[3]{x}",
            build(|t| {
                t.make_root();
                sym(t, "3");
                t.down();
                sym(t, "x");
            }),
        ),
        (r"\frac{}{}", build(|t| t.make_fraction())),
        (r"\sqrt[]{}", build(|t| t.make_root())),
        (
            r"\alpha x",
            build(|t| {
                t.insert_symbol(r"\alpha", "a");
                sym(t, "x");
            }),
        ),
        (
            r"\frac{\alpha}{\beta}",
            build(|t| {
                t.make_fraction();
                t.insert_symbol(r"\alpha", "a");
                t.down();
                t.insert_symbol(r"\beta", "b");
            }),
        ),
        (
            r"f'\left(x\right)",
            build(|t| {
                sym(t, "f");
                t.make_prime();
                t.make_delimited("(", ")");
                sym(t, "x");
            }),
        ),
        (
            r"\in\mathbb{R}",
            build(|t| {
                t.insert_symbol(r"\in", "i");
                t.insert(Node::Raw(r"\mathbb{R}".to_string()));
            }),
        ),
        (
            r"\operatorname{argmax}_{x}",
            build(|t| {
                t.insert(Node::Raw(r"\operatorname{argmax}".to_string()));
                t.make_script(false);
                sym(t, "x");
            }),
        ),
        (
            r"\begin{pmatrix}1&2\\3&4\end{pmatrix}",
            build(|t| {
                t.insert(Node::Raw(
                    r"\begin{pmatrix}1&2\\3&4\end{pmatrix}".to_string(),
                ))
            }),
        ),
        (
            r"\left\{x\right\}",
            build(|t| {
                t.make_delimited("{", "}");
                sym(t, "x");
            }),
        ),
        (
            r"\left.x\right.",
            build(|t| {
                t.make_delimited(".", ".");
                sym(t, "x");
            }),
        ),
        (
            r"\left\langle x\right\rangle",
            build(|t| {
                t.make_delimited(r"\langle", r"\rangle");
                sym(t, "x");
            }),
        ),
        (
            r"\left(\left[x\right]\right)",
            build(|t| {
                t.make_delimited("(", ")");
                t.make_delimited("[", "]");
                sym(t, "x");
            }),
        ),
    ];
    for (expected, tree) in cases {
        assert_eq!(tree.latex(), expected);
        let parsed = MathTree::parse(expected);
        assert_eq!(parsed.latex(), expected, "parse of {expected}");
        assert_eq!(MathTree::parse(&tree.latex()).latex(), tree.latex());
    }
}

#[test]
fn parses_hand_written_latex() {
    let cases = [
        (r"\int_0^1 x^2\,dx", r"\int_{0}^{1}x^{2}\,dx"),
        (r"\sum_{n=1}^{\infty} n^2", r"\sum_{n=1}^{\infty}n^{2}"),
        (
            r"\begin{pmatrix}1&2\\3&4\end{pmatrix}",
            r"\begin{pmatrix}1&2\\3&4\end{pmatrix}",
        ),
        (r"\{x\in\mathbb{R}\mid x>0\}", r"\{x\in\mathbb{R}\mid x>0\}"),
        (r"\frac{a", r"\frac{a}{}"),
        (r"\left( x", r"\left(x\right."),
        (
            r"\left\langle x \right\rangle",
            r"\left\langle x\right\rangle",
        ),
        (r"\sqrt[3]{x}", r"\sqrt[3]{x}"),
        (r"x'", r"x'"),
        (r"\left\{ x \right\}", r"\left\{x\right\}"),
        ("{}_0", "{}_{0}"),
        (r"a_{}", r"a_{}"),
        (r"\mathbb R", r"\mathbb R"),
        (r"\alpha x", r"\alpha x"),
        ("x^", "x^{}"),
        ("}", "}"),
        (r"\right)", r"\right)"),
        (r"\rightarrow x", r"\rightarrow x"),
    ];
    for (input, expected) in cases {
        let parsed = MathTree::parse(input).latex();
        assert_eq!(parsed, expected, "input {input:?}");
        assert_eq!(
            MathTree::parse(&parsed).latex(),
            parsed,
            "re-parse of {input:?}"
        );
    }
    for input in [
        r"\int_0^1 x^2\,dx",
        r"\sum_{n=1}^{\infty} n^2",
        r"\begin{pmatrix}1&2\\3&4\end{pmatrix}",
        r"\{x\in\mathbb{R}\mid x>0\}",
        r"\frac{a",
        r"\left( x",
    ] {
        assert!(
            braces_balance(&MathTree::parse(input).latex()),
            "unbalanced {input:?}"
        );
    }
}
