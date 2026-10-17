//! AI prompt lifecycle: opening, submitting, cancelling and reopening prompt formulas.
use super::{ATOM, ATOM_CHAR, Document, Formula};
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
    pub(super) fn close_prompts(&mut self) {
        loop {
            let text = self.text();
            let mut open = None;
            let mut found = None;
            for (i, c) in text.char_indices() {
                if c == ATOM_CHAR {
                    open = None;
                    continue;
                }
                if c != '\\' {
                    continue;
                }
                if self
                    .editing
                    .as_ref()
                    .is_some_and(|edit| edit.start <= i && i < edit.end)
                {
                    continue;
                }
                if self.cancelled.contains(&i) {
                    continue;
                }
                if let Some(start) = open {
                    if self.editing.as_ref().is_some_and(|e| e.start == start) {
                        continue;
                    }
                    let body = &text[start + 1..i];
                    if !body.trim().is_empty()
                        && text[i + 1..].chars().next().is_none_or(closing_boundary)
                    {
                        found = Some((start, i + 1, body.to_owned()));
                        break;
                    }
                } else if !self.cancelled.contains(&i) && opening_boundary(&text, i) {
                    open = Some(i);
                }
            }
            let Some((start, end, prompt)) = found else {
                break;
            };
            self.splice(start..end, ATOM);
            self.formulas
                .insert(start, Formula::new(Some(prompt), None));
        }
    }
    pub fn open_range(&self) -> Option<Range<usize>> {
        if let Some(e) = &self.editing {
            return Some(e.start..e.end);
        }
        let text = self.text();
        let mut open = None;
        for (i, c) in text.char_indices() {
            if c == ATOM_CHAR {
                open = None;
            }
            if c == '\\'
                && open.is_none()
                && !self.cancelled.contains(&i)
                && opening_boundary(&text, i)
            {
                open = Some(i);
            }
        }
        open.map(|start| start..text.len())
    }
    pub fn cancel(&mut self) -> bool {
        if let Some(range) = self
            .open_range()
            .filter(|r| r.start <= self.caret && self.caret <= r.end)
        {
            self.record();
            let edit = self.editing.clone();
            if let Some(edit) = edit.as_ref().filter(|edit| edit.start == edit.end) {
                let text = self.text();
                let start = edit.start
                    - usize::from(
                        edit.prefix_space
                            && edit.start > 0
                            && text.as_bytes()[edit.start - 1] == b' ',
                    );
                let end = edit.end
                    + usize::from(
                        edit.suffix_space && text.as_bytes().get(edit.end) == Some(&b' '),
                    );
                self.editing = None;
                self.splice(start..end, "");
                self.caret = start;
                self.anchor = start;
                return true;
            }
            if let Some(edit) = edit {
                let text = self.text();
                self.cancelled.extend(
                    text[edit.start..edit.end]
                        .char_indices()
                        .filter_map(|(offset, c)| (c == '\\').then_some(edit.start + offset)),
                );
            } else {
                self.cancelled.insert(range.start);
            }
            self.editing = None;
            return true;
        }
        false
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
    /// Submits the prompt currently being edited, or a legacy open prompt at the caret.
    pub fn submit_prompt(&mut self) -> bool {
        if let Some(edit) = &self.editing
            && !(edit.start <= self.caret && self.caret <= edit.end)
        {
            return false;
        }
        let text = self.text();
        let (range, prompt, original, prefix_space, suffix_space) =
            if let Some(edit) = self.editing.clone() {
                let Some(label) = text.get(edit.start..edit.end) else {
                    return false;
                };
                let prompt = label;
                (
                    edit.start..edit.end,
                    prompt.to_owned(),
                    Some(edit.original),
                    edit.prefix_space,
                    edit.suffix_space,
                )
            } else {
                let Some(open) = self
                    .open_range()
                    .filter(|r| r.start < self.caret && self.caret <= r.end)
                else {
                    return false;
                };
                let mut end = self.caret;
                if text[..end].ends_with('\\') && end > open.start + 1 {
                    end -= 1;
                }
                let Some(prompt) = text.get(open.start + 1..end) else {
                    return false;
                };
                (open.start..end, prompt.to_owned(), None, false, false)
            };

        self.record();
        self.editing = None;
        if prompt.trim().is_empty() {
            let text = self.text();
            let start = range.start
                - usize::from(
                    prefix_space && range.start > 0 && text.as_bytes()[range.start - 1] == b' ',
                );
            let end = range.end
                + usize::from(suffix_space && text.as_bytes().get(range.end) == Some(&b' '));
            self.splice(start..end, "");
            self.caret = start;
        } else {
            self.splice(range.clone(), ATOM);
            let formula = original
                .filter(|formula| formula.prompt.as_deref() == Some(prompt.as_str()))
                .unwrap_or_else(|| Formula::new(Some(prompt), None));
            self.formulas.insert(range.start, formula);
            self.caret = range.start + ATOM.len();
        }
        self.anchor = self.caret;
        self.editing = None;
        self.close_prompts();
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
