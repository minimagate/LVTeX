//! Text codec: the Markdown-compatible file format, pasted text and formula labels.
use super::{ATOM, ATOM_CHAR, Document, Formula};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};

pub(super) const LITERAL_BACKSLASH: &str = "<!--mathstral:literal-->";
const PENDING_PREFIX: &str = "<!--mathstral:pending:";

impl Document {
    pub fn markdown(&self) -> String {
        self.encode(0..self.rope.len_bytes(), true)
    }
    pub(super) fn encode(&self, range: Range<usize>, escape: bool) -> String {
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
            if let Some(edit) = editing
                && pos == edit.start
            {
                push_pending(&mut result, &text[edit.start..edit.end]);
                pos = edit.end;
                continue;
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
                        push_pending(&mut result, prompt);
                    } else {
                        let label = f.label();
                        result.push_str(&if escape { escape_label(&label) } else { label });
                    }
                }
                pos += ATOM.len();
            } else {
                let c = text[pos..].chars().next().unwrap();
                let literal = c == '\\' && self.cancelled.contains(&pos);
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
        result
    }
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

// A pending prompt is a comment, so its text is never read as prose.
fn push_pending(out: &mut String, prompt: &str) {
    out.push_str(&format!(
        "{PENDING_PREFIX}{}-->",
        URL_SAFE_NO_PAD.encode(prompt)
    ));
}

fn push_literal_marker(out: &mut String) {
    out.push_str(LITERAL_BACKSLASH);
    out.push_str(LITERAL_BACKSLASH);
}

pub(super) fn unescape_literal_markers(source: &str) -> String {
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
pub(super) fn decode(
    source: &str,
    unescape: bool,
) -> (String, BTreeMap<usize, Formula>, BTreeSet<usize>) {
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
        // Malformed payloads fall through and stay plain text.
        if let Some(rest) = source[i..].strip_prefix(PENDING_PREFIX)
            && let Some(end) = rest.find("-->")
            && let Ok(bytes) = URL_SAFE_NO_PAD.decode(&rest[..end])
            && let Ok(prompt) = String::from_utf8(bytes)
            && !prompt.is_empty()
        {
            formulas.insert(text.len(), Formula::new(Some(prompt), None));
            text.push_str(ATOM);
            i += PENDING_PREFIX.len() + end + "-->".len();
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
