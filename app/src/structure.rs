/// A sequence of sibling nodes. Every argument of a box is one row.
pub type Row = Vec<Node>;

/// The caret's way into a box: (node index in its row, argument index of that node).
type Step = (usize, usize);

const CARET: &str = r"\textcolor{#FF0000}{|}";
const PLACEHOLDER: &str = r"\textcolor{#73716F}{\square}";
const RIGHT: &str = r"\right";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    /// One character or command. `keys` is the typed key sequence that produced it.
    Symbol {
        latex: String,
        keys: String,
    },
    /// A subscript or superscript placed after its base.
    Script {
        sup: bool,
        body: Row,
    },
    Prime,
    Fraction {
        num: Row,
        den: Row,
    },
    Root {
        index: Option<Row>,
        body: Row,
    },
    /// `.` is an invisible delimiter.
    Delimited {
        left: String,
        right: String,
        body: Row,
    },
    /// Atomic LaTeX emitted verbatim.
    Raw(String),
}

impl Node {
    fn arg_count(&self) -> usize {
        match self {
            Node::Script { .. } | Node::Delimited { .. } => 1,
            Node::Fraction { .. } => 2,
            Node::Root { index, .. } => 1 + usize::from(index.is_some()),
            Node::Symbol { .. } | Node::Prime | Node::Raw(_) => 0,
        }
    }

    fn arg(&self, k: usize) -> Option<&Row> {
        match self {
            Node::Script { body, .. } | Node::Delimited { body, .. } => (k == 0).then_some(body),
            Node::Fraction { num, den } => match k {
                0 => Some(num),
                1 => Some(den),
                _ => None,
            },
            Node::Root { index, body } => match (index, k) {
                (Some(index), 0) => Some(index),
                (Some(_), 1) | (None, 0) => Some(body),
                _ => None,
            },
            Node::Symbol { .. } | Node::Prime | Node::Raw(_) => None,
        }
    }

    fn arg_mut(&mut self, k: usize) -> Option<&mut Row> {
        match self {
            Node::Script { body, .. } | Node::Delimited { body, .. } => (k == 0).then_some(body),
            Node::Fraction { num, den } => match k {
                0 => Some(num),
                1 => Some(den),
                _ => None,
            },
            Node::Root { index, body } => match (index, k) {
                (Some(index), 0) => Some(index),
                (Some(_), 1) | (None, 0) => Some(body),
                _ => None,
            },
            Node::Symbol { .. } | Node::Prime | Node::Raw(_) => None,
        }
    }

    fn into_args(self) -> Vec<Row> {
        match self {
            Node::Script { body, .. } | Node::Delimited { body, .. } => vec![body],
            Node::Fraction { num, den } => vec![num, den],
            Node::Root { index, body } => index.into_iter().chain([body]).collect(),
            Node::Symbol { .. } | Node::Prime | Node::Raw(_) => Vec::new(),
        }
    }

    fn all_args_empty(&self) -> bool {
        (0..self.arg_count()).all(|k| self.arg(k).is_some_and(Vec::is_empty))
    }

    /// Fractions and indexed roots have a row above and below the caret's row.
    fn is_stacked(&self) -> bool {
        matches!(
            self,
            Node::Fraction { .. } | Node::Root { index: Some(_), .. }
        )
    }
}

/// A formula being typed by hand. The caret is a path of steps from the root row down to one
/// position inside the innermost row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MathTree {
    root: Row,
    path: Vec<Step>,
    pos: usize,
}

impl MathTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses LaTeX. Unknown or unbalanced input is kept as `Raw` nodes or closed implicitly,
    /// so this never fails.
    pub fn parse(latex: &str) -> Self {
        let mut parser = Parser { src: latex, at: 0 };
        let root = parser.row(Close::Eof);
        let pos = root.len();
        Self {
            root,
            path: Vec::new(),
            pos,
        }
    }

    pub fn latex(&self) -> String {
        self.emit(false)
    }

    /// The LaTeX with the caret and grey placeholders in empty arguments, for display only.
    pub fn latex_with_caret(&self) -> String {
        self.emit(true)
    }

    fn emit(&self, show: bool) -> String {
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
                push_text(out, CARET);
            }
            self.write_node(node, row[..i].last(), i, path, show, out);
        }
        if here && self.pos == row.len() {
            push_text(out, CARET);
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

    fn cur(&self) -> &Row {
        row_ref(&self.root, &self.path)
    }

    fn cur_mut(&mut self) -> &mut Row {
        row_mut(&mut self.root, &self.path)
    }

    fn parent_mut(&mut self) -> &mut Row {
        let (_, outer) = self.path.split_last().expect("caret is inside a box");
        row_mut(&mut self.root, outer)
    }

    /// The box the caret is inside: its index in the parent row, the caret's argument, and the box.
    fn enclosing(&self) -> Option<(usize, usize, &Node)> {
        let (&(i, k), outer) = self.path.split_last()?;
        Some((i, k, &row_ref(&self.root, outer)[i]))
    }

    fn enter(&mut self, i: usize, k: usize, end: bool) {
        self.path.push((i, k));
        self.pos = if end { self.cur().len() } else { 0 };
    }

    fn switch_arg(&mut self, k: usize, end: bool) {
        if let Some(step) = self.path.last_mut() {
            step.1 = k;
        }
        self.pos = if end { self.cur().len() } else { 0 };
    }

    /// Inserts `node` at the caret and leaves the caret after it.
    pub fn insert(&mut self, node: Node) {
        let i = self.pos;
        self.cur_mut().insert(i, node);
        self.pos += 1;
    }

    pub fn insert_symbol(&mut self, latex: &str, keys: &str) {
        self.insert(Node::Symbol {
            latex: latex.to_string(),
            keys: keys.to_string(),
        });
    }

    /// The keys of the symbols just before the caret, oldest first, at most `limit` of them. A
    /// node without keys ends the run.
    pub fn keys_before_caret(&self, limit: usize) -> Vec<String> {
        let mut keys: Vec<String> = self.cur()[..self.pos]
            .iter()
            .rev()
            .map_while(|node| match node {
                Node::Symbol { keys, .. } if !keys.is_empty() => Some(keys.clone()),
                _ => None,
            })
            .take(limit)
            .collect();
        keys.reverse();
        keys
    }

    /// Replaces the `count` nodes before the caret with one symbol.
    pub fn replace_before(&mut self, count: usize, latex: &str, keys: &str) {
        let Some(start) = self.pos.checked_sub(count) else {
            return;
        };
        let end = self.pos;
        let row = self.cur_mut();
        row.drain(start..end);
        row.insert(
            start,
            Node::Symbol {
                latex: latex.to_string(),
                keys: keys.to_string(),
            },
        );
        self.pos = start + 1;
    }

    /// Inserts a box at the caret and moves the caret into its first argument.
    fn open(&mut self, node: Node) {
        let i = self.pos;
        self.cur_mut().insert(i, node);
        self.enter(i, 0, false);
    }

    pub fn make_script(&mut self, sup: bool) {
        self.open(Node::Script {
            sup,
            body: Vec::new(),
        });
    }

    pub fn make_prime(&mut self) {
        self.insert(Node::Prime);
    }

    pub fn make_fraction(&mut self) {
        self.open(Node::Fraction {
            num: Vec::new(),
            den: Vec::new(),
        });
    }

    pub fn make_sqrt(&mut self) {
        self.open(Node::Root {
            index: None,
            body: Vec::new(),
        });
    }

    /// An nth root; the caret starts in the index.
    pub fn make_root(&mut self) {
        self.open(Node::Root {
            index: Some(Vec::new()),
            body: Vec::new(),
        });
    }

    pub fn make_delimited(&mut self, left: &str, right: &str) {
        self.open(Node::Delimited {
            left: left.to_string(),
            right: right.to_string(),
            body: Vec::new(),
        });
    }

    /// Moves one position left. Entering a box from its right side lands at the end of its last argument.
    pub fn left(&mut self) {
        if let Some(i) = self.pos.checked_sub(1) {
            let args = self.cur()[i].arg_count();
            if args == 0 {
                self.pos = i;
            } else {
                self.enter(i, args - 1, true);
            }
        } else if let Some(&(i, k)) = self.path.last() {
            if k > 0 {
                self.switch_arg(k - 1, true);
            } else {
                self.path.pop();
                self.pos = i;
            }
        }
    }

    /// Moves one position right. Entering a box from its left side lands at the start of its first argument.
    pub fn right(&mut self) {
        if self.pos < self.cur().len() {
            let i = self.pos;
            if self.cur()[i].arg_count() == 0 {
                self.pos += 1;
            } else {
                self.enter(i, 0, false);
            }
        } else if let Some((i, k, node)) = self.enclosing() {
            let count = node.arg_count();
            if k + 1 < count {
                self.switch_arg(k + 1, false);
            } else {
                self.path.pop();
                self.pos = i + 1;
            }
        }
    }

    /// Moves to the other argument of the innermost fraction or indexed root that has one.
    pub fn up(&mut self) {
        self.vertical(0);
    }

    pub fn down(&mut self) {
        self.vertical(1);
    }

    fn vertical(&mut self, to_arg: usize) {
        for d in (0..self.path.len()).rev() {
            let (i, k) = self.path[d];
            let stacked = row_ref(&self.root, &self.path[..d])[i].is_stacked();
            if stacked && k != to_arg {
                self.path.truncate(d + 1);
                self.path[d] = (i, to_arg);
                self.pos = if to_arg == 0 { self.cur().len() } else { 0 };
                return;
            }
        }
    }

    pub fn next_argument(&mut self) {
        let Some((_, k, node)) = self.enclosing() else {
            return;
        };
        let count = node.arg_count();
        if k + 1 < count {
            self.switch_arg(k + 1, false);
        }
    }

    pub fn previous_argument(&mut self) {
        let Some((_, k, _)) = self.enclosing() else {
            return;
        };
        if k > 0 {
            self.switch_arg(k - 1, true);
        }
    }

    /// Moves the caret just after (`forward`) or just before the innermost box.
    pub fn exit(&mut self, forward: bool) {
        if let Some(&(i, _)) = self.path.last() {
            self.path.pop();
            self.pos = if forward { i + 1 } else { i };
        }
    }

    /// Removes the leaf before the caret. Just after a box, enters the box instead. At the start
    /// of an argument, moves back, or removes the box when it is empty.
    pub fn backspace(&mut self) {
        if let Some(i) = self.pos.checked_sub(1) {
            let args = self.cur()[i].arg_count();
            if args == 0 {
                self.cur_mut().remove(i);
                self.pos = i;
            } else {
                self.enter(i, args - 1, true);
            }
            return;
        }
        let Some((i, k, node)) = self.enclosing() else {
            return;
        };
        if k > 0 {
            self.switch_arg(k - 1, true);
            return;
        }
        if matches!(node, Node::Delimited { .. }) {
            self.edit_delimiters(true);
            return;
        }
        let empty = node.all_args_empty();
        self.path.pop();
        if empty {
            self.cur_mut().remove(i);
        }
        self.pos = i;
    }

    /// Removes the leaf after the caret, or enters the box after it. Mirrors `backspace`.
    pub fn delete(&mut self) {
        if self.pos < self.cur().len() {
            let i = self.pos;
            if self.cur()[i].arg_count() == 0 {
                self.cur_mut().remove(i);
            } else {
                self.enter(i, 0, false);
            }
            return;
        }
        let Some((i, k, node)) = self.enclosing() else {
            return;
        };
        if k + 1 < node.arg_count() {
            self.switch_arg(k + 1, false);
            return;
        }
        if matches!(node, Node::Delimited { .. }) {
            self.edit_delimiters(false);
            return;
        }
        let empty = node.all_args_empty();
        self.path.pop();
        if empty {
            self.cur_mut().remove(i);
            self.pos = i;
        } else {
            self.pos = i + 1;
        }
    }

    /// At the body's edge of a Delimited box: hides one visible delimiter, or unwraps the box once
    /// both are hidden. `left_first` picks which side is hidden first.
    fn edit_delimiters(&mut self, left_first: bool) {
        let Some(&(i, _)) = self.path.last() else {
            return;
        };
        if let Node::Delimited { left, right, .. } = &mut self.parent_mut()[i] {
            let (first, second) = if left_first {
                (left, right)
            } else {
                (right, left)
            };
            let slot = if first.as_str() == "." { second } else { first };
            if slot.as_str() != "." {
                *slot = ".".to_string();
                return;
            }
        }
        self.unwrap_enclosing();
    }

    /// Removes the whole node before the caret, including a box.
    pub fn remove_box(&mut self) {
        if let Some(i) = self.pos.checked_sub(1) {
            self.cur_mut().remove(i);
            self.pos = i;
        }
    }

    /// Replaces the innermost box by the contents of its arguments. The caret keeps its place in
    /// the argument it was in.
    pub fn unwrap_enclosing(&mut self) {
        let Some(&(i, k)) = self.path.last() else {
            return;
        };
        let inner = self.pos;
        self.path.pop();
        let parent = self.cur_mut();
        let args = parent.remove(i).into_args();
        let offset: usize = args.iter().take(k).map(Vec::len).sum();
        let merged: Row = args.into_iter().flatten().collect();
        let tail = parent.split_off(i);
        parent.extend(merged);
        parent.extend(tail);
        self.pos = i + offset + inner;
    }

    /// Sets the right delimiter of the Delimited box whose body ends at the caret, and leaves the
    /// caret after the box. Returns false when the caret is not at such an end.
    pub fn set_right_delimiter(&mut self, right: &str) -> bool {
        let Some((i, _, node)) = self.enclosing() else {
            return false;
        };
        let at_end = self.pos == self.cur().len();
        if !at_end || !matches!(node, Node::Delimited { .. }) {
            return false;
        }
        if let Node::Delimited { right: slot, .. } = &mut self.parent_mut()[i] {
            *slot = right.to_string();
        }
        self.path.pop();
        self.pos = i + 1;
        true
    }
}

fn row_ref<'a>(mut row: &'a Row, path: &[Step]) -> &'a Row {
    for &(i, k) in path {
        row = row[i].arg(k).expect("caret path names an argument");
    }
    row
}

fn row_mut<'a>(mut row: &'a mut Row, path: &[Step]) -> &'a mut Row {
    for &(i, k) in path {
        let node = &mut row[i];
        row = node.arg_mut(k).expect("caret path names an argument");
    }
    row
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

    /// Reads one item and pushes its node (a `{...}` group pushes its contents). Always consumes input.
    fn item(&mut self, row: &mut Row) {
        let Some(c) = self.peek() else {
            return;
        };
        match c {
            '\\' => {
                let node = self.command_node();
                row.push(node);
            }
            '{' => {
                self.at += 1;
                let inner = self.row(Close::Brace);
                row.extend(inner);
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

    fn sym(t: &mut MathTree, s: &str) {
        t.insert_symbol(s, s);
    }

    fn build(f: impl FnOnce(&mut MathTree)) -> MathTree {
        let mut t = MathTree::new();
        f(&mut t);
        t
    }

    type Position = (Vec<Step>, usize);

    fn walk(t: &mut MathTree, forward: bool, steps: usize) -> Vec<Position> {
        let mut seen = vec![(t.path.clone(), t.pos)];
        for _ in 0..steps {
            if forward {
                t.right();
            } else {
                t.left();
            }
            seen.push((t.path.clone(), t.pos));
        }
        seen
    }

    fn braces_balance(s: &str) -> bool {
        let mut depth = 0i32;
        let mut bytes = s.bytes();
        while let Some(b) = bytes.next() {
            match b {
                b'\\' => {
                    bytes.next();
                }
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
            if depth < 0 {
                return false;
            }
        }
        depth == 0
    }

    #[test]
    fn scripts_build_and_nest() {
        let cases = [
            (
                "x^{2}",
                build(|t| {
                    sym(t, "x");
                    t.make_script(true);
                    sym(t, "2");
                }),
            ),
            (
                "a_{i}^{2}",
                build(|t| {
                    sym(t, "a");
                    t.make_script(false);
                    sym(t, "i");
                    t.exit(true);
                    t.make_script(true);
                    sym(t, "2");
                }),
            ),
            (
                "{}_{0}",
                build(|t| {
                    t.make_script(false);
                    sym(t, "0");
                }),
            ),
            (
                "x^{1}{}^{2}",
                build(|t| {
                    sym(t, "x");
                    t.make_script(true);
                    sym(t, "1");
                    t.exit(true);
                    t.make_script(true);
                    sym(t, "2");
                }),
            ),
        ];
        for (expected, tree) in cases {
            assert_eq!(tree.latex(), expected);
        }
    }

    #[test]
    fn fraction_by_make_then_down() {
        let t = build(|t| {
            sym(t, "a");
            t.make_fraction();
            sym(t, "a");
            t.down();
            sym(t, "b");
        });
        assert_eq!(t.latex(), r"a\frac{a}{b}");
        let mut t = build(|t| {
            t.make_fraction();
            sym(t, "a");
            t.down();
            sym(t, "b");
        });
        assert_eq!(t.latex(), r"\frac{a}{b}");
        t.up();
        assert_eq!((t.path.clone(), t.pos), (vec![(0, 0)], 1));
    }

    #[test]
    fn nested_fraction_in_a_script() {
        let mut t = build(|t| {
            sym(t, "x");
            t.make_script(true);
            t.make_fraction();
            sym(t, "1");
            t.down();
            sym(t, "2");
            t.exit(true);
            t.exit(true);
            sym(t, "y");
        });
        assert_eq!(t.latex(), r"x^{\frac{1}{2}}y");
        t.exit(false);
        assert_eq!((t.path.clone(), t.pos), (vec![], 3));
    }

    #[test]
    fn sqrt_and_nth_root_with_up_and_down() {
        let sqrt = build(|t| {
            t.make_sqrt();
            sym(t, "2");
        });
        assert_eq!(sqrt.latex(), r"\sqrt{2}");

        let mut root = build(|t| {
            t.make_root();
            sym(t, "3");
            t.down();
            sym(t, "x");
        });
        assert_eq!(root.latex(), r"\sqrt[3]{x}");
        root.up();
        sym(&mut root, "4");
        assert_eq!(root.latex(), r"\sqrt[34]{x}");
        root.down();
        assert_eq!((root.path.clone(), root.pos), (vec![(0, 1)], 0));
    }

    #[test]
    fn up_and_down_use_the_innermost_stacked_box() {
        let mut t = build(|t| {
            t.make_fraction();
            t.make_fraction();
            sym(t, "a");
            t.down();
            sym(t, "b");
        });
        assert_eq!(t.latex(), r"\frac{\frac{a}{b}}{}");
        t.up();
        assert_eq!((t.path.clone(), t.pos), (vec![(0, 0), (0, 0)], 1));
        t.up();
        assert_eq!((t.path.clone(), t.pos), (vec![(0, 0), (0, 0)], 1));
        t.exit(true);
        t.down();
        assert_eq!((t.path.clone(), t.pos), (vec![(0, 1)], 0));
    }

    #[test]
    fn left_and_right_visit_every_position_and_are_symmetric() {
        let mut t = build(|t| {
            t.make_fraction();
            sym(t, "a");
            t.down();
            sym(t, "b");
            t.exit(true);
            sym(t, "+");
            sym(t, "c");
        });
        for _ in 0..10 {
            t.left();
        }
        assert_eq!((t.path.clone(), t.pos), (vec![], 0));
        let forward = walk(&mut t, true, 7);
        let expected: Vec<Position> = vec![
            (vec![], 0),
            (vec![(0, 0)], 0),
            (vec![(0, 0)], 1),
            (vec![(0, 1)], 0),
            (vec![(0, 1)], 1),
            (vec![], 1),
            (vec![], 2),
            (vec![], 3),
        ];
        assert_eq!(forward, expected);
        let backward = walk(&mut t, false, 7);
        let mut reversed = expected;
        reversed.reverse();
        assert_eq!(backward, reversed);
    }

    #[test]
    fn backspace_enters_a_box_then_removes_it_when_empty() {
        let mut t = build(|t| {
            t.make_fraction();
            sym(t, "a");
            t.down();
            sym(t, "b");
        });
        t.exit(true);
        t.backspace();
        assert_eq!(t.latex(), r"\frac{a}{b}");
        assert_eq!((t.path.clone(), t.pos), (vec![(0, 1)], 1));

        let mut empty = build(|t| t.make_fraction());
        empty.backspace();
        assert!(empty.latex().is_empty());
    }

    #[test]
    fn backspace_at_an_argument_start_moves_or_keeps_the_box() {
        let mut t = build(|t| {
            t.make_fraction();
            sym(t, "a");
            t.down();
            sym(t, "b");
        });
        t.backspace();
        assert_eq!(t.latex(), r"\frac{a}{}");
        t.backspace();
        assert_eq!((t.path.clone(), t.pos), (vec![(0, 0)], 1));
        t.left();
        t.backspace();
        assert_eq!(t.latex(), r"\frac{a}{}");
        assert_eq!((t.path.clone(), t.pos), (vec![], 0));
    }

    #[test]
    fn delimiter_hides_then_unwraps_on_backspace() {
        let mut t = build(|t| {
            t.make_delimited("(", ")");
            sym(t, "x");
        });
        t.left();
        t.backspace();
        assert_eq!(t.latex(), r"\left.x\right)");
        t.backspace();
        assert_eq!(t.latex(), r"\left.x\right.");
        t.backspace();
        assert_eq!(t.latex(), "x");
        assert_eq!((t.path.clone(), t.pos), (vec![], 0));
    }

    #[test]
    fn delete_mirrors_backspace_on_delimiters() {
        let mut t = build(|t| {
            t.make_delimited("(", ")");
            sym(t, "x");
        });
        t.delete();
        assert_eq!(t.latex(), r"\left(x\right.");
        t.delete();
        assert_eq!(t.latex(), r"\left.x\right.");
        t.delete();
        assert_eq!(t.latex(), "x");
        assert_eq!((t.path.clone(), t.pos), (vec![], 1));
    }

    #[test]
    fn delete_removes_leaves_and_empty_boxes() {
        let mut t = build(|t| {
            sym(t, "a");
            t.make_fraction();
            sym(t, "1");
            t.exit(false);
            t.exit(false);
        });
        assert_eq!(t.latex(), r"a\frac{1}{}");
        t.delete();
        assert_eq!((t.path.clone(), t.pos), (vec![(1, 0)], 0));
        t.delete();
        assert_eq!(t.latex(), r"a\frac{}{}");
        t.delete();
        assert_eq!((t.path.clone(), t.pos), (vec![(1, 1)], 0));
        t.delete();
        assert_eq!(t.latex(), "a");
        assert_eq!((t.path.clone(), t.pos), (vec![], 1));
    }

    #[test]
    fn remove_box_removes_the_node_before_the_caret() {
        let mut t = build(|t| {
            sym(t, "a");
            t.make_fraction();
            sym(t, "1");
            t.exit(true);
        });
        t.remove_box();
        assert_eq!(t.latex(), "a");
        assert_eq!((t.path.clone(), t.pos), (vec![], 1));
    }

    #[test]
    fn unwrap_enclosing_keeps_contents_and_caret() {
        let mut t = build(|t| {
            t.make_fraction();
            sym(t, "a");
            t.down();
            sym(t, "b");
        });
        t.unwrap_enclosing();
        assert_eq!(t.latex(), "ab");
        assert_eq!((t.path.clone(), t.pos), (vec![], 2));
        sym(&mut t, "c");
        assert_eq!(t.latex(), "abc");
    }

    #[test]
    fn exit_moves_just_outside_the_box() {
        let mut t = build(|t| {
            t.make_fraction();
            sym(t, "a");
            t.down();
            sym(t, "b");
        });
        t.exit(false);
        assert_eq!((t.path.clone(), t.pos), (vec![], 0));
        t.exit(true);
        assert_eq!((t.path.clone(), t.pos), (vec![], 0));
    }

    #[test]
    fn set_right_delimiter_only_at_the_body_end() {
        let mut t = build(|t| {
            t.make_delimited("[", ")");
            sym(t, "x");
        });
        t.left();
        assert!(!t.set_right_delimiter("]"));
        t.right();
        assert!(t.set_right_delimiter("]"));
        assert_eq!(t.latex(), r"\left[x\right]");
        sym(&mut t, "y");
        assert_eq!(t.latex(), r"\left[x\right]y");
    }

    #[test]
    fn next_and_previous_argument_stay_inside_the_innermost_box() {
        let mut t = build(|t| {
            t.make_fraction();
            sym(t, "a");
        });
        t.previous_argument();
        assert_eq!((t.path.clone(), t.pos), (vec![(0, 0)], 1));
        t.next_argument();
        assert_eq!((t.path.clone(), t.pos), (vec![(0, 1)], 0));
        t.next_argument();
        assert_eq!((t.path.clone(), t.pos), (vec![(0, 1)], 0));
        t.previous_argument();
        assert_eq!((t.path.clone(), t.pos), (vec![(0, 0)], 1));
    }

    #[test]
    fn latex_with_caret_marks_caret_and_empty_arguments() {
        let frac = build(|t| {
            t.make_fraction();
        });
        assert_eq!(
            frac.latex_with_caret(),
            r"\frac{\textcolor{#FF0000}{|}\textcolor{#73716F}{\square}}{\textcolor{#73716F}{\square}}"
        );
        assert_eq!(frac.latex(), r"\frac{}{}");
        assert_eq!(MathTree::new().latex_with_caret(), CARET);
        let script = build(|t| {
            sym(t, "x");
            t.make_script(true);
            sym(t, "2");
        });
        assert_eq!(script.latex_with_caret(), r"x^{2\textcolor{#FF0000}{|}}");
    }

    #[test]
    fn keys_before_caret_and_replace_before_work_on_the_run() {
        let mut t = build(|t| {
            t.insert_symbol("<", "<");
            t.insert_symbol("=", "=");
        });
        assert_eq!(t.keys_before_caret(5), ["<", "="]);
        assert_eq!(t.keys_before_caret(1), ["="]);
        t.replace_before(2, r"\leqslant", "< =");
        assert_eq!(t.latex(), r"\leqslant");
        assert_eq!(t.keys_before_caret(5), ["< ="]);
        t.make_prime();
        assert!(t.keys_before_caret(5).is_empty());
    }

    #[test]
    fn built_trees_emit_expected_latex_and_round_trip() {
        let cases = [
            (
                r"\frac{a}{b}",
                build(|t| {
                    t.make_fraction();
                    sym(t, "a");
                    t.down();
                    sym(t, "b");
                }),
            ),
            (
                r"x^{\frac{1}{2}}",
                build(|t| {
                    sym(t, "x");
                    t.make_script(true);
                    t.make_fraction();
                    sym(t, "1");
                    t.down();
                    sym(t, "2");
                }),
            ),
            (
                r"\sqrt{x}",
                build(|t| {
                    t.make_sqrt();
                    sym(t, "x");
                }),
            ),
            (
                r"\sqrt[3]{x}",
                build(|t| {
                    t.make_root();
                    sym(t, "3");
                    t.down();
                    sym(t, "x");
                }),
            ),
            (r"\frac{}{}", build(|t| t.make_fraction())),
            (r"\sqrt[]{}", build(|t| t.make_root())),
            (
                r"\alpha x",
                build(|t| {
                    t.insert_symbol(r"\alpha", "a");
                    sym(t, "x");
                }),
            ),
            (
                r"\frac{\alpha}{\beta}",
                build(|t| {
                    t.make_fraction();
                    t.insert_symbol(r"\alpha", "a");
                    t.down();
                    t.insert_symbol(r"\beta", "b");
                }),
            ),
            (
                r"f'\left(x\right)",
                build(|t| {
                    sym(t, "f");
                    t.make_prime();
                    t.make_delimited("(", ")");
                    sym(t, "x");
                }),
            ),
            (
                r"\in\mathbb{R}",
                build(|t| {
                    t.insert_symbol(r"\in", "i");
                    t.insert(Node::Raw(r"\mathbb{R}".to_string()));
                }),
            ),
            (
                r"\operatorname{argmax}_{x}",
                build(|t| {
                    t.insert(Node::Raw(r"\operatorname{argmax}".to_string()));
                    t.make_script(false);
                    sym(t, "x");
                }),
            ),
            (
                r"\begin{pmatrix}1&2\\3&4\end{pmatrix}",
                build(|t| {
                    t.insert(Node::Raw(
                        r"\begin{pmatrix}1&2\\3&4\end{pmatrix}".to_string(),
                    ))
                }),
            ),
            (
                r"\left\{x\right\}",
                build(|t| {
                    t.make_delimited("{", "}");
                    sym(t, "x");
                }),
            ),
            (
                r"\left.x\right.",
                build(|t| {
                    t.make_delimited(".", ".");
                    sym(t, "x");
                }),
            ),
            (
                r"\left\langle x\right\rangle",
                build(|t| {
                    t.make_delimited(r"\langle", r"\rangle");
                    sym(t, "x");
                }),
            ),
            (
                r"\left(\left[x\right]\right)",
                build(|t| {
                    t.make_delimited("(", ")");
                    t.make_delimited("[", "]");
                    sym(t, "x");
                }),
            ),
        ];
        for (expected, tree) in cases {
            assert_eq!(tree.latex(), expected);
            let parsed = MathTree::parse(expected);
            assert_eq!(parsed.latex(), expected, "parse of {expected}");
            assert_eq!(MathTree::parse(&tree.latex()).latex(), tree.latex());
        }
    }

    #[test]
    fn parses_hand_written_latex() {
        let cases = [
            (r"\int_0^1 x^2\,dx", r"\int_{0}^{1}x^{2}\,dx"),
            (r"\sum_{n=1}^{\infty} n^2", r"\sum_{n=1}^{\infty}n^{2}"),
            (
                r"\begin{pmatrix}1&2\\3&4\end{pmatrix}",
                r"\begin{pmatrix}1&2\\3&4\end{pmatrix}",
            ),
            (r"\{x\in\mathbb{R}\mid x>0\}", r"\{x\in\mathbb{R}\mid x>0\}"),
            (r"\frac{a", r"\frac{a}{}"),
            (r"\left( x", r"\left(x\right."),
            (
                r"\left\langle x \right\rangle",
                r"\left\langle x\right\rangle",
            ),
            (r"\sqrt[3]{x}", r"\sqrt[3]{x}"),
            (r"x'", r"x'"),
            (r"\left\{ x \right\}", r"\left\{x\right\}"),
            ("{}_0", "{}_{0}"),
            (r"a_{}", r"a_{}"),
            (r"\mathbb R", r"\mathbb R"),
            (r"\alpha x", r"\alpha x"),
            ("x^", "x^{}"),
            ("}", "}"),
            (r"\right)", r"\right)"),
            (r"\rightarrow x", r"\rightarrow x"),
        ];
        for (input, expected) in cases {
            let parsed = MathTree::parse(input).latex();
            assert_eq!(parsed, expected, "input {input:?}");
            assert_eq!(
                MathTree::parse(&parsed).latex(),
                parsed,
                "re-parse of {input:?}"
            );
        }
        for input in [
            r"\int_0^1 x^2\,dx",
            r"\sum_{n=1}^{\infty} n^2",
            r"\begin{pmatrix}1&2\\3&4\end{pmatrix}",
            r"\{x\in\mathbb{R}\mid x>0\}",
            r"\frac{a",
            r"\left( x",
        ] {
            assert!(
                braces_balance(&MathTree::parse(input).latex()),
                "unbalanced {input:?}"
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

    struct Lcg(u64);

    impl Lcg {
        fn below(&mut self, n: usize) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((self.0 >> 33) as usize) % n
        }
    }

    #[test]
    fn random_edits_never_panic_and_always_round_trip() {
        const SYMBOLS: [(&str, &str); 4] = [("x", "x"), ("1", "1"), ("+", "+"), (r"\alpha", "a")];
        const PAIRS: [(&str, &str); 6] = [
            ("(", ")"),
            ("[", "]"),
            ("{", "}"),
            (".", "."),
            ("|", "|"),
            (r"\langle", r"\rangle"),
        ];
        for seed in 1..=30 {
            let mut rng = Lcg(seed);
            let mut t = MathTree::new();
            for step in 0..300 {
                match rng.below(22) {
                    0..=3 => {
                        let (latex, keys) = SYMBOLS[rng.below(SYMBOLS.len())];
                        t.insert_symbol(latex, keys);
                    }
                    4 => t.make_script(rng.below(2) == 0),
                    5 => t.make_prime(),
                    6 => t.make_fraction(),
                    7 => t.make_sqrt(),
                    8 => t.make_root(),
                    9 => {
                        let (left, right) = PAIRS[rng.below(PAIRS.len())];
                        t.make_delimited(left, right);
                    }
                    10 => t.left(),
                    11 => t.right(),
                    12 => t.up(),
                    13 => t.down(),
                    14 => t.next_argument(),
                    15 => t.previous_argument(),
                    16 => t.exit(rng.below(2) == 0),
                    17 => t.backspace(),
                    18 => t.delete(),
                    19 => t.remove_box(),
                    20 => t.unwrap_enclosing(),
                    _ => {
                        let right = PAIRS[rng.below(PAIRS.len())].1;
                        t.set_right_delimiter(right);
                    }
                }
                let latex = t.latex();
                assert_eq!(
                    MathTree::parse(&latex).latex(),
                    latex,
                    "seed {seed} step {step}"
                );
                assert_eq!(
                    t.latex_with_caret().matches(CARET).count(),
                    1,
                    "seed {seed} step {step}"
                );
            }
        }
    }
}
