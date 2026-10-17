//! Parses LaTeX into a math tree, keeping input it cannot read as raw nodes.

use super::tree::{MathTree, Node, Row};

const RIGHT: &str = r"\right";
const MAX_PARSE_DEPTH: usize = 64;

impl MathTree {
    /// Parses LaTeX. Unknown or unbalanced input is kept as `Raw` nodes or closed implicitly.
    /// Inputs nested beyond `MAX_PARSE_DEPTH` are retained as one raw node.
    pub fn parse(latex: &str) -> Self {
        let mut parser = Parser {
            src: latex,
            at: 0,
            depth: 0,
            exceeded: false,
        };
        let root = parser.row(Close::Eof);
        if parser.exceeded {
            return Self {
                root: vec![Node::Raw(latex.to_owned())],
                path: Vec::new(),
                pos: 1,
            };
        }
        let pos = root.len();
        Self {
            root,
            path: Vec::new(),
            pos,
        }
    }
}

#[derive(Clone, Copy)]
enum Close {
    Eof,
    Brace,
    Bracket,
    Right,
}

/// `\right` followed by a non-letter, so that `\rightarrow` is not mistaken for it.
fn starts_with_right(rest: &str) -> bool {
    rest.strip_prefix(RIGHT)
        .is_some_and(|tail| !tail.starts_with(|c: char| c.is_ascii_alphabetic()))
}

struct Parser<'a> {
    src: &'a str,
    at: usize,
    depth: usize,
    exceeded: bool,
}

impl<'a> Parser<'a> {
    fn rest(&self) -> &'a str {
        let src: &'a str = self.src;
        &src[self.at..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn eat(&mut self, prefix: &str) -> bool {
        let found = self.rest().starts_with(prefix);
        if found {
            self.at += prefix.len();
        }
        found
    }

    fn skip_space(&mut self) {
        while let Some(' ' | '\t' | '\n' | '\r') = self.peek() {
            self.at += 1;
        }
    }

    /// Reads a control word or a control symbol starting at a backslash. A lone trailing
    /// backslash is returned as is.
    fn command(&mut self) -> &'a str {
        let src: &'a str = self.src;
        let start = self.at;
        self.at += 1;
        let letters = self
            .rest()
            .bytes()
            .take_while(u8::is_ascii_alphabetic)
            .count();
        if letters > 0 {
            self.at += letters;
        } else if let Some(c) = self.peek() {
            self.at += c.len_utf8();
        }
        &src[start..self.at]
    }

    /// Skips a balanced `{...}` group starting at the current `{`. An unclosed group runs to the end.
    fn skip_braces(&mut self) {
        let mut depth = 0usize;
        let mut chars = self.rest().char_indices();
        while let Some((i, c)) = chars.next() {
            match c {
                '\\' => {
                    chars.next();
                }
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        self.at += i + 1;
                        return;
                    }
                }
                _ => {}
            }
        }
        self.at = self.src.len();
    }

    /// Reads `{text}` and returns the text, or None when the group is missing or unclosed.
    fn group_text(&mut self) -> Option<&'a str> {
        let inner = self.rest().strip_prefix('{')?;
        let close = inner.find('}')?;
        self.at += close + 2;
        Some(&inner[..close])
    }

    fn raw_from(&self, start: usize) -> Node {
        Node::Raw(self.src[start..self.at].to_string())
    }

    /// A delimiter after `\left` or `\right`. `\{` and `\}` are stored as bare braces.
    fn delimiter(&mut self) -> Option<String> {
        self.skip_space();
        let c = self.peek()?;
        if c != '\\' {
            self.at += c.len_utf8();
            return Some(c.to_string());
        }
        Some(match self.command() {
            r"\{" => "{".to_string(),
            r"\}" => "}".to_string(),
            cmd if cmd.len() == 1 => ".".to_string(),
            cmd => cmd.to_string(),
        })
    }

    /// One argument: a braced group, or a single token. Missing arguments are empty.
    fn arg(&mut self) -> Row {
        self.skip_space();
        let rest = self.rest();
        match rest.chars().next() {
            None | Some('}' | ']') => Vec::new(),
            _ if starts_with_right(rest) => Vec::new(),
            Some('{') => {
                self.at += 1;
                self.row(Close::Brace)
            }
            Some(_) => {
                let mut row = Vec::new();
                self.item(&mut row);
                row
            }
        }
    }

    fn row(&mut self, close: Close) -> Row {
        let mut row = Vec::new();
        loop {
            if self.exceeded {
                break;
            }
            self.skip_space();
            let rest = self.rest();
            match close {
                Close::Brace if rest.starts_with('}') => {
                    self.at += 1;
                    break;
                }
                Close::Bracket if rest.starts_with(']') => {
                    self.at += 1;
                    break;
                }
                Close::Right if starts_with_right(rest) => break,
                _ if rest.is_empty() => break,
                _ => self.item(&mut row),
            }
        }
        row
    }

    /// Reads one item and pushes its node. Always consumes input.
    fn item(&mut self, row: &mut Row) {
        if self.exceeded {
            return;
        }
        if self.depth > MAX_PARSE_DEPTH {
            self.exceeded = true;
            self.at = self.src.len();
            return;
        }
        self.depth += 1;
        self.parse_item(row);
        self.depth -= 1;
    }

    fn parse_item(&mut self, row: &mut Row) {
        let Some(c) = self.peek() else {
            return;
        };
        match c {
            '%' => {
                let start = self.at;
                self.at += 1;
                while let Some(next) = self.peek() {
                    self.at += next.len_utf8();
                    if next == '\n' {
                        break;
                    }
                }
                let mut comment = self.src[start..self.at].to_string();
                if !comment.ends_with('\n') {
                    comment.push('\n');
                }
                row.push(Node::Raw(comment));
            }
            '\\' => {
                let node = self.command_node();
                row.push(node);
            }
            '{' => {
                let start = self.at;
                self.skip_braces();
                row.push(self.raw_from(start));
            }
            '_' | '^' => {
                self.at += 1;
                let body = self.arg();
                row.push(Node::Script {
                    sup: c == '^',
                    body,
                });
            }
            '\'' => {
                self.at += 1;
                row.push(Node::Prime);
            }
            _ => {
                self.at += c.len_utf8();
                row.push(Node::Symbol {
                    latex: c.to_string(),
                    keys: String::new(),
                });
            }
        }
    }

    fn command_node(&mut self) -> Node {
        let start = self.at;
        let cmd = self.command();
        match cmd {
            r"\frac" => {
                let num = self.arg();
                let den = self.arg();
                Node::Fraction { num, den }
            }
            r"\sqrt" => {
                self.skip_space();
                let index = if self.eat("[") {
                    Some(self.row(Close::Bracket))
                } else {
                    None
                };
                let body = self.arg();
                Node::Root { index, body }
            }
            r"\left" => match self.delimiter() {
                None => self.raw_from(start),
                Some(left) => {
                    let body = self.row(Close::Right);
                    let right = if starts_with_right(self.rest()) {
                        self.at += RIGHT.len();
                        self.delimiter().unwrap_or_else(|| ".".to_string())
                    } else {
                        ".".to_string()
                    };
                    Node::Delimited { left, right, body }
                }
            },
            r"\right" => {
                // A `\right` with no `\left` before it is kept verbatim.
                self.delimiter();
                self.raw_from(start)
            }
            r"\begin" => match self.group_text() {
                Some(env) => {
                    let end_tag = format!(r"\end{{{env}}}");
                    let rest = self.rest();
                    let len = rest
                        .find(end_tag.as_str())
                        .map_or(rest.len(), |i| i + end_tag.len());
                    self.at += len;
                    self.raw_from(start)
                }
                None => {
                    self.at = self.src.len();
                    self.raw_from(start)
                }
            },
            _ => {
                // Commands with a braced argument, such as `\mathbb{R}`, stay as one raw piece.
                let word = cmd.len() > 1 && cmd.as_bytes()[1].is_ascii_alphabetic();
                let save = self.at;
                self.skip_space();
                if word && self.peek() == Some('{') {
                    self.skip_braces();
                    self.raw_from(start)
                } else {
                    self.at = save;
                    Node::Symbol {
                        latex: cmd.to_string(),
                        keys: String::new(),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_retains_deep_input_raw_and_keeps_shallow_trees_editable() {
        let at_limit = format!(
            "{}x{}",
            r"\sqrt{".repeat(MAX_PARSE_DEPTH),
            "}".repeat(MAX_PARSE_DEPTH)
        );
        let parsed_at_limit = MathTree::parse(&at_limit);
        assert_eq!(parsed_at_limit.latex(), at_limit);
        assert!(matches!(
            parsed_at_limit.root.first(),
            Some(Node::Root { .. })
        ));

        let nested_braced = format!("{}x{}", r"\sqrt{".repeat(80), "}".repeat(80));
        let nested_unbraced = format!("{}x", r"\sqrt".repeat(80));
        for (name, source) in [
            ("braced", nested_braced.as_str()),
            ("unbraced", nested_unbraced.as_str()),
        ] {
            let parsed = MathTree::parse(source);
            assert_eq!(parsed.latex(), source, "{name} source changed");
            assert!(matches!(parsed.root.as_slice(), [Node::Raw(raw)] if raw == source));
        }

        let mut shallow = MathTree::parse(r"\sqrt{x^2}");
        assert!(matches!(shallow.root.first(), Some(Node::Root { .. })));
        shallow.insert_symbol("y", "y");
        assert_eq!(shallow.latex(), r"\sqrt{x^{2}}y");
    }

    #[test]
    fn standalone_groups_and_unknown_command_arguments_are_preserved() {
        let cases = [
            ("{ab}^2", "{ab}^{2}"),
            ("{{a}{b}}", "{{a}{b}}"),
            ("{}", "{}"),
            (r"\textcolor{red}{ab}", r"\textcolor{red}{ab}"),
            (r"\unknown{a}{b}{c}", r"\unknown{a}{b}{c}"),
        ];

        for (input, expected) in cases {
            let parsed = MathTree::parse(input).latex();
            assert_eq!(parsed, expected, "input {input:?}");
            assert_eq!(
                MathTree::parse(&parsed).latex(),
                expected,
                "re-parse of {input:?}"
            );
        }
    }

    #[test]
    fn malformed_latex_never_panics_and_normalises() {
        let inputs = [
            "\\",
            "\\left",
            "\\left(",
            "\\begin{x",
            "\\frac{",
            "\\sqrt[",
            "^{",
            "{{{",
            "_",
            "\\right",
            "\\operatorname",
            "é^é_",
            "\\{",
            "\\text{a",
            "a}b{",
            "\\frac}{",
            "\\left\\right",
            "\\end{x}",
            "%&#$~",
            "\u{1F600}^\u{1F600}",
        ];
        for input in inputs {
            let once = MathTree::parse(input).latex();
            assert_eq!(MathTree::parse(&once).latex(), once, "input {input:?}");
        }
    }
}
