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
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
#[derive(Clone)]
pub struct Formula {
    pub id: u64,
    pub prompt: Option<String>,
    pub latex: Option<String>,
    pub image: Option<Arc<FormulaImage>>,
    pub failed: bool,
    provisional: bool,
}
impl Formula {
    fn new(prompt: Option<String>, latex: Option<String>) -> Self {
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            prompt,
            latex,
            image: None,
            failed: false,
            provisional: false,
        }
    }
    pub fn label(&self) -> String {
        self.prompt
            .as_ref()
            .map(|p| format!("/{p}/"))
            .unwrap_or_else(|| "math…".into())
    }
}
#[derive(Clone)]
struct Editing {
    start: usize,
    end: usize,
    original: Formula,
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
    saved: String,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    typing: Option<Instant>,
}
impl Document {
    pub fn new(text: &str) -> Self {
        let mut d = Self {
            rope: Rope::new(),
            caret: 0,
            anchor: 0,
            formulas: BTreeMap::new(),
            cancelled: BTreeSet::new(),
            editing: None,
            saved: String::new(),
            undo: vec![],
            redo: vec![],
            typing: None,
        };
        let (plain, atoms, cancelled) = decode(text);
        d.rope = Rope::from_str(&plain);
        d.formulas = atoms;
        d.cancelled = cancelled;
        d.close_prompts();
        for f in d.formulas.values_mut() {
            f.provisional = false;
        }
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
        self.encode(self.selection())
    }
    pub fn markdown(&self) -> String {
        self.encode(0..self.rope.len_bytes())
    }
    fn encode(&self, range: Range<usize>) -> String {
        let text = self.text();
        let mut result = String::new();
        let mut pos = range.start;
        while pos < range.end {
            if let Some(f) = self.formulas.get(&pos) {
                let display = self.is_display(pos);
                if let Some(latex) = &f.latex {
                    if display {
                        result.push_str(&format!("$$\n{latex}\n$$"));
                    } else {
                        result.push_str(&format!("${latex}$"));
                    }
                    if let Some(prompt) = &f.prompt {
                        result.push_str(&format!(
                            "<!--mathstral:{}-->",
                            URL_SAFE_NO_PAD.encode(prompt)
                        ));
                    }
                } else {
                    result.push_str(&f.label());
                }
                pos += ATOM.len();
            } else {
                let c = text[pos..].chars().next().unwrap();
                // Explicit cancellation survives a save/reopen without activating math.
                if c == '/' && self.cancelled.contains(&pos) {
                    result.push_str("<!--mathstral:literal-->");
                }
                result.push(c);
                pos += c.len_utf8();
            }
        }
        result
    }
    pub fn is_display(&self, pos: usize) -> bool {
        let text = self.text();
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
            if range.end <= edit.start {
                edit.start = edit.start.wrapping_add_signed(delta);
                edit.end = edit.end.wrapping_add_signed(delta);
            } else if range.start > edit.start && range.end < edit.end {
                edit.end = edit.end.wrapping_add_signed(delta);
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
        let range = self.selection();
        if range.is_empty() && value.is_empty() {
            return;
        }
        let now = Instant::now();
        let structural = value.contains('/') || value.contains('\n');
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
        // A slash at EOF closes immediately, but a subsequently typed denominator
        // reinterprets it as an internal slash, even if rendering already finished.
        if range.is_empty() && value.chars().next().is_some_and(|c| !closing_boundary(c)) {
            if let Some((&pos, f)) = self.formulas.range(..self.caret).next_back() {
                if pos + ATOM.len() == self.caret && f.provisional {
                    let label = f.label();
                    self.splice(pos..pos + ATOM.len(), &label);
                    self.caret = pos + label.len();
                    self.anchor = self.caret;
                }
            }
        }
        for f in self.formulas.values_mut() {
            f.provisional = false;
        }
        let mut range = self.selection();
        let text_before = self.text();
        let completes_edit = value == "/"
            && self.editing.as_ref().is_some_and(|e| {
                self.caret == e.end - 1
                    || text_before[self.caret..]
                        .chars()
                        .next()
                        .is_none_or(closing_boundary)
            });
        let original = if completes_edit {
            if let Some(edit) = self.editing.take() {
                if self.caret == edit.end - 1 && range.is_empty() {
                    range.end += 1;
                }
                Some(edit.original)
            } else {
                None
            }
        } else {
            None
        };
        self.splice(range.clone(), value);
        self.caret = range.start + value.len();
        self.anchor = self.caret;
        self.close_prompts();
        if let Some(old) = original {
            for f in self.formulas.values_mut() {
                if f.prompt == old.prompt && f.image.is_none() {
                    f.latex = old.latex.clone();
                    f.image = old.image.clone();
                }
            }
        }
        self.typing = if typing && !structural {
            Some(now)
        } else {
            None
        };
    }
    pub fn paste(&mut self, text: &str) {
        self.record();
        let imported = Document::new(text);
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
        for f in self.formulas.values_mut() {
            f.provisional = false;
        }
    }
    fn close_prompts(&mut self) {
        loop {
            let text = self.text();
            let mut open = None;
            let mut found = None;
            for (i, c) in text.char_indices() {
                if c == '\u{fffc}' {
                    open = None;
                    continue;
                }
                if c != '/' {
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
            let mut f = Formula::new(Some(prompt), None);
            f.provisional = end == self.caret;
            self.splice(start..end, ATOM);
            self.formulas.insert(start, f);
        }
    }
    pub fn open_range(&self) -> Option<Range<usize>> {
        if let Some(e) = &self.editing {
            return Some(e.start..e.end);
        }
        let text = self.text();
        let mut open = None;
        for (i, c) in text.char_indices() {
            if c == '\u{fffc}' {
                open = None;
            }
            if c == '/'
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
            self.cancelled.insert(range.start);
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
        self.caret = pos + label.len() - 1;
        self.anchor = self.caret;
        self.editing = Some(Editing {
            start: pos,
            end: pos + label.len(),
            original: f,
        });
        true
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
            .filter(|f| f.image.is_none() && !f.failed)
            .map(|f| Job {
                id: f.id,
                prompt: f.prompt.clone(),
                latex: f.latex.clone(),
                describe: false,
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
                Err(_) => f.failed = true,
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
    pub fn describe(&mut self, id: u64, prompt: String) -> Option<usize> {
        let (&pos, f) = self.formulas.iter_mut().find(|(_, f)| f.id == id)?;
        f.prompt = Some(prompt);
        Some(pos)
    }
    pub fn move_to(&mut self, position: usize, extend: bool) {
        self.caret = position.min(self.rope.len_bytes());
        if let Some((&start, _)) = self.formulas.range(..self.caret).next_back() {
            if self.caret < start + ATOM.len() {
                self.caret = start;
            }
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
            self.rope = p.rope;
            self.caret = p.caret;
            self.anchor = p.anchor;
            self.formulas = p.formulas;
            self.cancelled = p.cancelled;
            self.editing = p.editing;
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
fn opening_boundary(text: &str, i: usize) -> bool {
    text[..i]
        .chars()
        .next_back()
        .is_none_or(|c| c.is_whitespace() || "([{\"'“‘".contains(c))
}
fn closing_boundary(c: char) -> bool {
    c.is_whitespace() || ".,;:!?)]}(['\"“”‘’".contains(c)
}

fn decode(source: &str) -> (String, BTreeMap<usize, Formula>, BTreeSet<usize>) {
    let mut text = String::new();
    let mut formulas = BTreeMap::new();
    let mut cancelled = BTreeSet::new();
    let mut i = 0;
    let literal = "<!--mathstral:literal-->";
    while i < source.len() {
        if source[i..].starts_with(literal) {
            i += literal.len();
            cancelled.insert(text.len());
            continue;
        }
        if source.as_bytes()[i] == b'$' && (i == 0 || source.as_bytes()[i - 1] != b'\\') {
            let display = source[i..].starts_with("$$");
            let delimiter = if display { "$$" } else { "$" };
            let start = i + delimiter.len();
            let end = source[start..]
                .match_indices(delimiter)
                .find(|(j, _)| *j == 0 || source.as_bytes()[start + j - 1] != b'\\')
                .map(|(j, _)| start + j);
            if let Some(end) =
                end.filter(|&end| end > start && (display || !source[start..end].contains('\n')))
            {
                let latex = source[start..end].trim().to_owned();
                i = end + delimiter.len();
                let mut prompt = None;
                let prefix = "<!--mathstral:";
                if source[i..].starts_with(prefix) {
                    if let Some(end) = source[i + prefix.len()..].find("-->") {
                        let encoded = &source[i + prefix.len()..i + prefix.len() + end];
                        if let Ok(bytes) = URL_SAFE_NO_PAD.decode(encoded) {
                            if let Ok(p) = String::from_utf8(bytes) {
                                prompt = Some(p);
                                i += prefix.len() + end + 3;
                            }
                        }
                    }
                }
                formulas.insert(text.len(), Formula::new(prompt, Some(latex)));
                text.push_str(ATOM);
                continue;
            }
        }
        let c = source[i..].chars().next().unwrap();
        text.push(c);
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
    fn slash_rules_and_provisional_fraction() {
        let mut d = Document::new("");
        type_text(&mut d, "and/or https://a/b /1/2 plus a/b/ done");
        assert_eq!(d.formulas.len(), 1);
        assert_eq!(
            d.formulas.values().next().unwrap().prompt.as_deref(),
            Some("1/2 plus a/b")
        );
        assert_eq!(d.text(), format!("and/or https://a/b {ATOM} done"));
        type_text(&mut d, " /unfinished");
        assert!(d.open_range().is_some());
        assert!(d.cancel());
        type_text(&mut d, "/");
        assert_eq!(d.formulas.len(), 1);
        assert_eq!(Document::new(&d.markdown()).formulas.len(), 1);
    }
    #[test]
    fn atoms_survive_edits_and_history_and_roundtrip() {
        let mut d = Document::new("A /one/ and /two/.");
        let id = d.formulas[&2].id;
        d.move_to(0, false);
        d.replace("prefix ", true);
        assert_eq!(d.formulas[&9].id, id);
        d.move_to(12, false);
        assert!(d.reopen(9));
        d.replace("!", true);
        d.replace("/", true);
        assert_eq!(d.formulas[&9].prompt.as_deref(), Some("one!"));
        d.undo(false);
        assert!(d.text().contains("/one!/"));
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
}
