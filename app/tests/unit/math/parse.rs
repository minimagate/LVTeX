use super::*;

#[test]
fn parser_retains_deep_input_raw_and_keeps_shallow_trees_editable() {
    let at_limit = format!(
        "{}x{}",
        r"\sqrt{".repeat(MAX_PARSE_DEPTH),
        "}".repeat(MAX_PARSE_DEPTH)
    );
    let parsed_at_limit = MathTree::parse(&at_limit);
    assert_eq!(parsed_at_limit.latex(), at_limit);
    assert!(matches!(
        parsed_at_limit.root.first(),
        Some(Node::Root { .. })
    ));

    let nested_braced = format!("{}x{}", r"\sqrt{".repeat(80), "}".repeat(80));
    let nested_unbraced = format!("{}x", r"\sqrt".repeat(80));
    for (name, source) in [
        ("braced", nested_braced.as_str()),
        ("unbraced", nested_unbraced.as_str()),
    ] {
        let parsed = MathTree::parse(source);
        assert_eq!(parsed.latex(), source, "{name} source changed");
        assert!(matches!(parsed.root.as_slice(), [Node::Raw(raw)] if raw == source));
    }

    let mut shallow = MathTree::parse(r"\sqrt{x^2}");
    assert!(matches!(shallow.root.first(), Some(Node::Root { .. })));
    shallow.insert_symbol("y", "y");
    assert_eq!(shallow.latex(), r"\sqrt{x^{2}}y");
}

#[test]
fn standalone_groups_and_unknown_command_arguments_are_preserved() {
    let cases = [
        ("{ab}^2", "{ab}^{2}"),
        ("{{a}{b}}", "{{a}{b}}"),
        ("{}", "{}"),
        (r"\textcolor{red}{ab}", r"\textcolor{red}{ab}"),
        (r"\unknown{a}{b}{c}", r"\unknown{a}{b}{c}"),
    ];

    for (input, expected) in cases {
        let parsed = MathTree::parse(input).latex();
        assert_eq!(parsed, expected, "input {input:?}");
        assert_eq!(
            MathTree::parse(&parsed).latex(),
            expected,
            "re-parse of {input:?}"
        );
    }
}

#[test]
fn malformed_latex_never_panics_and_normalises() {
    let inputs = [
        "\\",
        "\\left",
        "\\left(",
        "\\begin{x",
        "\\frac{",
        "\\sqrt[",
        "^{",
        "{{{",
        "_",
        "\\right",
        "\\operatorname",
        "é^é_",
        "\\{",
        "\\text{a",
        "a}b{",
        "\\frac}{",
        "\\left\\right",
        "\\end{x}",
        "%&#$~",
        "\u{1F600}^\u{1F600}",
    ];
    for input in inputs {
        let once = MathTree::parse(input).latex();
        assert_eq!(MathTree::parse(&once).latex(), once, "input {input:?}");
    }
}
