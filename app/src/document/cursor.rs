//! Cursor movement: caret placement, formula edges and grapheme and word steps.
use super::{ATOM, Document};
use unicode_segmentation::UnicodeSegmentation;

impl Document {
    pub fn formula_at_caret(&self, backspace: bool) -> Option<usize> {
        if !self.selection().is_empty() {
            return None;
        }
        if backspace {
            self.caret
                .checked_sub(ATOM.len())
                .filter(|p| self.formulas.contains_key(p))
        } else {
            self.formulas
                .contains_key(&self.caret)
                .then_some(self.caret)
        }
    }

    pub fn move_to(&mut self, position: usize, extend: bool) {
        self.caret = position.min(self.rope.len_bytes());
        if let Some((&start, _)) = self.formulas.range(..self.caret).next_back()
            && self.caret < start + ATOM.len()
        {
            self.caret = start;
        }
        if !extend {
            self.anchor = self.caret;
        }
        self.break_group();
    }

    pub fn adjacent(&self, forward: bool, word: bool) -> usize {
        let text = self.text();
        let edge = if forward {
            self.formulas
                .get(&self.caret)
                .map(|_| self.caret + ATOM.len())
        } else {
            self.caret
                .checked_sub(ATOM.len())
                .filter(|p| self.formulas.contains_key(p))
        };
        if let Some(edge) = edge {
            return edge;
        }
        if word {
            let target = if forward {
                text.unicode_word_indices()
                    .map(|(i, w)| i + w.len())
                    .find(|&i| i > self.caret)
                    .unwrap_or(text.len())
            } else {
                text.unicode_word_indices()
                    .map(|(i, _)| i)
                    .take_while(|&i| i < self.caret)
                    .last()
                    .unwrap_or(0)
            };
            if forward {
                return self
                    .formulas
                    .range(self.caret..target)
                    .next()
                    .map_or(target, |(&p, _)| p);
            }
            return self
                .formulas
                .range(target..self.caret)
                .next_back()
                .map_or(target, |(&p, _)| p + ATOM.len());
        }
        if forward {
            text.grapheme_indices(true)
                .map(|(i, _)| i)
                .find(|&i| i > self.caret)
                .unwrap_or(text.len())
        } else {
            text.grapheme_indices(true)
                .map(|(i, _)| i)
                .take_while(|&i| i < self.caret)
                .last()
                .unwrap_or(0)
        }
    }
}
