//! Rules that reject a formalization before Lean checks it.

use crate::{Formal, Label};

/// Removes `--` line comments (keeping their newline) and nested `/- -/` block
/// comments, including doc comments. A block comment leaves one space, so the
/// tokens on either side stay apart.
fn strip_comments(lean: &str) -> String {
    let mut out = String::with_capacity(lean.len());
    let mut rest = lean;
    let mut depth = 0usize;
    while let Some(c) = rest.chars().next() {
        if depth > 0 {
            if let Some(tail) = rest.strip_prefix("/-") {
                depth += 1;
                rest = tail;
            } else if let Some(tail) = rest.strip_prefix("-/") {
                depth -= 1;
                rest = tail;
            } else {
                rest = &rest[c.len_utf8()..];
            }
        } else if rest.starts_with("--") {
            rest = &rest[rest.find('\n').unwrap_or(rest.len())..];
        } else if let Some(tail) = rest.strip_prefix("/-") {
            depth = 1;
            out.push(' ');
            rest = tail;
        } else {
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '.' | '\'' | '!' | '?')
}

/// Maximal runs of identifier characters.
fn words(code: &str) -> impl Iterator<Item = &str> {
    code.split(|c: char| !is_ident_char(c))
        .filter(|word| !word.is_empty())
}

fn has_word(code: &str, word: &str) -> bool {
    words(code).any(|w| w == word)
}

fn count_word(code: &str, word: &str) -> usize {
    words(code).filter(|w| *w == word).count()
}

/// `text` without a final whole word `word`, or `None` when it does not end with one.
fn strip_last_word<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    let rest = text.strip_suffix(word)?;
    if rest.chars().next_back().is_some_and(is_ident_char) {
        None
    } else {
        Some(rest)
    }
}

fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The statement without its `sorry` proof and `by`: the text a proof must start with.
fn header(statement: &str) -> String {
    let text = normalize(&strip_comments(statement));
    let text = strip_last_word(&text, "sorry").unwrap_or(&text).trim_end();
    strip_last_word(text, "by")
        .unwrap_or(text)
        .trim_end()
        .to_string()
}

/// The names that the Lean declares with a declaration keyword, in order of appearance.
pub(crate) fn declared_names(lean: &str) -> Vec<String> {
    const KEYWORDS: [&str; 10] = [
        "def",
        "theorem",
        "lemma",
        "abbrev",
        "instance",
        "structure",
        "class",
        "inductive",
        "opaque",
        "axiom",
    ];
    let code = strip_comments(lean);
    let mut names: Vec<String> = Vec::new();
    let mut name_next = false;
    let mut rest = code.as_str();
    while let Some(start) = rest.find(is_ident_char) {
        let run = rest[start..]
            .find(|c: char| !is_ident_char(c))
            .unwrap_or(rest.len() - start);
        let gap = &rest[..start];
        let word = &rest[start..start + run];
        rest = &rest[start + run..];
        if KEYWORDS.contains(&word) {
            name_next = true;
        } else if std::mem::take(&mut name_next)
            && gap.chars().all(char::is_whitespace)
            && !names.iter().any(|name| name == word)
        {
            names.push(word.to_string());
        }
    }
    names
}

/// Rejects a formalization of `label` that must not be checked. `names_above`
/// are the names declared by earlier blocks; `statement` is the paired
/// statement's Lean for a proof.
pub(crate) fn reject(
    label: Label,
    formal: &Formal,
    names_above: &[String],
    statement: Option<&str>,
) -> Result<(), String> {
    let code = strip_comments(&formal.lean);
    if code.trim().is_empty() {
        return Err("The formalization is empty.".to_string());
    }
    if has_word(&code, "axiom") {
        return Err("The Lean declares an axiom.".to_string());
    }
    if has_word(&code, "admit") {
        return Err("The Lean uses admit.".to_string());
    }
    let sorries = count_word(&code, "sorry");
    match label {
        Label::Statement if sorries != 1 || strip_last_word(code.trim_end(), "sorry").is_none() => {
            return Err("A statement must have `sorry` as its whole proof.".to_string());
        }
        Label::Statement => {}
        Label::Definition if sorries > 0 => {
            return Err("The definition uses sorry.".to_string());
        }
        Label::Proof | Label::Skip if sorries > 0 => {
            return Err("The proof uses sorry.".to_string());
        }
        Label::Definition | Label::Proof | Label::Skip => {}
    }
    let statement_names = statement.map(declared_names).unwrap_or_default();
    for name in declared_names(&formal.lean) {
        let restated = label == Label::Proof && statement_names.contains(&name);
        if names_above.contains(&name) && !restated {
            return Err(format!("`{name}` is already defined above."));
        }
    }
    if let (Label::Proof, Some(statement)) = (label, statement) {
        let head = header(statement);
        if head.is_empty() || !normalize(&code).starts_with(&head) {
            return Err("The proof changes the statement above.".to_string());
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/rules.rs"]
mod tests;
