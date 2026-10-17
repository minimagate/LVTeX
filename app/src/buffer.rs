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
        self.prompt
            .as_ref()
            .map(|p| format!("\\{p}\\"))
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
struct Manual {
    id: u64,
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
        let mut pos = range.start;
        while pos < range.end {
            if let Some(f) = self.formulas.get(&pos) {
                let display = Self::is_display_in(&text, pos);
                if let Some(latex) = &f.latex {
                    // An empty formula still being written has no source; `$$` would not reopen.
                    if !latex.is_empty() {
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
                    }
                } else {
                    let label = f.label();
                    result.push_str(&if escape { escape_label(&label) } else { label });
                }
                pos += ATOM.len();
            } else {
                let c = text[pos..].chars().next().unwrap();
                // Explicit cancellation survives a save/reopen without activating math.
                if c == '\\' && self.cancelled.contains(&pos) {
                    result.push_str("<!--mathstral:literal-->");
                }
                let next = pos + c.len_utf8();
                if c == '$' && escape {
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
        let mut range = self.selection();
        let text_before = self.text();
        let completes_edit = value == "\\"
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
        self.manual = Some(Manual { id, original: None });
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
    pub fn set_manual(&mut self, latex: String, image: Option<Arc<FormulaImage>>) {
        let Some(id) = self.manual_id() else {
            return;
        };
        if let Some(f) = self.formulas.values_mut().find(|f| f.id == id) {
            f.latex = Some(latex);
            f.image = image;
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
            let formula = Formula {
                image: None,
                failed: false,
                ..formula
            };
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
            .filter(|f| f.image.is_none() && !f.failed && self.manual_id() != Some(f.id))
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

// Files use `unescape`; pasted text is taken literally, as before escapes existed.
fn decode(source: &str, unescape: bool) -> (String, BTreeMap<usize, Formula>, BTreeSet<usize>) {
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
        if source.as_bytes()[i] == b'$' && (unescape || i == 0 || source.as_bytes()[i - 1] != b'\\')
        {
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
        d.replace(r"\", true);
        assert_eq!(d.formulas[&9].prompt.as_deref(), Some("one!"));
        d.undo(false);
        assert!(d.text().contains(r"\one!\"));
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
}
