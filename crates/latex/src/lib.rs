//! MiTeX adapter. Symbol compatibility lives in the Typst scope rather than
//! rewriting generated text (which could corrupt text inside formulas).
//!
//! The Typst prelude is the vendored MiTeX command library (`vendor/mitex`),
//! followed by the local overrides in `mitex.typ`. `convert` returns a Typst
//! snippet that evaluates the converted body in math mode with that scope.
use std::borrow::Cow;

pub const PRELUDE: &str = concat!(
    include_str!("../vendor/mitex/prelude.typ"),
    include_str!("../vendor/mitex/standard.typ"),
    include_str!("mitex.typ"),
);

/// LaTeX control words that MiTeX 0.2.4 turns into symbol paths removed or
/// deprecated in Typst 0.15, mapped to the Unicode character that is used
/// instead. Substituting the character avoids the converter's prebuilt paths.
const UNICODE_FOR_CONTROL_WORD: &[(&str, char)] = &[
    ("bigdot", '\u{2A00}'),           // ⨀ (same as bigodot)
    ("bigodot", '\u{2A00}'),          // ⨀
    ("bigoplus", '\u{2A01}'),         // ⨁
    ("bigplus", '\u{2A01}'),          // ⨁ (same as bigoplus)
    ("bigotimes", '\u{2A02}'),        // ⨂
    ("bigtimes", '\u{2A02}'),         // ⨂ (same as bigotimes)
    ("circledast", '\u{229B}'),       // ⊛
    ("circledcirc", '\u{229A}'),      // ⊚
    ("circleddash", '\u{229D}'),      // ⊝
    ("dashleftarrow", '\u{21E0}'),    // ⇠
    ("dashrightarrow", '\u{21E2}'),   // ⇢
    ("hbar", '\u{0127}'),             // ħ
    ("hslash", '\u{0127}'),           // ħ
    ("llbracket", '\u{27E6}'),        // ⟦
    ("rrbracket", '\u{27E7}'),        // ⟧
    ("odot", '\u{2299}'),             // ⊙
    ("ominus", '\u{2296}'),           // ⊖
    ("oplus", '\u{2295}'),            // ⊕
    ("otimes", '\u{2297}'),           // ⊗
    ("ntriangleleft", '\u{22EA}'),    // ⋪
    ("ntrianglelefteq", '\u{22EC}'),  // ⋬
    ("ntriangleright", '\u{22EB}'),   // ⋫
    ("ntrianglerighteq", '\u{22ED}'), // ⋭
    ("trianglelefteq", '\u{22B4}'),   // ⊴
    ("unlhd", '\u{22B4}'),            // ⊴ (same as trianglelefteq)
    ("vartriangleleft", '\u{22B2}'),  // ⊲
    ("trianglerighteq", '\u{22B5}'),  // ⊵
    ("unrhd", '\u{22B5}'),            // ⊵ (same as trianglerighteq)
    ("vartriangleright", '\u{22B3}'), // ⊳
    ("vDash", '\u{22A8}'),            // ⊨
    ("nvDash", '\u{22AD}'),           // ⊭
    ("iiiint", '\u{2A0C}'),           // ⨌
    ("gtrapprox", '\u{2A86}'),        // ⪆
    ("gtreqqless", '\u{2A8C}'),       // ⪌
    ("lvertneqq", '\u{2268}'),        // ≨
    ("gvertneqq", '\u{2269}'),        // ≩
    ("precneq", '\u{2AB1}'),          // ⪱
    ("succneq", '\u{2AB2}'),          // ⪲
    ("preceqq", '\u{2AB3}'),          // ⪳
    ("succeqq", '\u{2AB4}'),          // ⪴
    ("Prec", '\u{2ABB}'),             // ⪻
    ("Succ", '\u{2ABC}'),             // ⪼
    ("leftsquigarrow", '\u{21DC}'),   // ⇜
    ("dotminus", '\u{2238}'),         // ∸
    ("dottimes", '\u{2A30}'),         // ⨰
    ("iddots", '\u{22F0}'),           // ⋰
];

/// Replaces each whole control word in `UNICODE_FOR_CONTROL_WORD` by its
/// character. An even run of backslashes before a name is a line break, so
/// `\\oplus` is left alone, while the odd run in `\\\oplus` is a line break
/// followed by `\oplus`. Comments are copied unchanged.
/// Returns the input borrowed when nothing matches.
fn substitute_symbols(latex: &str) -> Cow<'_, str> {
    let bytes = latex.as_bytes();
    let mut out: Option<String> = None;
    let mut copied = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'\\' => {
                i += 1;
                if i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                    let word_start = i;
                    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    let word = &latex[word_start..i];
                    if let Some((_, symbol)) = UNICODE_FOR_CONTROL_WORD
                        .iter()
                        .find(|(name, _)| *name == word)
                    {
                        let out = out.get_or_insert_with(|| String::with_capacity(latex.len()));
                        out.push_str(&latex[copied..word_start - 1]);
                        out.push(*symbol);
                        copied = i;
                    }
                } else if i < bytes.len() {
                    // A control symbol consumes the next character, which may be a backslash.
                    i += latex[i..].chars().next().map_or(1, char::len_utf8);
                }
            }
            _ => i += 1,
        }
    }
    match out {
        Some(mut out) => {
            out.push_str(&latex[copied..]);
            Cow::Owned(out)
        }
        None => Cow::Borrowed(latex),
    }
}

pub fn convert(latex: &str) -> Result<String, String> {
    let latex = substitute_symbols(latex);
    validate_structure(&latex)?;
    let body = mitex::convert_math(&latex, None)?;
    Ok(format!(
        "#eval({}, mode: \"math\", scope: mitex-scope)",
        typst_string_literal(&body)
    ))
}

/// Quotes `text` as a Typst string literal.
pub fn typst_string_literal(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('"');
    for c in text.chars() {
        match c {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            c if c.is_control() => quoted.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

// Bounds recursion in MiTeX's parser; ordinary formulas stay far below these.
const MAX_BRACE_DEPTH: usize = 64;
const MAX_ENVIRONMENT_DEPTH: usize = 32;
const TOO_DEEP: &str = "expression is nested too deeply";

/// MiTeX intentionally recovers some unfinished input, including an environment
/// without an end. Reject that before displaying an apparently successful atom.
/// This is structural validation of expression bodies, not a complete TeX parser.
fn validate_structure(latex: &str) -> Result<(), String> {
    let bytes = latex.as_bytes();
    let mut i = 0;
    let mut braces = 0usize;
    let mut environments = Vec::new();
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'{' => {
                braces += 1;
                if braces > MAX_BRACE_DEPTH {
                    return Err(TOO_DEEP.into());
                }
                i += 1;
            }
            b'}' => {
                braces = braces.checked_sub(1).ok_or("unmatched closing brace")?;
                i += 1;
            }
            b'\\' => {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                    i += 1;
                }
                if start == i {
                    // A control symbol consumes the next Unicode character.
                    if i < bytes.len() {
                        i += latex[i..].chars().next().unwrap().len_utf8();
                    }
                    continue;
                }
                let command = &latex[start..i];
                if command == "begin" || command == "end" {
                    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    if bytes.get(i) != Some(&b'{') {
                        return Err("environment name must be braced".into());
                    }
                    let name_start = i + 1;
                    i = name_start;
                    while i < bytes.len() && bytes[i] != b'}' {
                        i += 1;
                    }
                    if i == bytes.len() {
                        return Err("unfinished environment name".into());
                    }
                    let name = &latex[name_start..i];
                    if command == "begin" {
                        environments.push(name);
                        if environments.len() > MAX_ENVIRONMENT_DEPTH {
                            return Err(TOO_DEEP.into());
                        }
                    } else if environments.pop() != Some(name) {
                        return Err("mismatched environment end".into());
                    }
                    i += 1;
                }
            }
            _ => {
                i += 1;
            }
        }
    }
    if braces != 0 {
        return Err("unclosed brace".into());
    }
    if !environments.is_empty() {
        return Err("unclosed environment".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/conversion.rs"]
mod tests;
