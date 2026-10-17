//! Undo and redo history of the document.
use super::{Document, Formula, prompt::Editing};
use ropey::Rope;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub(super) struct Snapshot {
    rope: Rope,
    caret: usize,
    anchor: usize,
    pub(super) formulas: BTreeMap<usize, Formula>,
    cancelled: BTreeSet<usize>,
    editing: Option<Editing>,
}

impl Document {
    pub fn dirty(&self) -> bool {
        self.markdown() != self.saved
    }
    pub fn mark_saved(&mut self) {
        self.saved = self.markdown();
        self.break_group();
    }
    pub fn break_group(&mut self) {
        self.typing = None;
    }
    pub(super) fn snapshot(&self) -> Snapshot {
        Snapshot {
            rope: self.rope.clone(),
            caret: self.caret,
            anchor: self.anchor,
            formulas: self.formulas.clone(),
            cancelled: self.cancelled.clone(),
            editing: self.editing.clone(),
        }
    }
    pub(super) fn record(&mut self) {
        self.undo.push(self.snapshot());
        self.redo.clear();
        self.break_group();
    }

    pub fn undo(&mut self, redo: bool) {
        let previous = if redo {
            self.redo.pop()
        } else {
            self.undo.pop()
        };
        if let Some(p) = previous {
            let current = self.snapshot();
            if redo {
                self.undo.push(current)
            } else {
                self.redo.push(current)
            }
            self.restore(p);
        }
        self.break_group();
    }
    pub(super) fn restore(&mut self, p: Snapshot) {
        self.rope = p.rope;
        self.caret = p.caret;
        self.anchor = p.anchor;
        self.formulas = p.formulas;
        self.cancelled = p.cancelled;
        self.editing = p.editing;
    }
}
