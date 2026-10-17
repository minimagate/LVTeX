use super::codec::LITERAL_BACKSLASH;
use super::*;
fn type_text(d: &mut Document, s: &str) {
    for c in s.chars() {
        d.replace(&c.to_string(), true);
    }
}
#[test]
fn backslash_rules() {
    let mut d = Document::new(r"and\or C:\a\b \1\2 plus a\b\ done");
    assert_eq!(d.formulas.len(), 1);
    assert_eq!(
        d.formulas.values().next().unwrap().prompt.as_deref(),
        Some(r"1\2 plus a\b")
    );
    assert_eq!(d.text(), format!(r"and\or C:\a\b {ATOM} done"));
    d.move_to(d.rope.len_bytes(), false);
    type_text(&mut d, r" \unfinished");
    assert!(d.open_range().is_some());
    assert!(d.cancel());
    type_text(&mut d, r"\");
    assert_eq!(d.formulas.len(), 1);
    assert_eq!(Document::new(&d.markdown()).formulas.len(), 1);
    let mut d = Document::new("");
    type_text(&mut d, r"\1\2");
    assert_eq!(d.formulas.len(), 1);
    assert_eq!(d.text(), format!("{ATOM}2"));
}
#[test]
fn atoms_survive_edits_and_history_and_roundtrip() {
    let mut d = Document::new(r"A \one\ and \two\.");
    let id = d.formulas[&2].id;
    d.move_to(0, false);
    d.replace("prefix ", true);
    assert_eq!(d.formulas[&9].id, id);
    d.move_to(12, false);
    assert!(d.reopen(9));
    d.replace("!", true);
    assert!(d.submit_prompt());
    assert_eq!(d.formulas[&9].prompt.as_deref(), Some("one!"));
    d.undo(false);
    assert!(d.text().contains("one!"));
    d.undo(true);
    assert_eq!(d.formulas[&9].prompt.as_deref(), Some("one!"));
    let saved = d.markdown();
    assert_eq!(Document::new(&saved).markdown(), saved);
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
fn graphemes_words_and_history() {
    let mut d = Document::new("a e\u{301} 👩‍🔬 end");
    d.move_to(2, false);
    assert_eq!(d.adjacent(true, false), 5);
    d.move_to(6, false);
    assert_eq!(d.adjacent(true, false), 17);
    d.move_to(0, false);
    assert_eq!(d.adjacent(true, true), 1);
    d.move_to(d.rope.len_bytes(), false);
    d.replace("!", true);
    d.replace("?", true);
    assert!(d.dirty());
    d.undo(false);
    assert!(!d.dirty());
    d.undo(true);
    assert!(d.text().ends_with("!?"));
}
#[test]
fn replacement_reverses_selection_and_restores_it() {
    let mut d = Document::new("one two");
    d.move_to(7, false);
    d.move_to(4, true);
    d.replace("three", false);
    assert_eq!(d.text(), "one three");
    d.undo(false);
    assert_eq!(d.selected(), "two");
}
#[test]
fn stray_atom_chars_are_dropped() {
    let d = Document::new("a\u{fffc}b $x$");
    assert_eq!(d.text(), format!("ab {ATOM}"));
    assert_eq!(d.formulas.len(), 1);
    assert_eq!(d.formulas[&3].latex.as_deref(), Some("x"));
    let mut d = Document::new("ab");
    d.move_to(2, false);
    d.replace("\u{fffc}", true);
    assert_eq!(d.text(), "ab");
    assert!(!d.dirty());
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
    assert_eq!(
        d.formulas.values().next().unwrap().prompt.as_deref(),
        Some("cost $5")
    );
    assert_eq!(d.markdown(), r"x \cost \$5\ y");
    let reopened = Document::new(&d.markdown());
    assert_eq!(
        reopened.formulas.values().next().unwrap().prompt.as_deref(),
        Some("cost $5")
    );
    let mut d = Document::new("$5");
    d.move_to(0, false);
    d.move_to(d.rope.len_bytes(), true);
    assert_eq!(d.selected(), "$5");
}
fn assert_round_trip(d: &Document) {
    let reopened = Document::new(&d.markdown());
    assert_eq!(reopened.text(), d.text());
    assert_eq!(reopened.formulas.len(), d.formulas.len());
    for (a, b) in d.formulas.values().zip(reopened.formulas.values()) {
        assert_eq!(a.prompt, b.prompt);
        assert_eq!(a.latex, b.latex);
    }
    assert!(!reopened.dirty());
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
    assert_eq!(
        Document::new(&d.markdown())
            .formulas
            .values()
            .next()
            .unwrap()
            .prompt
            .as_deref(),
        Some("cost $5")
    );
}
#[test]
fn backslash_prompts_round_trip() {
    let mut d = Document::new("");
    type_text(&mut d, r"\cost $5\ and a\b end");
    assert_eq!(
        d.formulas.values().next().unwrap().prompt.as_deref(),
        Some("cost $5")
    );
    assert_round_trip(&d);
    let mut d = Document::new("");
    type_text(&mut d, r"a\\ \x\");
    assert_eq!(d.text(), format!(r"a\\ {ATOM}"));
    assert_round_trip(&d);
    let mut d = Document::new("");
    type_text(&mut d, r"\$5 and x");
    assert!(d.cancel());
    assert_eq!(d.markdown(), r"<!--mathstral:literal-->\\\$5 and x");
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
        assert_eq!(
            editing_saved.contains(LITERAL_BACKSLASH),
            prompt.contains('\\'),
            "{prompt:?}: {editing_saved:?}"
        );
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
        assert_eq!(saved.contains(LITERAL_BACKSLASH), prompt.contains('\\'));
        let reopened = Document::new(&saved);
        assert_eq!(
            reopened.formulas.values().next().unwrap().prompt.as_deref(),
            Some(prompt),
            "submitted prompt {prompt:?}"
        );

        editing.move_to(0, false);
        editing.move_to(ATOM.len(), true);
        let copied = editing.selected();
        assert_eq!(
            copied.contains(LITERAL_BACKSLASH),
            prompt.contains('\\') || prompt.contains('$'),
            "{prompt:?}"
        );
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
fn literal_marker_text_survives_plain_ai_and_clipboard_round_trips() {
    let marker = LITERAL_BACKSLASH;
    for text in [
        marker.to_owned(),
        format!("{marker}{marker}"),
        format!("{marker}\\"),
        format!("{marker}$"),
    ] {
        let mut plain = Document::new("");
        type_text(&mut plain, &text);
        let saved = plain.markdown();
        assert_eq!(
            saved.matches(marker).count(),
            text.matches(marker).count() * 2
        );
        let reopened = Document::new(&saved);
        assert_eq!(reopened.text(), text, "plain {text:?}");
        assert_eq!(
            Document::new(&reopened.markdown()).text(),
            text,
            "save {text:?}"
        );

        plain.move_to(0, false);
        plain.move_to(plain.rope.len_bytes(), true);
        let copied = plain.selected();
        let mut pasted = Document::new("");
        pasted.paste(&copied);
        assert_eq!(pasted.text(), text, "clipboard {text:?}");
    }

    let prompt = format!("before {marker}{marker} {marker}\\ {marker}$ after");
    let mut active = Document::new("");
    active.open_ai_prompt();
    type_text(&mut active, &prompt);
    let saved = active.markdown();
    let reopened_active = Document::new(&saved);
    assert_eq!(
        reopened_active
            .formulas
            .values()
            .next()
            .unwrap()
            .prompt
            .as_deref(),
        Some(prompt.as_str())
    );
    assert_eq!(reopened_active.markdown(), saved);
    assert!(active.submit_prompt());
    let submitted = active.markdown();
    let reopened = Document::new(&submitted);
    assert_eq!(
        reopened.formulas.values().next().unwrap().prompt.as_deref(),
        Some(prompt.as_str())
    );
    assert_eq!(Document::new(&reopened.markdown()).markdown(), submitted);

    active.move_to(0, false);
    active.move_to(ATOM.len(), true);
    let copied = active.selected();
    let mut pasted = Document::new("");
    pasted.paste(&copied);
    assert_eq!(
        pasted.formulas.values().next().unwrap().prompt.as_deref(),
        Some(prompt.as_str())
    );

    let id = active.formulas.values().next().unwrap().id;
    assert!(active.complete(id, Err("generation failed".into())));
    let failed = active.markdown();
    assert_eq!(
        Document::new(&failed)
            .formulas
            .values()
            .next()
            .unwrap()
            .prompt
            .as_deref(),
        Some(prompt.as_str())
    );
    assert_eq!(Document::new(&failed).markdown(), failed);
}
#[test]
fn active_ai_clipboard_unescapes_literal_markers_without_parsing_text() {
    let marker = LITERAL_BACKSLASH;
    let prompt = format!("before {marker} \\alpha $ after");
    let mut active = Document::new("");
    active.open_ai_prompt();
    type_text(&mut active, &prompt);
    let marker_start = prompt.find(marker).unwrap();
    active.move_to(marker_start, false);
    active.move_to(marker_start + marker.len(), true);
    let copied = active.selected();
    assert_eq!(copied, format!("{marker}{marker}"));
    active.paste(&copied);
    assert_eq!(active.text(), prompt);
    assert!(active.formulas.is_empty());

    let mut another = Document::new("");
    another.open_ai_prompt();
    type_text(&mut another, "prefix suffix");
    another.move_to(7, false);
    another.paste(&copied);
    assert_eq!(another.text(), format!("prefix {marker}suffix"));
    assert!(another.formulas.is_empty());
    another.move_to(another.rope.len_bytes(), false);
    another.paste(marker);
    assert!(another.text().ends_with(marker));
    assert!(another.formulas.is_empty());
}
#[test]
fn paste_takes_text_literally() {
    let mut d = Document::new("");
    d.paste("\\$5 and $x$");
    assert_eq!(d.text(), format!("\\$5 and {ATOM}"));
    assert_eq!(d.formulas.len(), 1);
}
#[test]
fn replacing_with_only_atom_chars_is_a_no_op() {
    let mut d = Document::new("abc");
    d.move_to(0, false);
    d.move_to(3, true);
    d.replace("\u{fffc}", true);
    assert_eq!(d.text(), "abc");
    assert_eq!(d.selected(), "abc");
}
fn image(latex: &str) -> Arc<FormulaImage> {
    let pixels = || crate::math::Image {
        rgba: vec![],
        width: 0,
        height: 0,
        baseline: 0.0,
        caret: None,
    };
    Arc::new(FormulaImage {
        latex: latex.into(),
        inline: pixels(),
        display: pixels(),
    })
}
#[test]
fn manual_formula_saves_as_plain_math_and_renders() {
    let mut d = Document::new("");
    type_text(&mut d, "a ");
    d.open_manual();
    d.set_manual("x^{2}".into(), None);
    d.finish_manual();
    assert_eq!(d.markdown(), "a $x^{2}$");
    let jobs = d.jobs();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].latex.as_deref(), Some("x^{2}"));
    assert!(jobs[0].prompt.is_none());
    let reopened = Document::new(&d.markdown());
    assert_eq!(reopened.markdown(), "a $x^{2}$");
    assert_eq!(
        reopened.formulas.values().next().unwrap().latex.as_deref(),
        Some("x^{2}")
    );
}
#[test]
fn jobs_skip_the_formula_being_edited() {
    let mut d = Document::new("");
    type_text(&mut d, "a ");
    d.open_manual();
    d.set_manual("x".into(), None);
    assert!(d.jobs().is_empty());
    d.finish_manual();
    assert_eq!(d.jobs().len(), 1);
}
#[test]
fn empty_manual_formula_has_an_empty_source_label() {
    let mut d = Document::new("");
    d.open_manual();
    assert_eq!(d.formulas[&0].label(), "");
    let layout = crate::layout::Typesetter::new().document(&d, 680.0);
    assert!(!layout.lines.is_empty());
    assert!(layout.caret_on_line(d.caret, None).1.is_finite());
}
#[test]
fn finishing_manual_edit_keeps_preview_until_final_render() {
    let mut d = Document::new("");
    d.open_manual();
    let (id, revision) = d.update_manual("x^2".into()).unwrap();
    let preview = image("x^2 with caret");
    assert!(d.complete_manual_preview(id, revision, Ok(preview.clone())));

    d.finish_manual();

    let formula = &d.formulas[&0];
    assert!(Arc::ptr_eq(formula.image.as_ref().unwrap(), &preview));
    assert_eq!(formula.label(), "x^2");
    let jobs = d.jobs();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].id, id);
    assert_eq!(jobs[0].latex.as_deref(), Some("x^2"));

    let final_image = image("x^2");
    assert!(d.complete(id, Ok(final_image.clone())));
    assert!(Arc::ptr_eq(
        d.formulas[&0].image.as_ref().unwrap(),
        &final_image
    ));
    assert!(d.jobs().is_empty());
}
#[test]
fn failed_final_manual_render_clears_preview_and_shows_source() {
    let mut d = Document::new("");
    d.open_manual();
    let (id, revision) = d.update_manual("x^2".into()).unwrap();
    assert!(d.complete_manual_preview(id, revision, Ok(image("x^2 with caret"))));
    d.finish_manual();

    assert!(d.complete(id, Err("cannot render final source".into())));

    let formula = &d.formulas[&0];
    assert!(formula.image.is_none());
    assert!(formula.failed);
    assert_eq!(formula.label(), "x^2");
    assert!(d.jobs().is_empty());
}
#[test]
fn manual_preview_keeps_source_rejects_stale_results_and_shows_failures() {
    let mut d = Document::new("");
    d.open_manual();
    let (id, first) = d.update_manual("abcd".into()).unwrap();
    assert!(d.complete_manual_preview(id, first, Ok(image("caret markup"))));
    assert_eq!(d.formulas[&0].latex.as_deref(), Some("abcd"));
    let (id, second) = d.update_manual("abcd+e".into()).unwrap();
    assert_eq!(d.formulas[&0].image.as_ref().unwrap().latex, "caret markup");
    assert!(!d.complete_manual_preview(id, first, Err("stale".into())));
    assert!(d.complete_manual_preview(id, second, Err("cannot render".into())));
    let formula = &d.formulas[&0];
    assert_eq!(formula.latex.as_deref(), Some("abcd+e"));
    assert!(formula.image.is_none());
    assert!(formula.failed);
    assert_eq!(formula.label(), "abcd+e");
}
#[test]
fn manual_preview_results_are_ignored_after_finish_or_cancel() {
    let mut d = Document::new("");
    d.open_manual();
    let (id, revision) = d.update_manual("x".into()).unwrap();
    d.finish_manual();
    assert!(!d.complete_manual_preview(id, revision, Ok(image("late"))));

    d.open_manual();
    let (id, revision) = d.update_manual("y".into()).unwrap();
    d.cancel_manual();
    assert!(!d.complete_manual_preview(id, revision, Ok(image("late"))));
}
#[test]
fn finishing_an_empty_manual_formula_removes_it() {
    let mut d = Document::new("");
    type_text(&mut d, "a b");
    d.move_to(2, false);
    d.open_manual();
    d.finish_manual();
    assert_eq!(d.text(), "a b");
    assert!(d.formulas.is_empty());
    assert_eq!(d.caret, 2);
    assert_eq!(d.markdown(), "a b");
    assert!(d.manual_id().is_none());
}
#[test]
fn cancel_manual_restores_document_and_history() {
    let mut d = Document::new("a b");
    d.move_to(2, false);
    d.replace("x", false);
    d.undo(false);
    let before = (d.text(), d.caret, d.undo.len(), d.redo.len());
    d.open_manual();
    d.set_manual("x".into(), None);
    d.cancel_manual();
    assert_eq!((d.text(), d.caret, d.undo.len(), d.redo.len()), before);
    assert!(d.formulas.is_empty());
    assert!(d.manual_id().is_none());

    let mut d = Document::new("a $x$ b");
    let old = d.formulas[&2].id;
    assert!(d.reopen_manual(2).is_some());
    d.set_manual("y".into(), None);
    d.cancel_manual();
    assert_eq!(d.markdown(), "a $x$ b");
    assert_eq!(d.formulas[&2].id, old);
    assert_eq!(d.formulas[&2].latex.as_deref(), Some("x"));
}
#[test]
fn manual_session_is_one_undo_step() {
    let mut d = Document::new("");
    type_text(&mut d, "a b");
    d.move_to(2, false);
    let before = d.text();
    let depth = d.undo.len();
    d.open_manual();
    d.set_manual("x".into(), None);
    d.set_manual("x^{2}".into(), None);
    d.finish_manual();
    let after = d.markdown();
    assert_eq!(after, "a $x^{2}$b");
    d.undo(false);
    assert_eq!(d.text(), before);
    assert_eq!(d.caret, 2);
    assert!(d.formulas.is_empty());
    assert_eq!(d.undo.len(), depth);
    d.undo(true);
    assert_eq!(d.markdown(), after);
    assert_eq!(d.formulas.len(), 1);
}
#[test]
fn reopened_formula_keeps_its_source_unless_changed() {
    let mut d = Document::new("a $\\frac{a}{b}$ b");
    d.move_to(d.rope.len_bytes(), false);
    d.replace("!", false);
    d.undo(false);
    let old = d.formulas[&2].id;
    d.complete(old, Ok(image("\\frac{a}{b}")));
    let history = (d.undo.len(), d.redo.len());
    assert_eq!(history.1, 1);
    assert_eq!(d.reopen_manual(2).as_deref(), Some("\\frac{a}{b}"));
    assert_eq!(d.caret, 2 + ATOM.len());
    assert_ne!(d.formulas[&2].id, old);
    // A late render for the old id must not touch the edited formula.
    assert!(!d.complete(old, Ok(image("\\frac{1}{1}"))));
    assert_eq!(d.formulas[&2].latex.as_deref(), Some("\\frac{a}{b}"));
    d.finish_manual();
    assert_eq!(d.formulas[&2].id, old);
    assert!(d.formulas[&2].image.is_some());
    assert_eq!(d.markdown(), "a $\\frac{a}{b}$ b");
    assert!(d.jobs().is_empty());
    assert_eq!((d.undo.len(), d.redo.len()), history);

    assert!(d.reopen_manual(2).is_some());
    d.set_manual("\\frac{1}{b}".into(), None);
    d.finish_manual();
    assert_eq!(d.markdown(), "a $\\frac{1}{b}$ b");
    let jobs = d.jobs();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].latex.as_deref(), Some("\\frac{1}{b}"));
    assert_eq!(jobs[0].id, d.formulas[&2].id);
}
#[test]
fn reopen_manual_refuses_prompts_and_empty_positions() {
    let mut d = Document::new(r"a \one\ b");
    let undo = d.undo.len();
    assert_eq!(d.reopen_manual(2), None);
    assert_eq!(d.reopen_manual(0), None);
    assert_eq!(d.undo.len(), undo);
    assert!(d.manual_id().is_none());
    assert_eq!(d.formulas[&2].prompt.as_deref(), Some("one"));
}
#[test]
fn ai_prompt_submits_from_document_edges_and_selection() {
    let cases = [
        ("", 0, 0, "integral", 0),
        ("left right", 4, 4, "sum", 5),
        ("word", 2, 2, "fraction", 3),
        ("tail", 4, 4, "root", 5),
        ("please integrate now", 0, 16, "please integrate", 0),
    ];

    for (source, start, end, prompt, formula_pos) in cases {
        let mut doc = Document::new(source);
        doc.anchor = start;
        doc.caret = end;
        doc.open_ai_prompt();
        if start == end {
            type_text(&mut doc, prompt);
        }
        assert!(doc.submit_prompt(), "source {source:?}");
        assert_eq!(doc.formulas[&formula_pos].prompt.as_deref(), Some(prompt));
        let saved = doc.markdown();
        let reopened = Document::new(&saved);
        assert_eq!(reopened.markdown(), saved, "source {source:?}");
        assert_eq!(
            reopened.formulas[&formula_pos].prompt.as_deref(),
            Some(prompt),
            "source {source:?}"
        );
        doc.undo(false);
        assert!(doc.formulas.is_empty(), "undo submission for {source:?}");
        assert!(doc.open_range().is_some(), "undo submission for {source:?}");
        doc.undo(true);
        assert_eq!(doc.markdown(), saved, "redo submission for {source:?}");
    }
}
#[test]
fn empty_and_unfocused_ai_prompts_do_not_submit() {
    let mut empty = Document::new("word");
    empty.open_ai_prompt();
    assert_eq!(empty.text(), " word");
    assert_eq!(empty.open_range(), Some(0..0));
    assert_eq!(empty.markdown(), "word");
    assert_eq!(Document::new(&empty.markdown()).text(), "word");
    assert!(empty.cancel());
    assert_eq!(empty.text(), "word");
    assert_eq!(empty.markdown(), "word");
    empty.undo(false);
    assert_eq!(empty.text(), " word");
    assert!(empty.submit_prompt());
    assert!(empty.formulas.is_empty());
    assert_eq!(empty.text(), "word");

    let mut elsewhere = Document::new("before after");
    elsewhere.move_to(0, false);
    elsewhere.open_ai_prompt();
    elsewhere.move_to(elsewhere.rope.len_bytes(), false);
    let open = elsewhere.text();
    assert!(!elsewhere.submit_prompt());
    assert_eq!(elsewhere.text(), open);
    assert!(elsewhere.formulas.is_empty());

    let mut redo = Document::new("a");
    redo.replace("b", false);
    redo.undo(false);
    assert!(!redo.redo.is_empty());
    redo.open_ai_prompt();
    assert!(redo.redo.is_empty());
}
#[test]
fn pasted_dollar_math_stays_literal_inside_ai_prompt() {
    let mut doc = Document::new("");
    doc.open_ai_prompt();
    doc.paste("solve $x=1$");
    assert_eq!(doc.text(), "solve $x=1$");
    assert!(doc.formulas.is_empty());
    assert!(doc.submit_prompt());
    assert_eq!(doc.formulas[&0].prompt.as_deref(), Some("solve $x=1$"));
}
#[test]
fn replacing_full_ai_prompt_selection_keeps_the_edit_range() {
    let mut doc = Document::new("");
    doc.open_ai_prompt();
    type_text(&mut doc, "draft");
    doc.move_to(0, false);
    doc.move_to(5, true);
    doc.replace("solve $x=1$", false);
    assert_eq!(doc.text(), "solve $x=1$");
    assert!(doc.formulas.is_empty());
    assert!(doc.submit_prompt());
    assert_eq!(doc.formulas[&0].prompt.as_deref(), Some("solve $x=1$"));
}
#[test]
fn empty_ai_prompt_submission_handles_deleted_boundary_spaces() {
    let mut prefix = Document::new("ab");
    prefix.move_to(1, false);
    prefix.open_ai_prompt();
    prefix.anchor = 0;
    prefix.caret = 2;
    prefix.replace("", false);
    assert_eq!(prefix.text(), " b");
    assert!(prefix.submit_prompt());
    assert_eq!(prefix.text(), "b");

    let mut suffix = Document::new("ab");
    suffix.move_to(1, false);
    suffix.open_ai_prompt();
    suffix.anchor = 2;
    suffix.caret = 4;
    suffix.replace("", false);
    assert_eq!(suffix.text(), "a ");
    assert!(suffix.submit_prompt());
    assert_eq!(suffix.text(), "a");
}
#[test]
fn cancelling_ai_prompt_keeps_literal_text_and_undo_restores_edit() {
    let mut doc = Document::new("word");
    doc.move_to(2, false);
    doc.open_ai_prompt();
    type_text(&mut doc, "fraction");
    assert!(doc.cancel());
    assert_eq!(doc.text(), "wo fraction rd");
    let saved = doc.markdown();
    let reopened = Document::new(&saved);
    assert_eq!(reopened.markdown(), saved);
    assert!(reopened.formulas.is_empty());
    doc.undo(false);
    assert!(doc.open_range().is_some());
    assert!(doc.submit_prompt());
    assert_eq!(
        doc.formulas.values().next().unwrap().prompt.as_deref(),
        Some("fraction")
    );
}
#[test]
fn reopening_prompt_edits_plain_text_and_saves_legacy_delimiters() {
    let mut doc = Document::new(r"before \sum_{i=1}^n i\ after");
    assert_eq!(doc.formulas[&7].label(), r"sum_{i=1}^n i");
    assert!(doc.reopen(7));
    assert_eq!(doc.text(), "before sum_{i=1}^n i after");
    assert_eq!(doc.markdown(), r"before \sum_{i=1}^n i\ after");
    assert!(doc.submit_prompt());
    assert_eq!(doc.formulas[&7].prompt.as_deref(), Some("sum_{i=1}^n i"));
}
