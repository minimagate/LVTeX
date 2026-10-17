//! AI prompt lifecycle: opening, submitting, cancelling and reopening prompt formulas.
use super::{ATOM, Document, Formula};
use std::ops::Range;

#[derive(Clone)]
pub(super) struct Editing {
    pub(super) start: usize,
    pub(super) end: usize,
    original: Formula,
    pub(super) prefix_space: bool,
    pub(super) suffix_space: bool,
}

impl Document {
    /// Marks a backslash that an edit left at an opening boundary as literal, the same mark Escape
    /// leaves. Only such a backslash can open a prompt, so the others stay plain text and save
    /// without the literal marker. Checks the backslashes at the edit and the one just after it,
    /// since only that character's opening status can change. Backslashes inside an open AI prompt
    /// stay plain prompt text.
    pub(super) fn mark_literal_backslashes(&mut self, start: usize, inserted_len: usize) {
        let text = self.text();
        let end = start + inserted_len;
        let marks: Vec<usize> = text
            .char_indices()
            .filter(|&(pos, c)| {
                c == '\\'
                    && start <= pos
                    && pos <= end
                    && opening_boundary(&text, pos)
                    && !self
                        .editing
                        .as_ref()
                        .is_some_and(|edit| edit.start <= pos && pos < edit.end)
            })
            .map(|(pos, _)| pos)
            .collect();
        self.cancelled.extend(marks);
    }
    pub fn open_range(&self) -> Option<Range<usize>> {
        self.editing.as_ref().map(|e| e.start..e.end)
    }
    pub fn cancel(&mut self) -> bool {
        let Some(edit) = self
            .editing
            .clone()
            .filter(|edit| edit.start <= self.caret && self.caret <= edit.end)
        else {
            return false;
        };
        self.record();
        if edit.start == edit.end {
            let text = self.text();
            let start = edit.start
                - usize::from(
                    edit.prefix_space && edit.start > 0 && text.as_bytes()[edit.start - 1] == b' ',
                );
            let end = edit.end
                + usize::from(edit.suffix_space && text.as_bytes().get(edit.end) == Some(&b' '));
            self.editing = None;
            self.splice(start..end, "");
            self.caret = start;
            self.anchor = start;
            return true;
        }
        let text = self.text();
        self.cancelled.extend(
            text[edit.start..edit.end]
                .char_indices()
                .filter_map(|(offset, c)| (c == '\\').then_some(edit.start + offset)),
        );
        self.editing = None;
        true
    }

    pub fn reopen(&mut self, pos: usize) -> bool {
        let Some(f) = self.formulas.get(&pos).cloned() else {
            return false;
        };
        if f.prompt.is_none() {
            return false;
        }
        self.record();
        let label = f.label();
        self.splice(pos..pos + ATOM.len(), &label);
        self.caret = pos + label.len();
        self.anchor = self.caret;
        self.editing = Some(Editing {
            start: pos,
            end: pos + label.len(),
            original: f,
            prefix_space: false,
            suffix_space: false,
        });
        true
    }
    /// Starts an AI prompt edit, using the current selection as its initial prompt.
    pub fn open_ai_prompt(&mut self) {
        self.record();
        let range = self.selection();
        let text = self.text();
        let prompt = self.encode(range.clone(), false);
        let prefix = if opening_boundary(&text, range.start) {
            ""
        } else {
            " "
        };
        let suffix = if text[range.end..]
            .chars()
            .next()
            .is_some_and(|c| !closing_boundary(c))
        {
            " "
        } else {
            ""
        };
        let label = format!("{prefix}{prompt}{suffix}");
        let start = range.start + prefix.len();
        let end = start + prompt.len();
        let original = Formula::new(Some(prompt), None);
        self.splice(range, &label);
        self.editing = Some(Editing {
            start,
            end,
            original,
            prefix_space: !prefix.is_empty(),
            suffix_space: !suffix.is_empty(),
        });
        self.caret = end;
        self.anchor = self.caret;
    }
    /// Submits the prompt currently being edited.
    pub fn submit_prompt(&mut self) -> bool {
        let Some(edit) = self.editing.clone() else {
            return false;
        };
        if !(edit.start <= self.caret && self.caret <= edit.end) {
            return false;
        }
        let text = self.text();
        let Some(label) = text.get(edit.start..edit.end) else {
            return false;
        };
        let prompt = label.to_owned();
        let range = edit.start..edit.end;

        self.record();
        self.editing = None;
        if prompt.trim().is_empty() {
            let text = self.text();
            let start = range.start
                - usize::from(
                    edit.prefix_space
                        && range.start > 0
                        && text.as_bytes()[range.start - 1] == b' ',
                );
            let end = range.end
                + usize::from(edit.suffix_space && text.as_bytes().get(range.end) == Some(&b' '));
            self.splice(start..end, "");
            self.caret = start;
        } else {
            self.splice(range.clone(), ATOM);
            let formula = if edit.original.prompt.as_deref() == Some(prompt.as_str()) {
                edit.original
            } else {
                Formula::new(Some(prompt), None)
            };
            self.formulas.insert(range.start, formula);
            self.caret = range.start + ATOM.len();
        }
        self.anchor = self.caret;
        self.editing = None;
        self.break_group();
        true
    }
}

fn opening_boundary(text: &str, i: usize) -> bool {
    text[..i]
        .chars()
        .next_back()
        .is_none_or(|c| c.is_whitespace() || "([{\"'“‘".contains(c))
}
fn closing_boundary(c: char) -> bool {
    c.is_whitespace() || ".,;:!?)]}(['\"“”‘’".contains(c)
}
