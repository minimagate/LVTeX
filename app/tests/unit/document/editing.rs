use super::*;

#[test]
fn typed_backslash_is_literal_and_undoable() {
    let mut d = Document::new("");
    d.replace(r"a \b c\ d", true);
    assert!(d.formulas.is_empty());
    assert!(d.open_range().is_none());
    assert_eq!(d.text(), r"a \b c\ d");
    let reopened = Document::new(&d.markdown());
    assert!(reopened.formulas.is_empty());
    assert_eq!(reopened.text(), d.text());
    assert_eq!(reopened.markdown(), d.markdown());
    d.undo(false);
    assert_eq!(d.text(), "");
    assert!(d.open_range().is_none());
}
#[test]
fn edits_that_expose_a_backslash_keep_it_literal() {
    // A space typed before the backslash makes it an opener, so it must be marked.
    let mut d = Document::new("");
    type_text(&mut d, r"a\b");
    d.move_to(1, false);
    d.replace(" ", true);
    assert_eq!(d.text(), r"a \b");
    assert!(d.markdown().contains(LITERAL_BACKSLASH));
    let reopened = Document::new(&d.markdown());
    assert!(reopened.formulas.is_empty());
    assert_eq!(reopened.text(), r"a \b");
    d.move_to(d.rope.len_bytes(), false);
    type_text(&mut d, r"\ c\");
    assert!(d.formulas.is_empty());
    assert_eq!(d.text(), r"a \b\ c\");
    // Deleting the letter before a backslash leaves it at the start, where it would open.
    let mut d = Document::new("");
    type_text(&mut d, r"x\b");
    d.anchor = 0;
    d.caret = 1;
    d.replace("", false);
    assert_eq!(d.text(), r"\b");
    let reopened = Document::new(&d.markdown());
    assert!(reopened.formulas.is_empty());
    assert_eq!(reopened.text(), r"\b");
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
