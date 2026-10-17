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
mod tests {
    use super::*;
    use mathstral_render::{
        fixtures::FORMULAS,
        math::{Renderer, SCALE},
    };
    use mitex::{ArgPattern, ArgShape, CommandSpecItem};

    #[test]
    fn all_latex_cases_render_without_clipping() {
        let mut renderer = Renderer::with_prelude(PRELUDE);
        for formula in FORMULAS {
            let body = convert(formula.latex).unwrap();
            for display in [false, true] {
                let image = renderer
                    .render_math(&body, display, SCALE)
                    .unwrap_or_else(|e| panic!("{}: {e}\n{body}", formula.name));
                let w = image.width() as usize;
                let h = image.height() as usize;
                for (index, pixel) in image.pixels().iter().enumerate() {
                    let (x, y) = (index % w, index / w);
                    if x < 2 || y < 2 || x >= w - 2 || y >= h - 2 {
                        assert_eq!(
                            pixel.alpha(),
                            0,
                            "clipped {} display={display}",
                            formula.name
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn nesting_limit_is_64_braces() {
        let nested = |n: usize| format!("{}x{}", "{".repeat(n), "}".repeat(n));
        assert!(convert(&nested(64)).is_ok());
        assert_eq!(convert(&nested(65)).unwrap_err(), TOO_DEEP);
    }
    #[test]
    fn errors_and_helper_variations() {
        let mut renderer = Renderer::with_prelude(PRELUDE);
        for latex in [r"\notARealCommand{x}", r"\frac{", r"\begin{pmatrix}a&b"] {
            assert!(
                convert(latex)
                    .and_then(|s| renderer.render_math(&s, true, SCALE))
                    .is_err(),
                "accepted {latex}"
            );
        }
        for latex in [
            r"\sqrt[3]{x}",
            r"\text{diff angle.l}",
            r"\angle ABC + \partial_x f",
            r"\begin{vmatrix}a&b\\c&d\end{vmatrix}",
            r"\text{\{x\}} + \alpha",
            "x % ignored } \\end{matrix}\n + y",
        ] {
            let body = convert(latex).unwrap();
            renderer
                .render_math(&body, true, SCALE)
                .unwrap_or_else(|e| panic!("{latex}: {e}\n{body}"));
        }
    }

    #[test]
    fn typst_string_literal_escapes_quotes_and_backslashes() {
        assert_eq!(typst_string_literal(r#"a"b\c"#), r#""a\"b\\c""#);
        assert_eq!(typst_string_literal("x\ny"), r#""x\ny""#);
    }

    #[test]
    fn mitex_helpers_render() {
        let mut renderer = Renderer::with_prelude(PRELUDE);
        for latex in [
            r"\begin{cases}x\equiv a \pmod{32}\\x\equiv 1 \pmod{35}\end{cases}",
            r"a \pod{n}",
            r"\dbinom{n}{k}",
            r"\tfrac{1}{2}",
            r"\dfrac{a}{b}",
            r"\underbrace{a+b}_{n}",
            r"\overset{?}{=}",
            r"\xrightarrow{f}",
            r"\boxed{x}",
            r"\operatorname{rank}A",
            r"\begin{array}{cc}1&2\end{array}",
            r"\sum_{\substack{i\\j}}",
            r"\stackrel{a}{=}",
            r"x\phantom{x}y",
            r"\displaystyle x",
            r"\color{red}x",
            r"\mathbin{\star}",
            r"\big( x \big)",
        ] {
            let body = convert(latex).unwrap();
            for display in [false, true] {
                renderer
                    .render_math(&body, display, SCALE)
                    .unwrap_or_else(|e| panic!("{latex} display={display}: {e}\n{body}"));
            }
        }
    }

    /// Spec items that cannot render in math mode with the generic or in-context samples, with the reason.
    const SPEC_ALLOWLIST: &[(&str, &str)] = &[
        (
            "@ifstar",
            "TeX or LaTeX primitive or ignored token: nothing to draw",
        ),
        (
            "AtBeginDocument",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "AtEndDocument",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "AtEndOfClass",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "AtEndOfPackage",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "CheckCommand",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "CheckCommand*",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "CurrentOption",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "DeclareOption",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "DeclareOption*",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "DeclareRobustCommand",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "DeclareRobustCommand*",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "DeclareTextCommand",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "DeclareTextCommandDefault",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "ExecuteOptions",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        ("Huge", "size declaration: nothing to draw on its own"),
        (
            "IfFileExists",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "InputIfFileExists",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        ("LARGE", "size declaration: nothing to draw on its own"),
        ("Large", "size declaration: nothing to draw on its own"),
        (
            "LoadClass",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "LoadClassWithOptions",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "PassOptionsToClass",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "PassOptionsToPackage",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "ProcessOptions",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "ProvideTextCommand",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "ProvideTextCommandDefault",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "ProvidesClass",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "ProvidesFile",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "RequirePackage",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "RequirePackageWithOptions",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "begingroup",
            "TeX or LaTeX primitive or ignored token: nothing to draw",
        ),
        (
            "bottomrule",
            "text-mode only (table rule, list item, file access or length macro)",
        ),
        (
            "caption",
            "label, numbering or float/table command the converter drops in math",
        ),
        (
            "centering",
            "label, numbering or float/table command the converter drops in math",
        ),
        (
            "cite",
            "needs a label or bibliography defined in the document",
        ),
        (
            "cr",
            "TeX or LaTeX primitive or ignored token: nothing to draw",
        ),
        (
            "def",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "documentclass",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "eqref",
            "needs a label or bibliography defined in the document",
        ),
        (
            "expandafter",
            "TeX or LaTeX primitive or ignored token: nothing to draw",
        ),
        (
            "footnotesize",
            "size declaration: nothing to draw on its own",
        ),
        (
            "hline",
            "label, numbering or float/table command the converter drops in math",
        ),
        ("huge", "size declaration: nothing to draw on its own"),
        (
            "if",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "ifdim",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "iffalse",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "ifhbox",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "ifhmode",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "ifinner",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "ifmmode",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "ifnum",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "ifodd",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "iftrue",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "ifvbox",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "ifvmode",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "ifvoid",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "ifx",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "ignorespaces",
            "TeX or LaTeX primitive or ignored token: nothing to draw",
        ),
        (
            "ignorespacesafterend",
            "TeX or LaTeX primitive or ignored token: nothing to draw",
        ),
        (
            "includegraphics",
            "text-mode only (table rule, list item, file access or length macro)",
        ),
        (
            "item",
            "text-mode only (table rule, list item, file access or length macro)",
        ),
        (
            "label",
            "label, numbering or float/table command the converter drops in math",
        ),
        ("large", "size declaration: nothing to draw on its own"),
        (
            "midrule",
            "text-mode only (table rule, list item, file access or length macro)",
        ),
        (
            "newcommand",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "newcommand*",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "newcounter",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "newenvironment",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "newenvironment*",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "newfont",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "newlength",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "newline",
            "TeX or LaTeX primitive or ignored token: nothing to draw",
        ),
        (
            "newsavebox",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "newtheorem",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "noexpand",
            "TeX or LaTeX primitive or ignored token: nothing to draw",
        ),
        (
            "nonumber",
            "label, numbering or float/table command the converter drops in math",
        ),
        ("normalsize", "size declaration: nothing to draw on its own"),
        (
            "notag",
            "label, numbering or float/table command the converter drops in math",
        ),
        (
            "providecommand",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "providecommand*",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "ref",
            "needs a label or bibliography defined in the document",
        ),
        (
            "relax",
            "TeX or LaTeX primitive or ignored token: nothing to draw",
        ),
        (
            "renewcommand",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "renewcommand*",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "renewenvironment",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        (
            "renewenvironment*",
            "preamble, macro-definition or conditional command: converter emits nothing to draw",
        ),
        ("scriptsize", "size declaration: nothing to draw on its own"),
        ("small", "size declaration: nothing to draw on its own"),
        (
            "tabular",
            "text-mode environment; converter output is malformed inside math",
        ),
        (
            "tag",
            "label, numbering or float/table command the converter drops in math",
        ),
        (
            "textwidth",
            "text-mode only (table rule, list item, file access or length macro)",
        ),
        ("tiny", "size declaration: nothing to draw on its own"),
        (
            "toprule",
            "text-mode only (table rule, list item, file access or length macro)",
        ),
        (
            "vline",
            "label, numbering or float/table command the converter drops in math",
        ),
    ];

    /// Builds a minimal LaTeX sample for a spec item from its argument shape.
    fn spec_sample(name: &str, item: &CommandSpecItem) -> String {
        let terms = |n: usize| "{x}".repeat(n);
        match item {
            CommandSpecItem::Cmd(cmd) => match &cmd.args {
                ArgShape::Right { pattern } => match pattern {
                    ArgPattern::None => format!(r"\{name}"),
                    ArgPattern::FixedLenTerm { len } => format!(r"\{name}{}", terms(*len as usize)),
                    ArgPattern::RangeLenTerm { min, .. } => {
                        format!(r"\{name}{}", terms(*min as usize))
                    }
                    ArgPattern::Greedy => format!(r"\{name} x"),
                    ArgPattern::Glob { .. } => format!(r"\{name}{{x}}"),
                },
                ArgShape::Left1 => format!(r"x \{name}"),
                ArgShape::InfixGreedy => format!(r"a \{name} b"),
            },
            CommandSpecItem::Env(env) => {
                let count = match &env.args {
                    ArgPattern::FixedLenTerm { len } => *len as usize,
                    ArgPattern::RangeLenTerm { min, .. } => *min as usize,
                    _ => 0,
                };
                format!(r"\begin{{{name}}}{}x\end{{{name}}}", "{c}".repeat(count))
            }
        }
    }

    /// Spacing and phantom commands, which draw no ink on their own. They are
    /// also tried between text. Commands that only matter in preambles or
    /// labels are not, so they stay allowlisted.
    const IN_CONTEXT: &[&str] = &[
        "",
        "!",
        "*",
        ",",
        ":",
        ";",
        ">",
        "Box",
        "enspace",
        "hphantom",
        "hskip",
        "kern",
        "mathstrut",
        "medspace",
        "mkern",
        "mskip",
        "negmedspace",
        "negthickspace",
        "negthinmedspace",
        "negthinspace",
        "nobreak",
        "nobreakspace",
        "phantom",
        "qquad",
        "quad",
        "space",
        "thickspace",
        "thinspace",
        "vphantom",
    ];

    /// Samples tried for a spec item, in order. The in-context and explicit
    /// samples cover the spacing commands and the color and length commands
    /// that the generic sample cannot supply.
    fn spec_samples(name: &str, item: &CommandSpecItem) -> Vec<String> {
        let mut samples = vec![spec_sample(name, item)];
        if let CommandSpecItem::Cmd(cmd) = item
            && let ArgShape::Right { pattern } = &cmd.args
            && IN_CONTEXT.contains(&name)
        {
            // Only fixed-length shapes: glob and greedy samples would
            // swallow the trailing text, and the converter panics on some.
            let dims = match pattern {
                ArgPattern::None => Some(0),
                ArgPattern::FixedLenTerm { len } => Some(*len as usize),
                ArgPattern::RangeLenTerm { min, .. } => Some(*min as usize),
                ArgPattern::Greedy | ArgPattern::Glob { .. } => None,
            };
            if let Some(n) = dims {
                samples.push(if n == 0 {
                    format!(r"x \{name} y")
                } else {
                    format!(r"x\{name}{}y", "{1em}".repeat(n))
                });
            }
        }
        if let Some(explicit) = explicit_sample(name) {
            samples.push(explicit.to_string());
        }
        samples
    }

    /// Hand-written samples for commands whose arguments must be a color or length.
    fn explicit_sample(name: &str) -> Option<&'static str> {
        Some(match name {
            "color" => r"\color{red}x",
            "textcolor" => r"\textcolor{red}{x}",
            "colorbox" => r"\colorbox{red}{x}",
            "hspace" => r"x\hspace{1em}y",
            "vspace" => r"x\vspace{1em}y",
            "raisebox" => r"x\raisebox{1pt}{x}y",
            _ => return None,
        })
    }

    #[test]
    fn every_spec_command_renders_or_is_allowlisted() {
        let mut renderer = Renderer::with_prelude(PRELUDE);
        let mut failures = Vec::new();
        let mut total = 0;
        for (name, item) in mitex_spec_gen::DEFAULT_SPEC.items() {
            total += 1;
            // An item passes when any of its samples renders.
            let mut last = None;
            let mut passed = false;
            for sample in spec_samples(name, item) {
                let result = convert(&sample)
                    .and_then(|body| renderer.render_math(&body, true, SCALE).map(|_| ()));
                match result {
                    Ok(()) => {
                        passed = true;
                        break;
                    }
                    Err(error) => last = Some((sample, error)),
                }
            }
            if !passed {
                let (sample, error) = last.expect("at least one sample");
                failures.push((name.to_string(), sample, error));
            }
        }
        let failing: Vec<&str> = failures.iter().map(|(n, _, _)| n.as_str()).collect();
        let allowed: Vec<&str> = SPEC_ALLOWLIST.iter().map(|(n, _)| *n).collect();
        let mut unexpected: Vec<_> = failures
            .iter()
            .filter(|(n, _, _)| !allowed.contains(&n.as_str()))
            .collect();
        unexpected.sort_by(|a, b| a.0.cmp(&b.0));
        assert!(
            unexpected.is_empty(),
            "{} of {total} spec items fail without allowlist entries:\n{}",
            unexpected.len(),
            unexpected
                .iter()
                .map(|(n, s, e)| format!("  {n}: {s} => {e}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        let mut stale: Vec<&str> = allowed
            .iter()
            .copied()
            .filter(|n| !failing.contains(n))
            .collect();
        stale.sort_unstable();
        assert!(stale.is_empty(), "allowlisted but now passing: {stale:?}");
    }

    #[test]
    fn substitute_symbols_replaces_whole_control_words() {
        assert_eq!(substitute_symbols(r"a \oplus b"), "a ⊕ b");
        assert_eq!(substitute_symbols(r"\oplus_1"), "⊕_1");
        assert_eq!(substitute_symbols(r"\oplusx"), r"\oplusx");
        assert_eq!(substitute_symbols(r"\\oplus"), r"\\oplus");
        assert_eq!(substitute_symbols(r"\\\oplus"), "\\\\⊕");
        assert_eq!(substitute_symbols(r"\bigdot"), "\u{2A00}");
        assert_eq!(substitute_symbols(r"% \oplus"), r"% \oplus");
        assert_eq!(substitute_symbols(r"\%\oplus"), "\\%⊕");
        assert!(matches!(substitute_symbols(r"\alpha"), Cow::Borrowed(_)));
        assert_eq!(substitute_symbols(r"\preceqq\cdots\preceqq"), "⪳\\cdots⪳");
        assert_eq!(substitute_symbols(r"\not\gtrapprox"), r"\not⪆");
    }

    #[test]
    fn substituted_symbols_convert_and_render() {
        let mut renderer = Renderer::with_prelude(PRELUDE);
        for latex in [
            r"\iiiint",
            r"\preceqq\cdots\preceqq",
            r"\not\succeqq",
            r"\Prec\cdots\Prec",
            r"\leftsquigarrow\cdots\leftsquigarrow",
            r"\dotminus\dottimes",
            r"\iddots",
        ] {
            let body = convert(latex).unwrap_or_else(|e| panic!("{latex}: {e}"));
            renderer
                .render_math(&body, false, SCALE)
                .unwrap_or_else(|e| panic!("{latex}: {e}\n{body}"));
        }
    }
}
