use crate::math::{FormulaImage, Job};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
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
use unicode_segmentation::UnicodeSegmentation;

pub const ATOM: &str = "\u{fffc}";
const ATOM_CHAR: char = '\u{fffc}';
const LITERAL_BACKSLASH: &str = "<!--mathstral:literal-->";
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
#[derive(Clone)]
struct Editing {
    start: usize,
    end: usize,
    original: Formula,
    prefix_space: bool,
    suffix_space: bool,
}
#[derive(Clone)]
struct Manual {
    id: u64,
    revision: u64,
    original: Option<Formula>,
}
#[derive(Clone)]
struct Snapshot {
    rope: Rope,
    caret: usize,
    anchor: usize,
    formulas: BTreeMap<usize, Formula>,
    cancelled: BTreeSet<usize>,
    editing: Option<Editing>,
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
    pub fn markdown(&self) -> String {
        self.encode(0..self.rope.len_bytes(), true)
    }
    fn encode(&self, range: Range<usize>, escape: bool) -> String {
        let text = self.text();
        let mut result = String::new();
        let active_edit = self
            .editing
            .as_ref()
            .filter(|edit| escape && range.start <= edit.start && edit.end <= range.end);
        let editing = active_edit.filter(|edit| {
            text[edit.start..edit.end]
                .chars()
                .any(|character| !character.is_whitespace())
        });
        let empty_edit = active_edit.filter(|edit| edit.start == edit.end);
        let mut pos = range.start;
        while pos < range.end {
            if let Some(edit) = empty_edit {
                if edit.prefix_space && pos + 1 == edit.start && text[pos..].starts_with(' ') {
                    pos += 1;
                    continue;
                }
                if edit.suffix_space && pos == edit.end && text[pos..].starts_with(' ') {
                    pos += 1;
                    continue;
                }
            }
            if let Some(edit) = editing {
                if pos == edit.start {
                    result.push('\\');
                }
                if pos == edit.end {
                    result.push('\\');
                }
            }
            if text[pos..range.end].starts_with(LITERAL_BACKSLASH) {
                push_literal_marker(&mut result);
                pos += LITERAL_BACKSLASH.len();
                continue;
            }
            if let Some(f) = self.formulas.get(&pos) {
                let display = Self::is_display_in(&text, pos);
                if let Some(latex) = &f.latex {
                    // An empty formula still being written has no source; `$$` would not reopen.
                    if !latex.is_empty() {
                        if display {
                            result.push_str(&format!("$$\n{latex}\n$$"));
                        } else {
                            result.push('$');
                            result.push_str(latex);
                            if has_terminal_tex_comment(latex) {
                                result.push('\n');
                            }
                            result.push('$');
                        }
                        if let Some(prompt) = &f.prompt {
                            result.push_str(&format!(
                                "<!--mathstral:{}-->",
                                URL_SAFE_NO_PAD.encode(prompt)
                            ));
                        }
                    }
                } else {
                    if let Some(prompt) = &f.prompt {
                        push_prompt_label(&mut result, prompt, escape);
                    } else {
                        let label = f.label();
                        result.push_str(&if escape { escape_label(&label) } else { label });
                    }
                }
                pos += ATOM.len();
            } else {
                let c = text[pos..].chars().next().unwrap();
                let literal_prompt_slash =
                    c == '\\' && editing.is_some_and(|edit| edit.start <= pos && pos < edit.end);
                let literal = c == '\\' && (self.cancelled.contains(&pos) || literal_prompt_slash);
                if literal {
                    result.push_str(LITERAL_BACKSLASH);
                }
                let next = pos + c.len_utf8();
                if literal {
                    result.push_str(if escape { r"\\" } else { r"\" });
                } else if c == '$' && escape {
                    result.push_str("\\$");
                } else if c == '\\'
                    && escape
                    && (self.formulas.contains_key(&next) || text[next..].starts_with(['$', '\\']))
                {
                    // A backslash that would otherwise escape the next atom or `$`.
                    result.push_str("\\\\");
                } else {
                    result.push(c);
                }
                pos = next;
            }
        }
        if let Some(edit) = editing {
            if edit.start == range.end {
                result.push('\\');
            }
            if edit.end == range.end {
                result.push('\\');
            }
        }
        result
    }
    pub fn is_display_in(text: &str, pos: usize) -> bool {
        let start = text[..pos].rfind('\n').map_or(0, |i| i + 1);
        let end = text[pos..].find('\n').map_or(text.len(), |i| pos + i);
        text[start..pos].trim().is_empty() && text[pos + ATOM.len()..end].trim().is_empty()
    }
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
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            rope: self.rope.clone(),
            caret: self.caret,
            anchor: self.anchor,
            formulas: self.formulas.clone(),
            cancelled: self.cancelled.clone(),
            editing: self.editing.clone(),
        }
    }
    fn record(&mut self) {
        self.undo.push(self.snapshot());
        self.redo.clear();
        self.break_group();
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
    fn close_prompts(&mut self) {
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
    fn restore(&mut self, p: Snapshot) {
        self.rope = p.rope;
        self.caret = p.caret;
        self.anchor = p.anchor;
        self.formulas = p.formulas;
        self.cancelled = p.cancelled;
        self.editing = p.editing;
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
fn opening_boundary(text: &str, i: usize) -> bool {
    text[..i]
        .chars()
        .next_back()
        .is_none_or(|c| c.is_whitespace() || "([{\"'“‘".contains(c))
}
fn closing_boundary(c: char) -> bool {
    c.is_whitespace() || ".,;:!?)]}(['\"“”‘’".contains(c)
}

fn escape_label(label: &str) -> String {
    let mut out = String::new();
    let mut chars = label.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '$' => out.push_str("\\$"),
            '\\' if matches!(chars.peek(), Some('$' | '\\')) => out.push_str("\\\\"),
            _ => out.push(c),
        }
    }
    out
}

fn push_prompt_label(out: &mut String, prompt: &str, escape: bool) {
    out.push('\\');
    let mut i = 0;
    while i < prompt.len() {
        if prompt[i..].starts_with(LITERAL_BACKSLASH) {
            push_literal_marker(out);
            i += LITERAL_BACKSLASH.len();
            continue;
        }
        let c = prompt[i..].chars().next().unwrap();
        if c == '\\' {
            out.push_str(LITERAL_BACKSLASH);
            out.push_str(if escape { r"\\" } else { r"\" });
        } else if c == '$' && escape {
            out.push_str("\\$");
        } else if c == '$' {
            out.push_str(LITERAL_BACKSLASH);
            out.push('$');
        } else {
            out.push(c);
        }
        i += c.len_utf8();
    }
    out.push('\\');
}

fn push_literal_marker(out: &mut String) {
    out.push_str(LITERAL_BACKSLASH);
    out.push_str(LITERAL_BACKSLASH);
}

fn unescape_literal_markers(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let mut i = 0;
    while i < source.len() {
        if source[i..].starts_with(LITERAL_BACKSLASH) {
            let marker_end = i + LITERAL_BACKSLASH.len();
            if source[marker_end..].starts_with(LITERAL_BACKSLASH) {
                result.push_str(LITERAL_BACKSLASH);
                i = marker_end + LITERAL_BACKSLASH.len();
                continue;
            }
        }
        let c = source[i..].chars().next().unwrap();
        result.push(c);
        i += c.len_utf8();
    }
    result
}

fn is_tex_escaped(source: &str, index: usize) -> bool {
    let bytes = source.as_bytes();
    let mut slashes = 0;
    let mut i = index;
    while i > 0 && bytes[i - 1] == b'\\' {
        slashes += 1;
        i -= 1;
    }
    slashes % 2 == 1
}

fn find_tex_delimiter(source: &str, start: usize, delimiter: &str) -> Option<usize> {
    let mut i = start;
    while i < source.len() {
        match source.as_bytes()[i] {
            b'\\' => {
                i += 1;
                if i < source.len() {
                    i += source[i..].chars().next().unwrap().len_utf8();
                }
            }
            b'%' => {
                let newline = source[i..].find('\n')?;
                i += newline + 1;
            }
            _ if source[i..].starts_with(delimiter) => return Some(i),
            _ => i += source[i..].chars().next().unwrap().len_utf8(),
        }
    }
    None
}

fn has_terminal_tex_comment(source: &str) -> bool {
    let mut i = 0;
    let mut in_comment = false;
    while i < source.len() {
        match source.as_bytes()[i] {
            b'\n' if in_comment => in_comment = false,
            b'\\' if !in_comment => {
                i += 1;
                if i < source.len() {
                    i += source[i..].chars().next().unwrap().len_utf8();
                }
                continue;
            }
            b'%' if !in_comment => in_comment = true,
            _ => {}
        }
        i += source[i..].chars().next().unwrap().len_utf8();
    }
    in_comment
}

// Files use `unescape`; pasted text is taken literally, as before escapes existed.
fn decode(source: &str, unescape: bool) -> (String, BTreeMap<usize, Formula>, BTreeSet<usize>) {
    let mut text = String::new();
    let mut formulas = BTreeMap::new();
    let mut cancelled = BTreeSet::new();
    let mut i = 0;
    while i < source.len() {
        if source[i..].starts_with(LITERAL_BACKSLASH) {
            let marker_end = i + LITERAL_BACKSLASH.len();
            if source[marker_end..].starts_with(LITERAL_BACKSLASH) {
                text.push_str(LITERAL_BACKSLASH);
                i = marker_end + LITERAL_BACKSLASH.len();
                continue;
            }
            i += LITERAL_BACKSLASH.len();
            cancelled.insert(text.len());
            continue;
        }
        if unescape {
            if source[i..].starts_with("\\\\") {
                text.push('\\');
                i += 2;
                continue;
            }
            if source[i..].starts_with("\\$") {
                text.push('$');
                i += 2;
                continue;
            }
        }
        if source.as_bytes()[i] == b'$'
            && !cancelled.contains(&text.len())
            && (unescape || !is_tex_escaped(source, i))
        {
            let display = source[i..].starts_with("$$");
            let delimiter = if display { "$$" } else { "$" };
            let start = i + delimiter.len();
            let end = find_tex_delimiter(source, start, delimiter);
            if let Some(end) = end.filter(|&end| end > start) {
                let body = &source[start..end];
                let latex = if display {
                    let body = body.strip_prefix('\n').unwrap_or(body);
                    let body = body.strip_suffix('\n').unwrap_or(body);
                    body.to_owned()
                } else {
                    body.to_owned()
                };
                i = end + delimiter.len();
                let mut prompt = None;
                let prefix = "<!--mathstral:";
                if source[i..].starts_with(prefix)
                    && let Some(end) = source[i + prefix.len()..].find("-->")
                    && let encoded = &source[i + prefix.len()..i + prefix.len() + end]
                    && let Ok(bytes) = URL_SAFE_NO_PAD.decode(encoded)
                    && let Ok(p) = String::from_utf8(bytes)
                {
                    prompt = Some(p);
                    i += prefix.len() + end + 3;
                }
                formulas.insert(text.len(), Formula::new(prompt, Some(latex)));
                text.push_str(ATOM);
                continue;
            }
        }
        let c = source[i..].chars().next().unwrap();
        // Stray atom chars would be mistaken for formula markers.
        if c != ATOM_CHAR {
            text.push(c);
        }
        i += c.len_utf8();
    }
    (text, formulas, cancelled)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn type_text(d: &mut Document, s: &str) {
        for c in s.chars() {
            d.replace(&c.to_string(), true);
        }
    }
    #[test]
    fn backslash_rules() {
        let mut d = Document::new(r"and\or C:\a\b \1\2 plus a\b\ done");
        assert_eq!(d.formulas.len(), 1);
        assert_eq!(
            d.formulas.values().next().unwrap().prompt.as_deref(),
            Some(r"1\2 plus a\b")
        );
        assert_eq!(d.text(), format!(r"and\or C:\a\b {ATOM} done"));
        d.move_to(d.rope.len_bytes(), false);
        type_text(&mut d, r" \unfinished");
        assert!(d.open_range().is_some());
        assert!(d.cancel());
        type_text(&mut d, r"\");
        assert_eq!(d.formulas.len(), 1);
        assert_eq!(Document::new(&d.markdown()).formulas.len(), 1);
        let mut d = Document::new("");
        type_text(&mut d, r"\1\2");
        assert_eq!(d.formulas.len(), 1);
        assert_eq!(d.text(), format!("{ATOM}2"));
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
    fn portable_math_is_never_in_visible_buffer() {
        let source =
            "Before $\\frac{1}{3}$<!--mathstral:MSBvdmVyIDM--> after\n$$\n\\int_0^1 x dx\n$$";
        let d = Document::new(source);
        assert_eq!(d.formulas.len(), 2);
        assert!(!d.text().contains("frac"));
        assert_eq!(d.formulas[&7].prompt.as_deref(), Some("1 over 3"));
        assert_eq!(d.markdown(), source);
    }
    #[test]
    fn manual_latex_round_trips_through_save_and_copy() {
        for (latex, expected) in [
            (r"a\\", r"a\\".to_owned()),
            (r"x\$y", r"x\$y".to_owned()),
            (r"a\\\$b", r"a\\\$b".to_owned()),
            (r"\alpha + λ", r"\alpha + λ".to_owned()),
            ("x\n+y", "x\n+y".to_owned()),
            (" x \n+y ", " x \n+y ".to_owned()),
            ("x% note $\n+y", "x% note $\n+y".to_owned()),
            ("x% terminal $", "x% terminal $\n".to_owned()),
        ] {
            let mut doc = Document::new("before after");
            doc.move_to(7, false);
            doc.open_manual();
            doc.update_manual(latex.to_owned());
            doc.finish_manual();

            let saved = doc.markdown();
            let mut reopened = Document::new(&saved);
            let formula_pos = *reopened.formulas.keys().next().unwrap();
            assert_eq!(
                reopened.formulas[&formula_pos].latex.as_deref(),
                Some(expected.as_str()),
                "{latex:?}"
            );
            assert_eq!(reopened.markdown(), saved, "{latex:?}");
            let reopened_again = Document::new(&reopened.markdown());
            assert_eq!(
                reopened_again.formulas[&7].latex.as_deref(),
                Some(expected.as_str()),
                "{latex:?}"
            );

            reopened.move_to(formula_pos, false);
            reopened.move_to(formula_pos + ATOM.len(), true);
            let copied = reopened.selected();
            let mut pasted = Document::new("");
            pasted.paste(&copied);
            let pasted_formula = pasted.formulas.values().next().unwrap();
            assert_eq!(
                pasted_formula.latex.as_deref(),
                Some(expected.as_str()),
                "copy {latex:?}"
            );
        }
    }
    #[test]
    fn display_latex_preserves_source_whitespace_and_compact_legacy_form() {
        for latex in [
            " x \n\n",
            r"a\\",
            r"a\",
            r"x\$y",
            r"a\\\$b",
            r"\alpha + λ",
            "x% note $\n+y",
            "x% terminal $",
        ] {
            let mut doc = Document::new("");
            doc.open_manual();
            doc.update_manual(latex.to_owned());
            doc.finish_manual();
            let saved = doc.markdown();
            let mut reopened = Document::new(&saved);
            assert_eq!(
                reopened.formulas.values().next().unwrap().latex.as_deref(),
                Some(latex),
                "{latex:?}"
            );
            assert_eq!(reopened.markdown(), saved, "{latex:?}");
            reopened.move_to(0, false);
            reopened.move_to(ATOM.len(), true);
            let copied = reopened.selected();
            let mut pasted = Document::new("");
            pasted.paste(&copied);
            assert_eq!(
                pasted.formulas.values().next().unwrap().latex.as_deref(),
                Some(latex),
                "copy {latex:?}"
            );
        }

        let compact = Document::new("$$x$$");
        assert_eq!(compact.formulas[&0].latex.as_deref(), Some("x"));
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
    fn dollars_in_prose_survive_save_and_reopen() {
        let mut d = Document::new("");
        type_text(&mut d, "I paid $5 and $6");
        assert_eq!(d.markdown(), "I paid \\$5 and \\$6");
        let reopened = Document::new(&d.markdown());
        assert!(reopened.formulas.is_empty());
        assert_eq!(reopened.text(), "I paid $5 and $6");
        assert!(!reopened.dirty());
        let mut d = Document::new("");
        d.replace("a\\", true);
        d.replace("$", true);
        assert_eq!(d.markdown(), "a\\\\\\$");
        assert_eq!(Document::new(&d.markdown()).text(), "a\\$");
        let d = Document::new(r"x \cost \$5\ y");
        assert_eq!(
            d.formulas.values().next().unwrap().prompt.as_deref(),
            Some("cost $5")
        );
        assert_eq!(d.markdown(), r"x \cost \$5\ y");
        let reopened = Document::new(&d.markdown());
        assert_eq!(
            reopened.formulas.values().next().unwrap().prompt.as_deref(),
            Some("cost $5")
        );
        let mut d = Document::new("$5");
        d.move_to(0, false);
        d.move_to(d.rope.len_bytes(), true);
        assert_eq!(d.selected(), "$5");
    }
    fn assert_round_trip(d: &Document) {
        let reopened = Document::new(&d.markdown());
        assert_eq!(reopened.text(), d.text());
        assert_eq!(reopened.formulas.len(), d.formulas.len());
        for (a, b) in d.formulas.values().zip(reopened.formulas.values()) {
            assert_eq!(a.prompt, b.prompt);
            assert_eq!(a.latex, b.latex);
        }
        assert!(!reopened.dirty());
    }
    #[test]
    fn backslashes_round_trip() {
        for prose in ["C:\\path", "a\\\\b", "\\$5", "$5 and $6", "end\\"] {
            let mut d = Document::new("");
            d.replace(prose, false);
            assert_eq!(d.text(), prose);
            assert_round_trip(&d);
        }
        let mut d = Document::new("");
        d.replace("C:\\path", false);
        assert_eq!(d.markdown(), "C:\\path");
        let mut d = Document::new("");
        d.replace("a\\", false);
        d.replace("$", false);
        d.replace("x", false);
        assert_round_trip(&d);
        let d = Document::new("a \\\\$x$ b");
        assert_eq!(d.text(), format!("a \\{ATOM} b"));
        assert_eq!(d.markdown(), "a \\\\$x$ b");
        assert_round_trip(&d);
        let d = Document::new(r"x \cost \$5\ y");
        assert_eq!(d.markdown(), r"x \cost \$5\ y");
        assert_round_trip(&d);
        assert_eq!(
            Document::new(&d.markdown())
                .formulas
                .values()
                .next()
                .unwrap()
                .prompt
                .as_deref(),
            Some("cost $5")
        );
    }
    #[test]
    fn backslash_prompts_round_trip() {
        let mut d = Document::new("");
        type_text(&mut d, r"\cost $5\ and a\b end");
        assert_eq!(
            d.formulas.values().next().unwrap().prompt.as_deref(),
            Some("cost $5")
        );
        assert_round_trip(&d);
        let mut d = Document::new("");
        type_text(&mut d, r"a\\ \x\");
        assert_eq!(d.text(), format!(r"a\\ {ATOM}"));
        assert_round_trip(&d);
        let mut d = Document::new("");
        type_text(&mut d, r"\$5 and x");
        assert!(d.cancel());
        assert_eq!(d.markdown(), r"<!--mathstral:literal-->\\\$5 and x");
        assert_round_trip(&d);
        assert!(Document::new(&d.markdown()).formulas.is_empty());
        let d = Document::new("/x/ and /y");
        assert!(d.formulas.is_empty());
        assert_eq!(d.text(), "/x/ and /y");
        let mut d = Document::new("");
        type_text(&mut d, "/x/");
        assert!(d.formulas.is_empty());
        assert_round_trip(&d);
    }
    #[test]
    fn prompt_backslashes_survive_save_edit_and_copy_round_trips() {
        for prompt in [
            "a \\ b",
            "a \\\\ b",
            r"\alpha",
            "a \\ $5",
            "solve $x=1$",
            "trailing\\",
        ] {
            let mut editing = Document::new("");
            editing.open_ai_prompt();
            type_text(&mut editing, prompt);
            assert_eq!(editing.text(), prompt);

            let editing_saved = editing.markdown();
            assert_eq!(
                editing_saved.contains(LITERAL_BACKSLASH),
                prompt.contains('\\'),
                "{prompt:?}: {editing_saved:?}"
            );
            let editing_reopened = Document::new(&editing_saved);
            assert_eq!(
                editing_reopened
                    .formulas
                    .values()
                    .next()
                    .unwrap()
                    .prompt
                    .as_deref(),
                Some(prompt),
                "editing prompt {prompt:?}"
            );

            assert!(editing.submit_prompt());
            let saved = editing.markdown();
            assert_eq!(saved.contains(LITERAL_BACKSLASH), prompt.contains('\\'));
            let reopened = Document::new(&saved);
            assert_eq!(
                reopened.formulas.values().next().unwrap().prompt.as_deref(),
                Some(prompt),
                "submitted prompt {prompt:?}"
            );

            editing.move_to(0, false);
            editing.move_to(ATOM.len(), true);
            let copied = editing.selected();
            assert_eq!(
                copied.contains(LITERAL_BACKSLASH),
                prompt.contains('\\') || prompt.contains('$'),
                "{prompt:?}"
            );
            let mut pasted = Document::new("");
            pasted.paste(&copied);
            assert_eq!(
                pasted.formulas.values().next().unwrap().prompt.as_deref(),
                Some(prompt),
                "copied prompt {prompt:?}"
            );
        }
    }
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
    fn image(latex: &str) -> Arc<FormulaImage> {
        let pixels = || crate::math::Image {
            rgba: vec![],
            width: 0,
            height: 0,
            baseline: 0.0,
        };
        Arc::new(FormulaImage {
            latex: latex.into(),
            inline: pixels(),
            display: pixels(),
        })
    }
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
        let layout = crate::layout::Typesetter::new().document(&d, 680.0);
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
        let mut d = Document::new(r"a \one\ b");
        let undo = d.undo.len();
        assert_eq!(d.reopen_manual(2), None);
        assert_eq!(d.reopen_manual(0), None);
        assert_eq!(d.undo.len(), undo);
        assert!(d.manual_id().is_none());
        assert_eq!(d.formulas[&2].prompt.as_deref(), Some("one"));
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
    fn reopening_prompt_edits_plain_text_and_saves_legacy_delimiters() {
        let mut doc = Document::new(r"before \sum_{i=1}^n i\ after");
        assert_eq!(doc.formulas[&7].label(), r"sum_{i=1}^n i");
        assert!(doc.reopen(7));
        assert_eq!(doc.text(), "before sum_{i=1}^n i after");
        assert_eq!(doc.markdown(), r"before \sum_{i=1}^n i\ after");
        assert!(doc.submit_prompt());
        assert_eq!(doc.formulas[&7].prompt.as_deref(), Some("sum_{i=1}^n i"));
    }
}
