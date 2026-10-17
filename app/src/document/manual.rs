//! Hand-written math session: opening, previewing, finishing and cancelling a manual formula.
use super::{ATOM, Document, Formula, NEXT_ID};
use crate::math::worker::FormulaImage;
use std::sync::{Arc, atomic::Ordering};

#[derive(Clone)]
pub(super) struct Manual {
    id: u64,
    revision: u64,
    original: Option<Formula>,
}

impl Document {
    // The session's one undo step is recorded at open. Redo is left alone until finish,
    // so cancel can restore it untouched.
    pub fn open_manual(&mut self) -> u64 {
        self.undo.push(self.snapshot());
        self.break_group();
        let range = self.selection();
        let formula = Formula::new(None, Some(String::new()));
        let id = formula.id;
        self.splice(range.clone(), ATOM);
        self.formulas.insert(range.start, formula);
        self.caret = range.start + ATOM.len();
        self.anchor = self.caret;
        self.manual = Some(Manual {
            id,
            revision: 0,
            original: None,
        });
        id
    }
    pub fn reopen_manual(&mut self, pos: usize) -> Option<String> {
        let original = self
            .formulas
            .get(&pos)
            .filter(|f| f.prompt.is_none())?
            .clone();
        let latex = original.latex.clone()?;
        self.undo.push(self.snapshot());
        self.break_group();
        // A fresh id makes late results for the old source land nowhere.
        let mut edited = original.clone();
        edited.id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        self.manual = Some(Manual {
            id: edited.id,
            revision: 0,
            original: Some(original),
        });
        self.formulas.insert(pos, edited);
        self.caret = pos + ATOM.len();
        self.anchor = self.caret;
        Some(latex)
    }
    pub fn manual_id(&self) -> Option<u64> {
        self.manual.as_ref().map(|m| m.id)
    }
    pub fn update_manual(&mut self, latex: String) -> Option<(u64, u64)> {
        let manual = self.manual.as_mut()?;
        manual.revision = manual.revision.wrapping_add(1);
        let (id, revision) = (manual.id, manual.revision);
        let formula = self.formulas.values_mut().find(|f| f.id == id)?;
        formula.latex = Some(latex);
        formula.failed = false;
        Some((id, revision))
    }
    pub fn complete_manual_preview(
        &mut self,
        id: u64,
        revision: u64,
        result: Result<Arc<FormulaImage>, String>,
    ) -> bool {
        if !self
            .manual
            .as_ref()
            .is_some_and(|manual| manual.id == id && manual.revision == revision)
        {
            return false;
        }
        let Some(formula) = self.formulas.values_mut().find(|formula| formula.id == id) else {
            return false;
        };
        match result {
            Ok(image) => {
                formula.image = Some(image);
                formula.failed = false;
            }
            Err(_) => {
                formula.image = None;
                formula.failed = true;
            }
        }
        true
    }
    #[cfg(test)]
    pub fn set_manual(&mut self, latex: String, image: Option<Arc<FormulaImage>>) {
        let Some((id, _)) = self.update_manual(latex) else {
            return;
        };
        if let Some(formula) = self.formulas.values_mut().find(|formula| formula.id == id) {
            formula.image = image;
            formula.failed = false;
        }
    }
    pub fn finish_manual(&mut self) {
        let Some(edit) = self.manual.take() else {
            return;
        };
        let Some((&pos, formula)) = self.formulas.iter().find(|(_, f)| f.id == edit.id) else {
            return;
        };
        let formula = formula.clone();
        let latex = formula.latex.clone().unwrap_or_default();
        if latex.is_empty() {
            self.redo.clear();
            self.splice(pos..pos + ATOM.len(), "");
            self.caret = pos;
        } else if let Some(original) = edit
            .original
            .filter(|o| o.latex.as_deref() == Some(latex.as_str()))
        {
            // Unchanged: the snapshot from reopen would be a no-op undo step, so drop it and keep redo.
            self.undo.pop();
            self.formulas.insert(pos, original);
            self.caret = pos + ATOM.len();
        } else {
            // Changed source is re-rendered; the session's undo step is the one recorded at open.
            self.redo.clear();
            self.formulas.insert(pos, formula);
            self.caret = pos + ATOM.len();
        }
        self.anchor = self.caret;
    }
    pub fn cancel_manual(&mut self) {
        if self.manual.take().is_some()
            && let Some(previous) = self.undo.pop()
        {
            self.restore(previous);
        }
        self.break_group();
    }
}
