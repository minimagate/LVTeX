use super::*;

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
fn reopening_pending_prompt_edits_its_text_and_saves_the_comment() {
    let source = format!("before {} after", pending_comment(r"sum_{i=1}^n i"));
    let mut doc = Document::new(&source);
    assert_eq!(doc.formulas[&7].label(), r"sum_{i=1}^n i");
    assert!(doc.reopen(7));
    assert_eq!(doc.text(), "before sum_{i=1}^n i after");
    assert_eq!(doc.markdown(), source);
    assert!(doc.submit_prompt());
    assert_eq!(doc.formulas[&7].prompt.as_deref(), Some("sum_{i=1}^n i"));
    assert_eq!(doc.markdown(), source);
}
