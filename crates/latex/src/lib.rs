//! MiTeX adapter. Symbol compatibility lives in the Typst scope rather than
//! rewriting generated text (which could corrupt text inside formulas).
pub const PRELUDE: &str = include_str!("mitex.typ");

pub fn convert(latex: &str) -> Result<String, String> {
    validate_structure(latex)?;
    mitex::convert_math(latex, None)
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
}
