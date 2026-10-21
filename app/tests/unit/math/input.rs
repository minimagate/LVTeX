use super::*;
use lvtex_render::math::Renderer;

/// Feeds one key per character. The markers stand for the keys that are not characters, and
/// the outcome of the last key is returned.
fn feed(input: &mut MathInput, script: &str) -> Outcome {
    let mut last = Outcome::Ignored;
    for c in script.chars() {
        let key = match c {
            '⇥' => MathKey::Tab,
            '⇤' => MathKey::ShiftTab,
            '↵' => MathKey::Return,
            '⎋' => MathKey::Escape,
            '⌫' => MathKey::Backspace,
            '⌦' => MathKey::Delete,
            '←' => MathKey::Left,
            '→' => MathKey::Right,
            '↑' => MathKey::Up,
            '↓' => MathKey::Down,
            '⇧' => MathKey::SymbolPrefix,
            _ => MathKey::Text(c),
        };
        last = input.key(key);
    }
    last
}

fn typed(script: &str) -> MathInput {
    let mut input = MathInput::new(MathTree::new());
    feed(&mut input, script);
    input
}

fn parsed(latex: &str) -> MathInput {
    MathInput::new(MathTree::parse(latex))
}

#[test]
fn parsed_letter_takes_its_tab_variant() {
    let mut input = parsed("a");
    assert_eq!(input.key(MathKey::Tab), Outcome::Edited);
    assert_eq!(input.latex(), r"\alpha");
}

#[test]
fn parsed_variant_has_no_further_tab_and_shift_tab_steps_back() {
    let mut input = parsed(r"\alpha");
    assert_eq!(input.key(MathKey::Tab), Outcome::Ignored);
    assert_eq!(input.key(MathKey::ShiftTab), Outcome::Edited);
    assert_eq!(input.latex(), "a");
}

#[test]
fn parsed_relation_combines_like_typed_keys() {
    let mut input = parsed("<");
    input.key(MathKey::Text('='));
    assert_eq!(input.latex(), r"\leqslant");
    assert_eq!(input.latex(), typed("<=").latex());
}

#[test]
fn parsed_comma_keeps_its_typed_key_not_a_variant() {
    let mut input = parsed(",");
    feed(&mut input, ",");
    assert_eq!(input.latex(), r",\ldots,");
}

#[test]
fn symbol_inside_a_parsed_fraction_takes_keys() {
    let mut input = parsed(r"\frac{a}{b}");
    for _ in 0..3 {
        input.key(MathKey::Left);
    }
    assert_eq!(input.key(MathKey::Tab), Outcome::Edited);
    assert_eq!(input.latex(), r"\frac{\alpha}{b}");
}

#[test]
fn every_binding_renders() {
    let mut renderer = Renderer::with_prelude(lvtex_latex::PRELUDE);
    let failed: Vec<String> = BINDINGS
        .iter()
        .filter(|(_, latex)| crate::math::render::render(&mut renderer, latex).is_err())
        .map(|(keys, latex)| format!("{keys} -> {latex}"))
        .collect();
    assert!(
        failed.is_empty(),
        "bindings that do not render: {failed:#?}"
    );
}

#[test]
fn bindings_fit_the_run_limit() {
    for (keys, _) in BINDINGS {
        let symbols = keys.split(' ').filter(|key| *key != "var").count();
        assert!(symbols <= MAX_RUN, "{keys} has {symbols} keys");
    }
}

#[test]
fn display_latex_renders_for_every_state() {
    let mut renderer = Renderer::with_prelude(lvtex_latex::PRELUDE);
    let mut empty_fraction = MathInput::new(MathTree::new());
    empty_fraction.key(MathKey::Alt {
        key: 'f',
        shift: false,
    });
    let states = [
        MathInput::new(MathTree::new()),
        empty_fraction,
        typed("x^a^b"),
        typed(r"\al"),
    ];
    for input in states {
        let latex = input.display_latex();
        assert!(
            crate::math::render::render(&mut renderer, &latex).is_ok(),
            "display {latex}"
        );
    }
    assert!(
        typed(r"\al")
            .display_latex()
            .contains(r"\textcolor{#73716F}{\backslash\hspace{-0.15em}\mathrm{al}}")
    );
}

/// The widest run of columns with no ink between the first and last inked column, in px.
fn widest_ink_gap(image: &crate::math::render::Image) -> usize {
    let inked: Vec<bool> = (0..image.width)
        .map(|x| (0..image.height).any(|y| image.rgba[(y * image.width + x) * 4 + 3] > 64))
        .collect();
    let (Some(first), Some(last)) = (
        inked.iter().position(|&ink| ink),
        inked.iter().rposition(|&ink| ink),
    ) else {
        return 0;
    };
    let (mut widest, mut run) = (0, 0);
    for &ink in &inked[first..=last] {
        run = if ink { 0 } else { run + 1 };
        widest = widest.max(run);
    }
    widest
}

#[test]
fn pending_command_backslash_sits_tight_against_its_letters() {
    let mut renderer = Renderer::with_prelude(lvtex_latex::PRELUDE);
    let latex = typed(r"\in").display_latex();
    // The live preview erases the caret marker, so the gap is measured between the command and
    // the letters only.
    let formula = crate::math::render::render_preview(&mut renderer, &latex)
        .unwrap_or_else(|error| panic!("display {latex}: {error}"));
    let gap = widest_ink_gap(&formula.inline);
    // Two pixels per pt: a gap wider than 2 pt is a visible space inside the command.
    assert!(
        gap <= 4,
        "gap of {gap} px ({} pt) inside {latex}",
        gap as f32 / 2.0
    );
}

#[test]
fn texmacs_bindings_apply_to_the_symbols_before_the_caret() {
    let cases = [
        ("<=", r"\leqslant"),
        ("<", "<"),
        ("->", r"\rightarrow"),
        ("-->", r"\longrightarrow"),
        ("=/", r"\neq"),
        ("a⇥", r"\alpha"),
        ("a⇥⇥", r"\alpha"),
        ("a⇥⇤", "a"),
        ("&", r"\wedge"),
        ("&⇥", r"\cap"),
        ("&..", r"\wedge\cdots\wedge"),
        ("⇧S", r"\sum"),
        ("⇧n⇥", r"\sqcap"),
        ("<⇥/", r"\notin"),
        ("=⇥⇥/", r"\not\equiv"),
        ("<⇥⇥/", r"\not\subset"),
        ("<<⇥", r"\Prec"),
        ("<<⇥..", r"\Prec\cdots\Prec"),
        ("Q⇥", "X"),
        ("R⇥", "P"),
    ];
    for (script, expected) in cases {
        assert_eq!(typed(script).latex(), expected, "script {script:?}");
    }
}

#[test]
fn backslash_commands_confirm_on_return_or_before_another_key() {
    let cases = [
        ("\\alpha↵", r"\alpha"),
        ("\\alpha^2", r"\alpha^{2}"),
        ("\\frac↵1↓2", r"\frac{1}{2}"),
        ("\\sqrt↵x", r"\sqrt{x}"),
        ("\\↵", ""),
    ];
    for (script, expected) in cases {
        assert_eq!(typed(script).latex(), expected, "script {script:?}");
    }
}

#[test]
fn brackets_pair_and_closers_step_out() {
    let cases = [
        ("(x)", r"\left(x\right)"),
        ("(x]", r"\left(x\right]"),
        ("|x|", r"\left|x\right|"),
        ("[x", r"\left[x\right]"),
        ("x)", "x)"),
    ];
    for (script, expected) in cases {
        assert_eq!(typed(script).latex(), expected, "script {script:?}");
    }
}

#[test]
fn option_keys_make_fractions_and_roots() {
    let mut frac = MathInput::new(MathTree::new());
    frac.key(MathKey::Alt {
        key: 'f',
        shift: false,
    });
    feed(&mut frac, "1↓2");
    assert_eq!(frac.latex(), r"\frac{1}{2}");

    let mut root = MathInput::new(MathTree::new());
    root.key(MathKey::Alt {
        key: 's',
        shift: true,
    });
    feed(&mut root, "3↓x");
    assert_eq!(root.latex(), r"\sqrt[3]{x}");

    let mut ignored = MathInput::new(MathTree::new());
    let outcome = ignored.key(MathKey::Alt {
        key: 'q',
        shift: false,
    });
    assert_eq!(outcome, Outcome::Ignored);
}

#[test]
fn return_closes_escape_cancels_and_escape_drops_only_a_command() {
    assert_eq!(typed("x").key(MathKey::Return), Outcome::Close);
    assert_eq!(
        MathInput::new(MathTree::new()).key(MathKey::Escape),
        Outcome::Cancel
    );
    let mut input = typed("x");
    let preview = input.display_latex();
    assert_eq!(feed(&mut input, r"\a⎋"), Outcome::Edited);
    assert_eq!(input.latex(), "x");
    assert_eq!(input.display_latex(), preview);
    assert_eq!(input.key(MathKey::Return), Outcome::Close);
}

#[test]
fn finishing_confirms_pending_commands() {
    for (keys, expected) in [
        (r"\alpha", r"\alpha"),
        (r"\unknownname", r"\unknownname"),
        (r"\frac", r"\frac{}{}"),
        (r"\sqrt", r"\sqrt{}"),
    ] {
        assert_eq!(typed(keys).finish(), expected, "command {keys:?}");
    }
}

#[test]
fn undo_clearing_a_pending_command_requests_redraw() {
    let mut input = typed(r"\alpha");
    assert_eq!(input.key(MathKey::Undo), Outcome::Edited);
    assert!(input.latex().is_empty());
    assert_eq!(
        input.display_latex(),
        MathInput::new(MathTree::new()).display_latex()
    );
    assert_eq!(input.key(MathKey::Undo), Outcome::Ignored);
}

#[test]
fn navigation_and_structural_keys_edit_the_tree() {
    assert_eq!(typed("ab←⌫").latex(), "b");
    assert_eq!(typed("ab←←⌦").latex(), "b");
    assert_eq!(typed("ab←←→x").latex(), "axb");

    let mut frac = MathInput::new(MathTree::new());
    frac.key(MathKey::Alt {
        key: 'f',
        shift: false,
    });
    feed(&mut frac, "1↓2↑3");
    assert_eq!(frac.latex(), r"\frac{13}{2}");
    frac.key(MathKey::ExitLeft);
    feed(&mut frac, "x");
    assert_eq!(frac.latex(), r"x\frac{13}{2}");

    let mut args = MathInput::new(MathTree::new());
    args.key(MathKey::Alt {
        key: 'f',
        shift: false,
    });
    feed(&mut args, "1");
    assert_eq!(args.key(MathKey::NextArgument), Outcome::Moved);
    feed(&mut args, "2");
    assert_eq!(args.key(MathKey::PreviousArgument), Outcome::Moved);
    feed(&mut args, "0");
    assert_eq!(args.latex(), r"\frac{10}{2}");

    let mut whole = typed("a");
    whole.key(MathKey::Alt {
        key: 'f',
        shift: false,
    });
    whole.key(MathKey::ExitRight);
    assert_eq!(whole.key(MathKey::StructuredBackspace), Outcome::Edited);
    assert_eq!(whole.latex(), "a");

    let mut unwrapped = MathInput::new(MathTree::new());
    unwrapped.key(MathKey::Alt {
        key: 'f',
        shift: false,
    });
    feed(&mut unwrapped, "1↓2");
    assert_eq!(unwrapped.key(MathKey::Unwrap), Outcome::Edited);
    assert_eq!(unwrapped.latex(), "12");
}

#[test]
fn undo_and_redo_step_through_edits_but_not_movement() {
    let mut input = typed("xy");
    assert_eq!(input.key(MathKey::Undo), Outcome::Edited);
    assert_eq!(input.latex(), "x");
    assert_eq!(input.key(MathKey::Redo), Outcome::Edited);
    assert_eq!(input.latex(), "xy");
    input.key(MathKey::Undo);
    input.key(MathKey::Undo);
    assert!(input.latex().is_empty());
    assert_eq!(input.key(MathKey::Undo), Outcome::Ignored);

    let mut moved = typed("x←");
    moved.key(MathKey::Undo);
    assert!(moved.latex().is_empty());
}

#[test]
fn imported_latex_survives_noop_motion_and_undo() {
    let cases = [("whitespace", " x ^2 "), ("unbraced script", "x^2")];
    for (name, source) in cases {
        let mut input = MathInput::from_latex(source);
        assert_eq!(input.latex(), source, "{name}");
        assert_eq!(input.key(MathKey::Left), Outcome::Moved, "{name}");
        assert_eq!(input.key(MathKey::Right), Outcome::Moved, "{name}");
        assert_eq!(input.latex(), source, "{name} after motion");
        assert_eq!(input.finish(), source, "{name} after finish");
    }

    let mut input = MathInput::from_latex("x^2");
    assert_eq!(input.key(MathKey::Text('y')), Outcome::Edited);
    assert_eq!(input.latex(), r"x^{2}y");
    assert_eq!(input.key(MathKey::Undo), Outcome::Edited);
    assert_eq!(input.latex(), "x^2");
    assert_eq!(input.finish(), "x^2");
}

#[test]
fn imported_latex_edits_and_pending_commands_finish_canonically() {
    let mut edited = MathInput::from_latex("x^2");
    edited.key(MathKey::Text('y'));
    assert_eq!(edited.finish(), r"x^{2}y");

    let mut command = MathInput::from_latex("x^2");
    for c in r"\alpha".chars() {
        command.key(MathKey::Text(c));
    }
    assert_eq!(command.finish(), r"x^{2}\alpha");
}

#[test]
fn imported_latex_preserves_comments_when_edited() {
    let mut input = MathInput::from_latex("x% comment\n+y");
    assert_eq!(input.latex(), "x% comment\n+y");
    input.key(MathKey::Text('z'));
    assert_eq!(input.latex(), "x% comment\n+yz");

    let mut terminal = MathInput::from_latex("x% naïve");
    assert_eq!(terminal.latex(), "x% naïve");
    terminal.key(MathKey::Text('y'));
    assert_eq!(terminal.latex(), "x% naïve\ny");
}
