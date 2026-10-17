use super::*;

#[test]
fn scripts_build_and_nest() {
    let cases = [
        (
            "x^{2}",
            build(|t| {
                sym(t, "x");
                t.make_script(true);
                sym(t, "2");
            }),
        ),
        (
            "a_{i}^{2}",
            build(|t| {
                sym(t, "a");
                t.make_script(false);
                sym(t, "i");
                t.exit(true);
                t.make_script(true);
                sym(t, "2");
            }),
        ),
        (
            "{}_{0}",
            build(|t| {
                t.make_script(false);
                sym(t, "0");
            }),
        ),
        (
            "x^{1}{}^{2}",
            build(|t| {
                sym(t, "x");
                t.make_script(true);
                sym(t, "1");
                t.exit(true);
                t.make_script(true);
                sym(t, "2");
            }),
        ),
    ];
    for (expected, tree) in cases {
        assert_eq!(tree.latex(), expected);
    }
}

#[test]
fn fraction_by_make_then_down() {
    let t = build(|t| {
        sym(t, "a");
        t.make_fraction();
        sym(t, "a");
        t.down();
        sym(t, "b");
    });
    assert_eq!(t.latex(), r"a\frac{a}{b}");
    let mut t = build(|t| {
        t.make_fraction();
        sym(t, "a");
        t.down();
        sym(t, "b");
    });
    assert_eq!(t.latex(), r"\frac{a}{b}");
    t.up();
    assert_eq!((t.path.clone(), t.pos), (vec![(0, 0)], 1));
}

#[test]
fn nested_fraction_in_a_script() {
    let mut t = build(|t| {
        sym(t, "x");
        t.make_script(true);
        t.make_fraction();
        sym(t, "1");
        t.down();
        sym(t, "2");
        t.exit(true);
        t.exit(true);
        sym(t, "y");
    });
    assert_eq!(t.latex(), r"x^{\frac{1}{2}}y");
    t.exit(false);
    assert_eq!((t.path.clone(), t.pos), (vec![], 3));
}

#[test]
fn backspace_enters_a_box_then_removes_it_when_empty() {
    let mut t = build(|t| {
        t.make_fraction();
        sym(t, "a");
        t.down();
        sym(t, "b");
    });
    t.exit(true);
    t.backspace();
    assert_eq!(t.latex(), r"\frac{a}{b}");
    assert_eq!((t.path.clone(), t.pos), (vec![(0, 1)], 1));

    let mut empty = build(|t| t.make_fraction());
    empty.backspace();
    assert!(empty.latex().is_empty());
}

#[test]
fn backspace_at_an_argument_start_moves_or_keeps_the_box() {
    let mut t = build(|t| {
        t.make_fraction();
        sym(t, "a");
        t.down();
        sym(t, "b");
    });
    t.backspace();
    assert_eq!(t.latex(), r"\frac{a}{}");
    t.backspace();
    assert_eq!((t.path.clone(), t.pos), (vec![(0, 0)], 1));
    t.left();
    t.backspace();
    assert_eq!(t.latex(), r"\frac{a}{}");
    assert_eq!((t.path.clone(), t.pos), (vec![], 0));
}

#[test]
fn delimiter_hides_then_unwraps_on_backspace() {
    let mut t = build(|t| {
        t.make_delimited("(", ")");
        sym(t, "x");
    });
    t.left();
    t.backspace();
    assert_eq!(t.latex(), r"\left.x\right)");
    t.backspace();
    assert_eq!(t.latex(), r"\left.x\right.");
    t.backspace();
    assert_eq!(t.latex(), "x");
    assert_eq!((t.path.clone(), t.pos), (vec![], 0));
}

#[test]
fn delete_mirrors_backspace_on_delimiters() {
    let mut t = build(|t| {
        t.make_delimited("(", ")");
        sym(t, "x");
    });
    t.delete();
    assert_eq!(t.latex(), r"\left(x\right.");
    t.delete();
    assert_eq!(t.latex(), r"\left.x\right.");
    t.delete();
    assert_eq!(t.latex(), "x");
    assert_eq!((t.path.clone(), t.pos), (vec![], 1));
}

#[test]
fn delete_removes_leaves_and_empty_boxes() {
    let mut t = build(|t| {
        sym(t, "a");
        t.make_fraction();
        sym(t, "1");
        t.exit(false);
        t.exit(false);
    });
    assert_eq!(t.latex(), r"a\frac{1}{}");
    t.delete();
    assert_eq!((t.path.clone(), t.pos), (vec![(1, 0)], 0));
    t.delete();
    assert_eq!(t.latex(), r"a\frac{}{}");
    t.delete();
    assert_eq!((t.path.clone(), t.pos), (vec![(1, 1)], 0));
    t.delete();
    assert_eq!(t.latex(), "a");
    assert_eq!((t.path.clone(), t.pos), (vec![], 1));
}

#[test]
fn remove_box_removes_the_node_before_the_caret() {
    let mut t = build(|t| {
        sym(t, "a");
        t.make_fraction();
        sym(t, "1");
        t.exit(true);
    });
    t.remove_box();
    assert_eq!(t.latex(), "a");
    assert_eq!((t.path.clone(), t.pos), (vec![], 1));
}

#[test]
fn unwrap_enclosing_keeps_contents_and_caret() {
    let mut t = build(|t| {
        t.make_fraction();
        sym(t, "a");
        t.down();
        sym(t, "b");
    });
    t.unwrap_enclosing();
    assert_eq!(t.latex(), "ab");
    assert_eq!((t.path.clone(), t.pos), (vec![], 2));
    sym(&mut t, "c");
    assert_eq!(t.latex(), "abc");
}

#[test]
fn set_right_delimiter_only_at_the_body_end() {
    let mut t = build(|t| {
        t.make_delimited("[", ")");
        sym(t, "x");
    });
    t.left();
    assert!(!t.set_right_delimiter("]"));
    t.right();
    assert!(t.set_right_delimiter("]"));
    assert_eq!(t.latex(), r"\left[x\right]");
    sym(&mut t, "y");
    assert_eq!(t.latex(), r"\left[x\right]y");
}

#[test]
fn keys_before_caret_and_replace_before_work_on_the_run() {
    let mut t = build(|t| {
        t.insert_symbol("<", "<");
        t.insert_symbol("=", "=");
    });
    assert_eq!(t.keys_before_caret(5), ["<", "="]);
    assert_eq!(t.keys_before_caret(1), ["="]);
    t.replace_before(2, r"\leqslant", "< =");
    assert_eq!(t.latex(), r"\leqslant");
    assert_eq!(t.keys_before_caret(5), ["< ="]);
    t.make_prime();
    assert!(t.keys_before_caret(5).is_empty());
}

#[test]
fn symbols_mut_visits_symbols_inside_boxes() {
    let mut tree = MathTree::parse(r"a\frac{b}{c^d}");
    let mut seen = Vec::new();
    tree.symbols_mut(|latex, keys| {
        seen.push(latex.to_string());
        keys.push('!');
    });
    assert_eq!(seen, ["a", "b", "c", "d"]);
    let mut keys = Vec::new();
    tree.symbols_mut(|_, k| keys.push(k.clone()));
    assert_eq!(keys, ["!", "!", "!", "!"]);
    assert_eq!(tree.latex(), r"a\frac{b}{c^{d}}");
}
