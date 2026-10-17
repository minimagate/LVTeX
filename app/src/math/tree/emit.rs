//! LaTeX serialization, including display-only caret and placeholders.
use super::{MathTree, Node, Step};
use crate::config::CARET_SENTINEL;
use std::fmt::Write as _;

const PLACEHOLDER: &str = r"\textcolor{#73716F}{\square}";

impl MathTree {
    pub(super) fn emit(&self, show: bool) -> String {
        let mut out = String::new();
        self.write_row(&self.root, &mut Vec::new(), show, true, &mut out);
        out
    }

    fn write_row(
        &self,
        row: &[Node],
        path: &mut Vec<Step>,
        show: bool,
        root: bool,
        out: &mut String,
    ) {
        let here = show && self.path == *path;
        for (i, node) in row.iter().enumerate() {
            if here && self.pos == i {
                push_caret(out);
            }
            self.write_node(node, row[..i].last(), i, path, show, out);
        }
        if here && self.pos == row.len() {
            push_caret(out);
        }
        if show && !root && row.is_empty() {
            push_text(out, PLACEHOLDER);
        }
    }

    fn write_node(
        &self,
        node: &Node,
        prev: Option<&Node>,
        i: usize,
        path: &mut Vec<Step>,
        show: bool,
        out: &mut String,
    ) {
        match node {
            Node::Symbol { latex, .. } => push_text(out, latex),
            Node::Raw(text) => push_text(out, text),
            Node::Prime => push_text(out, "'"),
            Node::Script { sup, body } => {
                // `x^{1}^{2}` is not valid LaTeX, and a script needs a base before it.
                let same_kind =
                    matches!(prev, Some(Node::Script { sup: before, .. }) if before == sup);
                if prev.is_none() || same_kind {
                    push_text(out, "{}");
                }
                push_text(out, if *sup { "^{" } else { "_{" });
                self.write_arg(body, (i, 0), path, show, out);
                push_text(out, "}");
            }
            Node::Fraction { num, den } => {
                push_text(out, r"\frac{");
                self.write_arg(num, (i, 0), path, show, out);
                push_text(out, "}{");
                self.write_arg(den, (i, 1), path, show, out);
                push_text(out, "}");
            }
            Node::Root { index, body } => {
                push_text(out, r"\sqrt");
                let body_arg = match index {
                    Some(index) => {
                        push_text(out, "[");
                        self.write_arg(index, (i, 0), path, show, out);
                        push_text(out, "]");
                        1
                    }
                    None => 0,
                };
                push_text(out, "{");
                self.write_arg(body, (i, body_arg), path, show, out);
                push_text(out, "}");
            }
            Node::Delimited { left, right, body } => {
                push_text(out, r"\left");
                push_text(out, delimiter(left));
                self.write_arg(body, (i, 0), path, show, out);
                push_text(out, r"\right");
                push_text(out, delimiter(right));
            }
        }
    }

    fn write_arg(
        &self,
        row: &[Node],
        step: Step,
        path: &mut Vec<Step>,
        show: bool,
        out: &mut String,
    ) {
        path.push(step);
        self.write_row(row, path, show, false, out);
        path.pop();
    }
}

fn delimiter(d: &str) -> &str {
    match d {
        "{" => r"\{",
        "}" => r"\}",
        _ => d,
    }
}

/// Appends LaTeX. A control word followed by a letter gets a space, so `\alpha x` does not read as
/// `\alphax`.
fn push_text(out: &mut String, text: &str) {
    let trailing_letters = out
        .bytes()
        .rev()
        .take_while(u8::is_ascii_alphabetic)
        .count();
    let word_ends_out = trailing_letters > 0
        && out.len() > trailing_letters
        && out.as_bytes()[out.len() - trailing_letters - 1] == b'\\';
    if word_ends_out && text.starts_with(|c: char| c.is_ascii_alphabetic()) {
        out.push(' ');
    }
    out.push_str(text);
}

fn push_caret(out: &mut String) {
    write!(out, r"\mathord{{\textcolor{{#{CARET_SENTINEL:06X}}}{{|}}}}")
        .expect("writing caret markup to a String cannot fail");
}
