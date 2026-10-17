use super::*;

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
fn cancelling_ai_prompt_keeps_plain_text_and_undo_restores_edit() {
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
fn cancelling_backslash_prompt_leaves_plain_prose() {
    let mut doc = Document::new("word");
    doc.move_to(2, false);
    doc.open_ai_prompt();
    type_text(&mut doc, r"a \x\ b");
    assert!(doc.cancel());
    assert_eq!(doc.text(), r"wo a \x\ b rd");
    assert!(doc.open_range().is_none());
    let saved = doc.markdown();
    assert_eq!(saved, r"wo a \x\ b rd");
    let reopened = Document::new(&saved);
    assert!(reopened.formulas.is_empty());
    assert_eq!(reopened.text(), doc.text());
    assert_eq!(reopened.markdown(), saved);
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
