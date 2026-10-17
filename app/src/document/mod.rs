//! Document model: text, formulas and their editing state.
mod codec;
mod cursor;
mod history;
mod manual;
mod prompt;
#[cfg(test)]
mod tests;

use crate::math::{render::FormulaImage, worker::Job};
use codec::{decode, unescape_literal_markers};
use history::Snapshot;
use manual::Manual;
use prompt::Editing;
use ropey::Rope;
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

pub const ATOM: &str = "\u{fffc}";
const ATOM_CHAR: char = '\u{fffc}';
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub struct Formula {
    pub id: u64,
    pub prompt: Option<String>,
    pub latex: Option<String>,
    pub image: Option<Arc<FormulaImage>>,
    pub failed: bool,
}
impl Formula {
    fn new(prompt: Option<String>, latex: Option<String>) -> Self {
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            prompt,
            latex,
            image: None,
            failed: false,
        }
    }
    pub fn label(&self) -> String {
        if let Some(prompt) = &self.prompt {
            prompt.clone()
        } else {
            self.latex.as_deref().unwrap_or("math…").to_owned()
        }
    }
}

pub struct Document {
    pub rope: Rope,
    pub caret: usize,
    pub anchor: usize,
    pub formulas: BTreeMap<usize, Formula>,
    cancelled: BTreeSet<usize>,
    editing: Option<Editing>,
    manual: Option<Manual>,
    saved: String,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    typing: Option<Instant>,
}
impl Document {
    pub fn new(text: &str) -> Self {
        Self::parse(text, true)
    }
    fn parse(text: &str, unescape: bool) -> Self {
        let mut d = Self {
            rope: Rope::new(),
            caret: 0,
            anchor: 0,
            formulas: BTreeMap::new(),
            cancelled: BTreeSet::new(),
            editing: None,
            manual: None,
            saved: String::new(),
            undo: vec![],
            redo: vec![],
            typing: None,
        };
        let (plain, atoms, cancelled) = decode(text, unescape);
        d.rope = Rope::from_str(&plain);
        d.formulas = atoms;
        d.cancelled = cancelled;
        d.close_prompts();
        d.caret = 0;
        d.anchor = 0;
        d.saved = d.markdown();
        d
    }
    pub fn text(&self) -> String {
        self.rope.to_string()
    }
    pub fn selection(&self) -> Range<usize> {
        self.caret.min(self.anchor)..self.caret.max(self.anchor)
    }
    pub fn selected(&self) -> String {
        self.encode(self.selection(), false)
    }

    pub fn is_display_in(text: &str, pos: usize) -> bool {
        let start = text[..pos].rfind('\n').map_or(0, |i| i + 1);
        let end = text[pos..].find('\n').map_or(text.len(), |i| pos + i);
        text[start..pos].trim().is_empty() && text[pos + ATOM.len()..end].trim().is_empty()
    }

    fn splice(&mut self, range: Range<usize>, value: &str) {
        let delta = value.len() as isize - (range.end - range.start) as isize;
        self.formulas = std::mem::take(&mut self.formulas)
            .into_iter()
            .filter_map(|(pos, f)| {
                if range.contains(&pos) {
                    None
                } else {
                    Some((
                        if pos >= range.end {
                            pos.wrapping_add_signed(delta)
                        } else {
                            pos
                        },
                        f,
                    ))
                }
            })
            .collect();
        self.cancelled = self
            .cancelled
            .iter()
            .filter_map(|&pos| {
                if range.contains(&pos) {
                    None
                } else {
                    Some(if pos >= range.end {
                        pos.wrapping_add_signed(delta)
                    } else {
                        pos
                    })
                }
            })
            .collect();
        if let Some(edit) = &mut self.editing {
            if range.is_empty() && edit.start <= range.start && range.start <= edit.end {
                edit.end = edit.end.wrapping_add_signed(delta);
            } else if range.end <= edit.start {
                edit.start = edit.start.wrapping_add_signed(delta);
                edit.end = edit.end.wrapping_add_signed(delta);
            } else if range.start >= edit.start && range.end <= edit.end {
                edit.end = edit.end.wrapping_add_signed(delta);
            } else if range.start >= edit.end {
                // Edits after the active prompt do not change its text.
            } else {
                self.editing = None;
            }
        }
        let start = self.rope.byte_to_char(range.start);
        let end = self.rope.byte_to_char(range.end);
        self.rope.remove(start..end);
        self.rope.insert(start, value);
        for p in [&mut self.caret, &mut self.anchor] {
            if *p >= range.end {
                *p = p.wrapping_add_signed(delta);
            } else if *p > range.start {
                *p = range.start + value.len();
            }
        }
    }
    pub fn replace(&mut self, value: &str, typing: bool) {
        // A literal atom char would be mistaken for a formula marker.
        let stripped = value.replace(ATOM_CHAR, "");
        if stripped.is_empty() && value.contains(ATOM_CHAR) {
            return;
        }
        let value = stripped.as_str();
        let range = self.selection();
        if range.is_empty() && value.is_empty() {
            return;
        }
        let now = Instant::now();
        let structural = value.contains('\\') || value.contains('\n');
        let grouped = typing
            && !structural
            && range.is_empty()
            && self
                .typing
                .is_some_and(|t| now.duration_since(t) < Duration::from_millis(900));
        if !grouped {
            self.undo.push(self.snapshot());
        }
        self.redo.clear();
        let range = self.selection();
        self.splice(range.clone(), value);
        self.caret = range.start + value.len();
        self.anchor = self.caret;
        self.close_prompts();
        self.typing = if typing && !structural {
            Some(now)
        } else {
            None
        };
    }
    pub fn paste(&mut self, text: &str) {
        let selection = self.selection();
        if self.editing.as_ref().is_some_and(|edit| {
            edit.start <= selection.start
                && selection.end <= edit.end
                && edit.start <= self.caret
                && self.caret <= edit.end
        }) {
            self.replace(&unescape_literal_markers(text), false);
            return;
        }
        self.record();
        let imported = Document::parse(text, false);
        let range = self.selection();
        let start = range.start;
        let plain = imported.text();
        self.splice(range, &plain);
        for (pos, formula) in imported.formulas {
            self.formulas.insert(start + pos, formula);
        }
        for pos in imported.cancelled {
            self.cancelled.insert(start + pos);
        }
        self.caret = start + plain.len();
        self.anchor = self.caret;
        self.close_prompts();
    }

    pub fn retry(&mut self, pos: usize) {
        if let Some(f) = self.formulas.get_mut(&pos) {
            f.id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            f.failed = false;
        }
    }
    pub fn jobs(&self) -> Vec<Job> {
        self.formulas
            .values()
            .filter(|f| {
                !f.failed
                    && self.manual_id() != Some(f.id)
                    && f.image
                        .as_ref()
                        .is_none_or(|image| f.latex.as_deref() != Some(image.latex.as_str()))
            })
            .map(|f| Job {
                id: f.id,
                prompt: f.prompt.clone(),
                latex: f.latex.clone(),
            })
            .collect()
    }
    pub fn complete(&mut self, id: u64, result: Result<Arc<FormulaImage>, String>) -> bool {
        fn apply(f: &mut Formula, result: &Result<Arc<FormulaImage>, String>) {
            match result {
                Ok(image) => {
                    f.latex = Some(image.latex.clone());
                    f.image = Some(image.clone());
                    f.failed = false;
                }
                Err(_) => {
                    f.image = None;
                    f.failed = true;
                }
            }
        }
        let mut changed = false;
        for f in self.formulas.values_mut().filter(|f| f.id == id) {
            apply(f, &result);
            changed = true;
        }
        // Completion is presentation, not a new undo step. Redo restores the same result.
        for snap in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            for f in snap.formulas.values_mut().filter(|f| f.id == id) {
                apply(f, &result);
            }
        }
        changed
    }
}
