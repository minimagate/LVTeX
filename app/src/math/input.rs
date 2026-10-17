//! Turns keys into edits of a `MathTree`, following TeXmacs math-mode bindings.
//!
//! Multi-key sequences live in the symbols' `keys` (`"< ="`, `"a var"`), so Tab and later keys
//! can extend the symbols just before the caret. The table was derived from TeXmacs's
//! `math-kbd.scm` and is now maintained by hand, using standard LaTeX names only.

use super::bindings::BINDINGS;
use super::tree::{MathTree, Node};

/// A key as the editor sees it. The app maps window events to these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathKey {
    /// A typed character, without Option or Control.
    Text(char),
    /// Option plus a key. `key` is the unshifted base character of the physical key.
    Alt {
        key: char,
        shift: bool,
    },
    Tab,
    ShiftTab,
    Return,
    Escape,
    Backspace,
    Delete,
    /// Option Backspace: removes the box before the caret.
    StructuredBackspace,
    /// Control Backspace: replaces the enclosing box by its contents.
    Unwrap,
    /// Command Option Backspace: moves the caret just before the enclosing box.
    ExitLeft,
    /// Command Option Delete: moves the caret just after the enclosing box.
    ExitRight,
    Left,
    Right,
    Up,
    Down,
    /// Command Option Left.
    PreviousArgument,
    /// Command Option Right.
    NextArgument,
    /// Shift F5: the next key is a big operator or symbol from `math:symbol`.
    SymbolPrefix,
    Undo,
    Redo,
}

/// What a key did, so the caller knows whether to redraw and whether to leave the editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Edited,
    Moved,
    /// Return with no `\command` pending: the formula is finished.
    Close,
    /// Escape with no `\command` pending: the edit is abandoned.
    Cancel,
    Ignored,
}

/// The longest key run, in symbols, that a binding can match.
const MAX_RUN: usize = 16;

/// A formula being typed with TeXmacs keys.
#[derive(Clone, Debug)]
pub struct MathInput {
    tree: MathTree,
    original: Option<(String, String)>,
    /// The letters of a `\command` being typed, without the backslash.
    command: Option<String>,
    /// Set by Shift F5 and consumed by the next key.
    symbol_prefix: bool,
    undo: Vec<MathTree>,
    redo: Vec<MathTree>,
}

impl MathInput {
    pub fn new(mut tree: MathTree) -> Self {
        // A parsed symbol has no typed keys, so give it the keys that would have typed it.
        tree.symbols_mut(|latex, keys| {
            if keys.is_empty()
                && let Some(found) = keys_for(latex)
            {
                *keys = found;
            }
        });
        Self {
            tree,
            original: None,
            command: None,
            symbol_prefix: false,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Creates an input that retains its imported LaTeX until the tree is edited.
    pub fn from_latex(latex: &str) -> Self {
        let mut input = Self::new(MathTree::parse(latex));
        let canonical = input.tree.latex();
        input.original = Some((latex.to_owned(), canonical));
        input
    }

    /// Handles one key. A key that changes the tree is one undo step.
    pub fn key(&mut self, key: MathKey) -> Outcome {
        match key {
            MathKey::Undo => self.restore(true),
            MathKey::Redo => self.restore(false),
            _ => {
                let before = self.tree.clone();
                let outcome = self.apply(key);
                if self.tree.latex() != before.latex() {
                    self.undo.push(before);
                    self.redo.clear();
                }
                outcome
            }
        }
    }

    pub fn latex(&self) -> String {
        self.source()
    }

    /// Confirms a pending `\command` and returns the completed formula source.
    pub fn finish(mut self) -> String {
        self.confirm_command();
        self.source()
    }

    fn source(&self) -> String {
        let current = self.tree.latex();
        if let Some((source, canonical)) = &self.original
            && canonical == &current
        {
            source.clone()
        } else {
            current
        }
    }

    /// The LaTeX for display: the caret, grey placeholders, and a pending `\command` at the caret.
    pub fn display_latex(&self) -> String {
        let mut tree = self.tree.clone();
        if let Some(name) = &self.command {
            tree.insert(Node::Raw(format!(
                r"\textcolor{{#73716F}}{{\backslash\hspace{{-0.15em}}\mathrm{{{name}}}}}"
            )));
        }
        tree.latex_with_caret()
    }

    fn apply(&mut self, key: MathKey) -> Outcome {
        if let Some(outcome) = self.command_key(key) {
            return outcome;
        }
        let prefixed = std::mem::take(&mut self.symbol_prefix);
        match key {
            MathKey::Text(c) => self.text(c, prefixed),
            MathKey::Alt { key: 'f', .. } => self.edit_tree(|t| t.make_fraction()),
            MathKey::Alt {
                key: 's',
                shift: false,
            } => self.edit_tree(|t| t.make_sqrt()),
            MathKey::Alt {
                key: 's',
                shift: true,
            } => self.edit_tree(|t| t.make_root()),
            MathKey::Alt { .. } => Outcome::Ignored,
            MathKey::Tab => {
                if self.combine("var") {
                    Outcome::Edited
                } else {
                    Outcome::Ignored
                }
            }
            MathKey::ShiftTab => self.unvary(),
            MathKey::Return => Outcome::Close,
            MathKey::Escape => Outcome::Cancel,
            MathKey::Backspace => self.edit_tree(|t| t.backspace()),
            MathKey::Delete => self.edit_tree(|t| t.delete()),
            MathKey::StructuredBackspace => self.edit_tree(|t| t.remove_box()),
            MathKey::Unwrap => self.edit_tree(|t| t.unwrap_enclosing()),
            MathKey::ExitLeft => self.move_caret(|t| t.exit(false)),
            MathKey::ExitRight => self.move_caret(|t| t.exit(true)),
            MathKey::Left => self.move_caret(|t| t.left()),
            MathKey::Right => self.move_caret(|t| t.right()),
            MathKey::Up => self.move_caret(|t| t.up()),
            MathKey::Down => self.move_caret(|t| t.down()),
            MathKey::PreviousArgument => self.move_caret(|t| t.previous_argument()),
            MathKey::NextArgument => self.move_caret(|t| t.next_argument()),
            MathKey::SymbolPrefix => {
                self.symbol_prefix = true;
                Outcome::Ignored
            }
            // Handled in `key`.
            MathKey::Undo | MathKey::Redo => Outcome::Ignored,
        }
    }

    /// Handles a key while a `\command` is pending. Returns None when the key should then be
    /// handled normally, after the command has been confirmed if it was not a letter.
    fn command_key(&mut self, key: MathKey) -> Option<Outcome> {
        let name = self.command.as_mut()?;
        match key {
            MathKey::Text(c) if c.is_ascii_alphabetic() => name.push(c),
            MathKey::Backspace => {
                if name.pop().is_none() {
                    self.command = None;
                }
            }
            MathKey::Escape => self.command = None,
            MathKey::Return => self.confirm_command(),
            _ => {
                self.confirm_command();
                return None;
            }
        }
        Some(Outcome::Edited)
    }

    /// Turns the pending `\command` into a fraction, a root or a symbol. An empty name inserts
    /// nothing.
    fn confirm_command(&mut self) {
        let Some(name) = self.command.take() else {
            return;
        };
        match name.as_str() {
            "" => {}
            "frac" => self.tree.make_fraction(),
            "sqrt" => self.tree.make_sqrt(),
            _ => {
                let latex = format!(r"\{name}");
                self.tree.insert_symbol(&latex, &latex);
            }
        }
    }

    fn text(&mut self, c: char, prefixed: bool) -> Outcome {
        if prefixed {
            let keys = format!("math:symbol {c}");
            if let Some(latex) = binding(&keys) {
                self.tree.insert_symbol(latex, &keys);
                return Outcome::Edited;
            }
        }
        match c {
            ' ' => return Outcome::Ignored,
            '\\' => self.command = Some(String::new()),
            '_' => self.tree.make_script(false),
            '^' => self.tree.make_script(true),
            '\'' => self.tree.make_prime(),
            '"' => {
                self.tree.make_prime();
                self.tree.make_prime();
            }
            '(' => self.tree.make_delimited("(", ")"),
            '[' => self.tree.make_delimited("[", "]"),
            '{' => self.tree.make_delimited("{", "}"),
            '|' => {
                if !self.tree.set_right_delimiter("|") {
                    self.tree.make_delimited("|", "|");
                }
            }
            ')' | ']' | '}' => {
                if !self.tree.set_right_delimiter(&c.to_string()) {
                    self.tree.insert_symbol(&plain(c), &c.to_string());
                }
            }
            _ => {
                if !self.combine(&c.to_string()) {
                    self.tree.insert_symbol(&plain(c), &c.to_string());
                }
            }
        }
        Outcome::Edited
    }

    /// Replaces the longest run of symbols before the caret, plus `key`, by a binding. Returns
    /// false, changing nothing, when no binding matches.
    fn combine(&mut self, key: &str) -> bool {
        let run = self.tree.keys_before_caret(MAX_RUN);
        for count in (0..=run.len()).rev() {
            let keys = run[run.len() - count..]
                .iter()
                .map(String::as_str)
                .chain([key])
                .collect::<Vec<_>>()
                .join(" ");
            if let Some(latex) = binding(&keys) {
                self.tree.replace_before(count, latex, &keys);
                return true;
            }
        }
        false
    }

    /// Shift Tab steps back from the last variant of the symbol before the caret.
    fn unvary(&mut self) -> Outcome {
        let Some(keys) = self.tree.keys_before_caret(1).pop() else {
            return Outcome::Ignored;
        };
        let Some(shorter) = keys.strip_suffix(" var") else {
            return Outcome::Ignored;
        };
        let latex = match binding(shorter) {
            Some(latex) => latex.to_string(),
            None => {
                let mut chars = shorter.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => plain(c),
                    _ => return Outcome::Ignored,
                }
            }
        };
        self.tree.replace_before(1, &latex, shorter);
        Outcome::Edited
    }

    /// Swaps the tree with the newest entry of one stack. A pending `\command` or ⇧F5 prefix is
    /// dropped first.
    fn restore(&mut self, undo: bool) -> Outcome {
        let cancelled_command = self.command.take().is_some();
        self.symbol_prefix = false;
        let (from, to) = if undo {
            (&mut self.undo, &mut self.redo)
        } else {
            (&mut self.redo, &mut self.undo)
        };
        let Some(tree) = from.pop() else {
            return if cancelled_command {
                Outcome::Edited
            } else {
                Outcome::Ignored
            };
        };
        to.push(std::mem::replace(&mut self.tree, tree));
        Outcome::Edited
    }

    fn edit_tree(&mut self, edit: impl FnOnce(&mut MathTree)) -> Outcome {
        edit(&mut self.tree);
        Outcome::Edited
    }

    fn move_caret(&mut self, step: impl FnOnce(&mut MathTree)) -> Outcome {
        step(&mut self.tree);
        Outcome::Moved
    }
}

fn binding(keys: &str) -> Option<&'static str> {
    BINDINGS
        .iter()
        .find(|(k, _)| *k == keys)
        .map(|&(_, latex)| latex)
}

/// LaTeX for a typed character that is not a binding.
fn plain(c: char) -> String {
    match c {
        '%' | '#' | '&' | '$' | '{' | '}' => format!(r"\{c}"),
        '~' => r"\sim".to_string(),
        _ => c.to_string(),
    }
}

/// The keys that type a symbol with this LaTeX, for a symbol read from a file. A single
/// printable ASCII character types itself; otherwise the binding with the fewest keys wins,
/// ties going to the first in the table.
fn keys_for(latex: &str) -> Option<String> {
    let mut chars = latex.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && c.is_ascii_graphic()
    {
        return Some(c.to_string());
    }
    BINDINGS
        .iter()
        .filter(|(_, bound)| *bound == latex)
        .min_by_key(|(keys, _)| keys.split(' ').count())
        .map(|(keys, _)| keys.to_string())
}

#[cfg(test)]
#[path = "../../tests/unit/math/input.rs"]
mod tests;
