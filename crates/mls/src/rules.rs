//! Rules that reject a formalization before Lean checks it.

use crate::{Formal, Label};

/// Removes `--` line comments (keeping their newline) and nested `/- -/` block
/// comments, including doc comments. A block comment leaves one space, so the
/// tokens on either side stay apart. String and char literals are copied as they
/// are, so a `--` or `/-` inside one is text, as it is for Lean.
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
        } else if c == '"' {
            let len = string_len(rest);
            out.push_str(&rest[..len]);
            rest = &rest[len..];
        } else if let Some(len) = char_literal_len(rest, out.chars().next_back()) {
            out.push_str(&rest[..len]);
            rest = &rest[len..];
        } else {
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

/// The byte length of the string literal at the start of `text`, which begins with `"`. An
/// unescaped `"` ends it and a backslash escapes the next byte. A string left open runs to the
/// end, as it does in Lean.
fn string_len(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// The byte length of the char literal at the start of `text`, such as `'a'`, `'"'` or `'\n'`.
/// A `'` right after an identifier character is part of a name (`f'`), not a literal.
fn char_literal_len(text: &str, before: Option<char>) -> Option<usize> {
    let body = text.strip_prefix('\'')?;
    if before.is_some_and(is_ident_char) {
        return None;
    }
    let mut chars = body.chars();
    let first = chars.next()?;
    let width = if first == '\\' {
        first.len_utf8() + chars.next()?.len_utf8()
    } else {
        first.len_utf8()
    };
    body[width..].starts_with('\'').then_some(width + 2)
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '.' | '\'' | '!' | '?')
}

/// Words whose code runs or changes limits while Lean elaborates a block. The REPL only
/// elaborates. Meta-programming (`run_cmd`, `elab`, `macro`, `syntax`) and `initialize` run
/// code. `set_option` changes the limits that bound elaboration. `unsafe`, `implemented_by`
/// and `extern` bypass the kernel's checks. `native_decide` trusts the compiler, and `import`
/// loads other code.
const FORBIDDEN_WORDS: &[&str] = &[
    "run_cmd",
    "run_elab",
    "run_meta",
    "run_tac",
    "elab",
    "elab_rules",
    "macro",
    "macro_rules",
    "syntax",
    "declare_syntax_cat",
    "initialize",
    "builtin_initialize",
    "set_option",
    "native_decide",
    "unsafe",
    "implemented_by",
    "extern",
    "import",
];

/// The first `#` command in `code`, such as `#eval`: a `#` followed by a letter. Such commands
/// print, run or exit while Lean elaborates.
fn hash_command(code: &str) -> Option<&str> {
    code.match_indices('#').find_map(|(start, _)| {
        let rest = &code[start + 1..];
        rest.starts_with(char::is_alphabetic).then(|| {
            let len = rest.find(|c: char| !is_ident_char(c)).unwrap_or(rest.len());
            &code[start..start + 1 + len]
        })
    })
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
    // After the sorry rules, so a statement that breaks them keeps its own message.
    if let Some(command) = hash_command(&code) {
        return Err(format!("The Lean uses `{command}`, which is not allowed."));
    }
    if let Some(word) = FORBIDDEN_WORDS.iter().find(|word| has_word(&code, word)) {
        return Err(format!("The Lean uses `{word}`, which is not allowed."));
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
