use super::*;

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
    let layout = crate::view::layout::Typesetter::new().document(&d, 680.0);
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
    let mut d = Document::new("a <!--mathstral:pending:b25l--> b");
    let undo = d.undo.len();
    assert_eq!(d.reopen_manual(2), None);
    assert_eq!(d.reopen_manual(0), None);
    assert_eq!(d.undo.len(), undo);
    assert!(d.manual_id().is_none());
    assert_eq!(d.formulas[&2].prompt.as_deref(), Some("one"));
}
