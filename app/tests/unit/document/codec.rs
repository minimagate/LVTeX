use super::*;

#[test]
fn backslash_rules() {
    let mut d = Document::new(r"and\or C:\a\b \1\2 plus a\b\ done");
    assert!(d.formulas.is_empty());
    assert_eq!(d.text(), r"and\or C:\a\b \1\2 plus a\b\ done");
    // Typed backslashes are ordinary prose: they open and close nothing.
    d.move_to(d.rope.len_bytes(), false);
    type_text(&mut d, r" \unfinished\");
    assert!(d.formulas.is_empty());
    assert!(d.open_range().is_none());
    assert!(d.text().ends_with(r" \unfinished\"));
    assert_eq!(Document::new(&d.markdown()).text(), d.text());
    let mut d = Document::new("");
    type_text(&mut d, r"\1\2");
    assert!(d.formulas.is_empty());
    assert_eq!(d.text(), r"\1\2");
    assert_eq!(Document::new(&d.markdown()).text(), r"\1\2");
}
#[test]
fn typed_backslashes_that_cannot_open_save_without_marker() {
    let mut d = Document::new("");
    type_text(&mut d, r"C:\path");
    assert_eq!(d.markdown(), r"C:\path");
    let reopened = Document::new(&d.markdown());
    assert!(reopened.formulas.is_empty());
    assert_eq!(reopened.text(), r"C:\path");
}
#[test]
fn old_literal_marker_is_ordinary_prose() {
    let source = r"<!--mathstral:literal-->\x\";
    let d = Document::new(source);
    assert!(d.formulas.is_empty());
    assert_eq!(d.text(), source);
    assert_eq!(d.markdown(), source);
}
#[test]
fn portable_math_is_never_in_visible_buffer() {
    let source = "Before $\\frac{1}{3}$<!--mathstral:MSBvdmVyIDM--> after\n$$\n\\int_0^1 x dx\n$$";
    let d = Document::new(source);
    assert_eq!(d.formulas.len(), 2);
    assert!(!d.text().contains("frac"));
    assert_eq!(d.formulas[&7].prompt.as_deref(), Some("1 over 3"));
    assert_eq!(d.markdown(), source);
}
#[test]
fn manual_latex_round_trips_through_save_and_copy() {
    for (latex, expected) in [
        (r"a\\", r"a\\".to_owned()),
        (r"x\$y", r"x\$y".to_owned()),
        (r"a\\\$b", r"a\\\$b".to_owned()),
        (r"\alpha + λ", r"\alpha + λ".to_owned()),
        ("x\n+y", "x\n+y".to_owned()),
        (" x \n+y ", " x \n+y ".to_owned()),
        ("x% note $\n+y", "x% note $\n+y".to_owned()),
        ("x% terminal $", "x% terminal $\n".to_owned()),
    ] {
        let mut doc = Document::new("before after");
        doc.move_to(7, false);
        doc.open_manual();
        doc.update_manual(latex.to_owned());
        doc.finish_manual();

        let saved = doc.markdown();
        let mut reopened = Document::new(&saved);
        let formula_pos = *reopened.formulas.keys().next().unwrap();
        assert_eq!(
            reopened.formulas[&formula_pos].latex.as_deref(),
            Some(expected.as_str()),
            "{latex:?}"
        );
        assert_eq!(reopened.markdown(), saved, "{latex:?}");
        let reopened_again = Document::new(&reopened.markdown());
        assert_eq!(
            reopened_again.formulas[&7].latex.as_deref(),
            Some(expected.as_str()),
            "{latex:?}"
        );

        reopened.move_to(formula_pos, false);
        reopened.move_to(formula_pos + ATOM.len(), true);
        let copied = reopened.selected();
        let mut pasted = Document::new("");
        pasted.paste(&copied);
        let pasted_formula = pasted.formulas.values().next().unwrap();
        assert_eq!(
            pasted_formula.latex.as_deref(),
            Some(expected.as_str()),
            "copy {latex:?}"
        );
    }
}
#[test]
fn display_latex_preserves_source_whitespace_and_compact_legacy_form() {
    for latex in [
        " x \n\n",
        r"a\\",
        r"a\",
        r"x\$y",
        r"a\\\$b",
        r"\alpha + λ",
        "x% note $\n+y",
        "x% terminal $",
    ] {
        let mut doc = Document::new("");
        doc.open_manual();
        doc.update_manual(latex.to_owned());
        doc.finish_manual();
        let saved = doc.markdown();
        let mut reopened = Document::new(&saved);
        assert_eq!(
            reopened.formulas.values().next().unwrap().latex.as_deref(),
            Some(latex),
            "{latex:?}"
        );
        assert_eq!(reopened.markdown(), saved, "{latex:?}");
        reopened.move_to(0, false);
        reopened.move_to(ATOM.len(), true);
        let copied = reopened.selected();
        let mut pasted = Document::new("");
        pasted.paste(&copied);
        assert_eq!(
            pasted.formulas.values().next().unwrap().latex.as_deref(),
            Some(latex),
            "copy {latex:?}"
        );
    }

    let compact = Document::new("$$x$$");
    assert_eq!(compact.formulas[&0].latex.as_deref(), Some("x"));
}
#[test]
fn dollars_in_prose_survive_save_and_reopen() {
    let mut d = Document::new("");
    type_text(&mut d, "I paid $5 and $6");
    assert_eq!(d.markdown(), "I paid \\$5 and \\$6");
    let reopened = Document::new(&d.markdown());
    assert!(reopened.formulas.is_empty());
    assert_eq!(reopened.text(), "I paid $5 and $6");
    assert!(!reopened.dirty());
    let mut d = Document::new("");
    d.replace("a\\", true);
    d.replace("$", true);
    assert_eq!(d.markdown(), "a\\\\\\$");
    assert_eq!(Document::new(&d.markdown()).text(), "a\\$");
    let d = Document::new(r"x \cost \$5\ y");
    assert!(d.formulas.is_empty());
    assert_eq!(d.text(), r"x \cost $5\ y");
    assert_eq!(d.markdown(), r"x \cost \$5\ y");
    assert!(Document::new(&d.markdown()).formulas.is_empty());
    let mut d = Document::new("$5");
    d.move_to(0, false);
    d.move_to(d.rope.len_bytes(), true);
    assert_eq!(d.selected(), "$5");
}
#[test]
fn backslashes_round_trip() {
    for prose in ["C:\\path", "a\\\\b", "\\$5", "$5 and $6", "end\\"] {
        let mut d = Document::new("");
        d.replace(prose, false);
        assert_eq!(d.text(), prose);
        assert_round_trip(&d);
    }
    let mut d = Document::new("");
    d.replace("C:\\path", false);
    assert_eq!(d.markdown(), "C:\\path");
    let mut d = Document::new("");
    d.replace("a\\", false);
    d.replace("$", false);
    d.replace("x", false);
    assert_round_trip(&d);
    let d = Document::new("a \\\\$x$ b");
    assert_eq!(d.text(), format!("a \\{ATOM} b"));
    assert_eq!(d.markdown(), "a \\\\$x$ b");
    assert_round_trip(&d);
    let d = Document::new(r"x \cost \$5\ y");
    assert_eq!(d.markdown(), r"x \cost \$5\ y");
    assert_round_trip(&d);
    assert!(Document::new(&d.markdown()).formulas.is_empty());
}
#[test]
fn backslash_prompts_round_trip() {
    let mut d = Document::new("");
    type_text(&mut d, r"\cost $5\ and a\b end");
    assert!(d.formulas.is_empty());
    assert_eq!(d.text(), r"\cost $5\ and a\b end");
    assert_round_trip(&d);
    let mut d = Document::new("");
    type_text(&mut d, r"a\\ \x\");
    assert!(d.formulas.is_empty());
    assert_eq!(d.text(), r"a\\ \x\");
    assert_round_trip(&d);
    let mut d = Document::new("");
    type_text(&mut d, r"\$5 and x");
    assert!(d.open_range().is_none());
    assert_eq!(d.markdown(), r"\\\$5 and x");
    assert_round_trip(&d);
    assert!(Document::new(&d.markdown()).formulas.is_empty());
    let d = Document::new("/x/ and /y");
    assert!(d.formulas.is_empty());
    assert_eq!(d.text(), "/x/ and /y");
    let mut d = Document::new("");
    type_text(&mut d, "/x/");
    assert!(d.formulas.is_empty());
    assert_round_trip(&d);
}
#[test]
fn prompt_backslashes_survive_save_edit_and_copy_round_trips() {
    for prompt in [
        "a \\ b",
        "a \\\\ b",
        r"\alpha",
        "a \\ $5",
        "solve $x=1$",
        "trailing\\",
    ] {
        let mut editing = Document::new("");
        editing.open_ai_prompt();
        type_text(&mut editing, prompt);
        assert_eq!(editing.text(), prompt);

        let editing_saved = editing.markdown();
        assert_eq!(editing_saved, pending_comment(prompt), "{prompt:?}");
        let editing_reopened = Document::new(&editing_saved);
        assert_eq!(
            editing_reopened
                .formulas
                .values()
                .next()
                .unwrap()
                .prompt
                .as_deref(),
            Some(prompt),
            "editing prompt {prompt:?}"
        );

        assert!(editing.submit_prompt());
        let saved = editing.markdown();
        assert_eq!(saved, pending_comment(prompt), "{prompt:?}");
        let reopened = Document::new(&saved);
        assert_eq!(
            reopened.formulas.values().next().unwrap().prompt.as_deref(),
            Some(prompt),
            "submitted prompt {prompt:?}"
        );

        editing.move_to(0, false);
        editing.move_to(ATOM.len(), true);
        let copied = editing.selected();
        assert_eq!(copied, pending_comment(prompt), "{prompt:?}");
        let mut pasted = Document::new("");
        pasted.paste(&copied);
        assert_eq!(
            pasted.formulas.values().next().unwrap().prompt.as_deref(),
            Some(prompt),
            "copied prompt {prompt:?}"
        );
    }
}
#[test]
fn pending_prompt_round_trips_as_comment() {
    let prompt = r"cost $5 \ naïve ∫";
    let source = format!("a {} b", pending_comment(prompt));
    let mut d = Document::new(&source);
    assert_eq!(d.text(), format!("a {ATOM} b"));
    assert_eq!(d.formulas[&2].prompt.as_deref(), Some(prompt));
    assert!(d.formulas[&2].latex.is_none());
    assert_eq!(d.markdown(), source);
    assert_round_trip(&d);

    // Copying a pending formula keeps it pending.
    d.move_to(2, false);
    d.move_to(2 + ATOM.len(), true);
    let copied = d.selected();
    assert_eq!(copied, pending_comment(prompt));
    let mut pasted = Document::new("");
    pasted.paste(&copied);
    assert_eq!(pasted.formulas[&0].prompt.as_deref(), Some(prompt));
    assert!(pasted.formulas[&0].latex.is_none());
}
#[test]
fn ai_prompt_being_edited_saves_as_pending_comment() {
    let mut d = Document::new("ab");
    d.move_to(1, false);
    d.open_ai_prompt();
    type_text(&mut d, r"x \ $y");
    assert_eq!(d.markdown(), format!("a {} b", pending_comment(r"x \ $y")));
    let reopened = Document::new(&d.markdown());
    assert_eq!(reopened.formulas[&2].prompt.as_deref(), Some(r"x \ $y"));
}
#[test]
fn backslash_prose_is_plain_text() {
    for source in [r"a \x\ b", r"C:\Users\me\ end"] {
        let d = Document::new(source);
        assert!(d.formulas.is_empty(), "{source:?}");
        assert_eq!(d.text(), source);
        assert_eq!(d.markdown(), source);
    }
}
#[test]
fn pending_comment_format_is_pinned() {
    let d = Document::new("<!--mathstral:pending:c3Vt-->");
    assert_eq!(d.formulas[&0].prompt.as_deref(), Some("sum"));
    assert_eq!(d.markdown(), "<!--mathstral:pending:c3Vt-->");
}
#[test]
fn malformed_pending_comment_stays_plain_text() {
    for source in [
        "a <!--mathstral:pending:!!!--> b",
        "a <!--mathstral:pending:--> b",
        "a <!--mathstral:pending:/w--> b",
        "a <!--mathstral:pending:YQ b",
    ] {
        let d = Document::new(source);
        assert!(d.formulas.is_empty(), "{source:?}");
        assert_eq!(d.text(), source, "{source:?}");
        assert_eq!(d.markdown(), source, "{source:?}");
    }
}
