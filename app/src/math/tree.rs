//! A formula typed by hand as boxes: the caret, its edits and the LaTeX it emits.
mod emit;

/// A sequence of sibling nodes. Every argument of a box is one row.
pub type Row = Vec<Node>;

/// The caret's way into a box: (node index in its row, argument index of that node).
type Step = (usize, usize);

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
    pub(super) root: Row,
    pub(super) path: Vec<Step>,
    pub(super) pos: usize,
}

impl MathTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn latex(&self) -> String {
        self.emit(false)
    }

    /// The LaTeX with the caret and grey placeholders in empty arguments, for display only.
    pub fn latex_with_caret(&self) -> String {
        self.emit(true)
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

    /// Calls `f` with the LaTeX and keys of every symbol in the tree, including symbols inside
    /// boxes.
    pub fn symbols_mut(&mut self, mut f: impl FnMut(&str, &mut String)) {
        fn visit(row: &mut Row, f: &mut impl FnMut(&str, &mut String)) {
            for node in row {
                if let Node::Symbol { latex, keys } = node {
                    f(latex, keys);
                }
                for k in 0..node.arg_count() {
                    if let Some(arg) = node.arg_mut(k) {
                        visit(arg, f);
                    }
                }
            }
        }
        visit(&mut self.root, &mut f);
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

#[cfg(test)]
#[path = "../../tests/unit/math/tree/mod.rs"]
mod tests;
