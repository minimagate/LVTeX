//! Turns keys into edits of a `MathTree`, following TeXmacs math-mode bindings.
//!
//! Multi-key sequences live in the symbols' `keys` (`"< ="`, `"a var"`), so Tab and later keys
//! can extend the symbols just before the caret. The table was derived from TeXmacs's
//! `math-kbd.scm` and is now maintained by hand, using standard LaTeX names only.

use crate::structure::{MathTree, Node};

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
            command: None,
            symbol_prefix: false,
            undo: Vec::new(),
            redo: Vec::new(),
        }
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
        self.tree.latex()
    }

    /// The LaTeX for display: the caret, grey placeholders, and a pending `\command` at the caret.
    pub fn display_latex(&self) -> String {
        let mut tree = self.tree.clone();
        if let Some(name) = &self.command {
            tree.insert(Node::Raw(format!(
                r"\textcolor{{#73716F}}{{\backslash\mathrm{{{name}}}}}"
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
        self.command = None;
        self.symbol_prefix = false;
        let (from, to) = if undo {
            (&mut self.undo, &mut self.redo)
        } else {
            (&mut self.redo, &mut self.undo)
        };
        let Some(tree) = from.pop() else {
            return Outcome::Ignored;
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

// BEGIN BINDINGS
const BINDINGS: &[(&str, &str)] = &[
    (", var", ","),
    (", ,", ",\\ldots,"),
    ("; ;", ";\\ldots;"),
    (".", "."),
    (". var", "."),
    ("math:symbol a", "\\amalg"),
    ("math:symbol d", "\\partial"),
    ("math:symbol p", "\\wp"),
    ("math:symbol n", "\\cap"),
    ("math:symbol n var", "\\sqcap"),
    ("math:symbol u", "\\cup"),
    ("math:symbol u var", "\\sqcup"),
    ("math:symbol w", "\\wedge"),
    ("math:symbol w var", "\\curlywedge"),
    ("math:symbol v", "\\vee"),
    ("math:symbol v var", "\\curlyvee"),
    ("math:symbol x", "\\times"),
    ("math:symbol I I", "\\iint"),
    ("math:symbol I", "\\int"),
    ("math:symbol I I I", "\\iiint"),
    ("math:symbol I I I I", "\\iiiint"),
    ("math:symbol O", "\\oint"),
    ("math:symbol O O", "\\oiint"),
    ("math:symbol O O O", "\\oiiint"),
    ("math:symbol A", "\\coprod"),
    ("math:symbol N", "\\bigcap"),
    ("math:symbol S", "\\sum"),
    ("math:symbol P", "\\prod"),
    ("math:symbol U", "\\bigcup"),
    ("math:symbol U var", "\\bigsqcup"),
    ("math:symbol V", "\\bigvee"),
    ("math:symbol W", "\\bigwedge"),
    ("<", "<"),
    (">", ">"),
    ("< /", "\\nless"),
    ("> /", "\\ngtr"),
    ("< . .", "<\\cdots<"),
    ("> . .", ">\\cdots>"),
    ("< =", "\\leqslant"),
    ("> =", "\\geqslant"),
    ("< = /", "\\not\\leqslant"),
    ("> = /", "\\ngeqslant"),
    ("< = . .", "\\leqslant\\cdots\\leqslant"),
    ("> = . .", "\\geqslant\\cdots\\geqslant"),
    ("< = var", "\\leq"),
    ("> = var", "\\geq"),
    ("< = var /", "\\nleq"),
    ("> = var /", "\\ngeq"),
    ("< = var . .", "\\leq\\cdots\\leq"),
    ("> = var . .", "\\geq\\cdots\\geq"),
    ("< <", "\\ll"),
    ("< < <", "\\lll"),
    ("> >", "\\gg"),
    ("> > >", "\\ggg"),
    ("< < . .", "\\ll\\cdots\\ll"),
    ("< < < . .", "\\lll\\cdots\\lll"),
    ("> > . .", "\\gg\\cdots\\gg"),
    ("> > > . .", "\\ggg\\cdots\\ggg"),
    ("< < /", "\\not\\ll"),
    ("< < < /", "\\not\\lll"),
    ("> > /", "\\not\\gg"),
    ("> > > /", "\\not\\ggg"),
    ("< .", "\\lessdot"),
    (". >", "\\gtrdot"),
    ("< >", "\\lessgtr"),
    ("< > var", "\\diamondsuit"),
    ("< > var var", "\\heartsuit"),
    ("< > var var var", "\\spadesuit"),
    ("< > var var var var", "\\clubsuit"),
    ("> <", "\\gtrless"),
    ("> < var", "\\Join"),
    ("< ~", "\\lesssim"),
    ("< ~ ~", "\\lessapprox"),
    ("> ~", "\\gtrsim"),
    ("> ~ ~", "\\gtrapprox"),
    ("> var ~", "\\gtrsim"),
    ("> var ~ ~", "\\gtrapprox"),
    ("< = var var", "\\leqq"),
    ("> = var var", "\\geqq"),
    ("< = var >", "\\lesseqgtr"),
    ("> = var <", "\\gtreqless"),
    ("< = var var >", "\\lesseqqgtr"),
    ("> = var var <", "\\gtreqqless"),
    ("< > /", "\\not\\lessgtr"),
    ("> < /", "\\not\\gtrless"),
    ("< ~ /", "\\not\\lesssim"),
    ("< ~ / var", "\\lnsim"),
    ("< ~ ~ /", "\\not\\lessapprox"),
    ("< ~ ~ / var", "\\lnapprox"),
    ("> ~ /", "\\not\\gtrsim"),
    ("> ~ / var", "\\gnsim"),
    ("> ~ ~ /", "\\not\\gtrapprox"),
    ("> ~ ~ / var", "\\gnapprox"),
    ("< = var var /", "\\not\\leqq"),
    ("> = var var /", "\\not\\geqq"),
    ("< = var > /", "\\not\\lesseqgtr"),
    ("> = var < /", "\\not\\gtreqless"),
    ("< = var var > /", "\\not\\lesseqqgtr"),
    ("> = var var < /", "\\not\\gtreqqless"),
    ("< = var / var", "\\lneq"),
    ("< = var var / var", "\\lneqq"),
    ("< = var var / var var", "\\lvertneqq"),
    ("> = var / var", "\\gneq"),
    ("> = var var / var", "\\gneqq"),
    ("> = var var / var var", "\\gvertneqq"),
    ("< var", "\\in"),
    ("> var", "\\ni"),
    ("< var /", "\\notin"),
    ("> var /", "\\not\\ni"),
    ("< var var", "\\subset"),
    ("> var var", "\\supset"),
    ("< var var . .", "\\subset\\cdots\\subset"),
    ("> var var . .", "\\supset\\cdots\\supset"),
    ("< var var /", "\\not\\subset"),
    ("> var var /", "\\not\\supset"),
    ("< var var =", "\\subseteq"),
    ("> var var =", "\\supseteq"),
    ("< var var = . .", "\\subseteq\\cdots\\subseteq"),
    ("> var var = . .", "\\supseteq\\cdots\\supseteq"),
    ("< var var = /", "\\nsubseteq"),
    ("> var var = /", "\\nsupseteq"),
    ("< var var = var", "\\subseteqq"),
    ("> var var = var", "\\supseteqq"),
    ("< var var = var . .", "\\subseteqq\\cdots\\subseteqq"),
    ("> var var = var . .", "\\supseteqq\\cdots\\supseteqq"),
    ("< var var = var /", "\\not\\subseteqq"),
    ("> var var = var /", "\\not\\supseteqq"),
    ("< var var = / var", "\\subsetneq"),
    ("> var var = / var", "\\supsetneq"),
    ("< var var = / var . .", "\\subsetneq\\cdots\\subsetneq"),
    ("> var var = / var . .", "\\supsetneq\\cdots\\supsetneq"),
    ("< var var = / var var", "\\varsubsetneq"),
    ("> var var = / var var", "\\varsupsetneq"),
    (
        "< var var = / var var . .",
        "\\varsubsetneq\\cdots\\varsubsetneq",
    ),
    (
        "> var var = / var var . .",
        "\\varsupsetneq\\cdots\\varsupsetneq",
    ),
    ("< var var = var / var", "\\subsetneqq"),
    ("> var var = var / var", "\\supsetneqq"),
    (
        "< var var = var / var . .",
        "\\subsetneqq\\cdots\\subsetneqq",
    ),
    (
        "> var var = var / var . .",
        "\\supsetneqq\\cdots\\supsetneqq",
    ),
    ("< var var = var / var var", "\\varsubsetneqq"),
    ("> var var = var / var var", "\\varsupsetneqq"),
    (
        "< var var = var / var var . .",
        "\\varsubsetneqq\\cdots\\varsubsetneqq",
    ),
    (
        "> var var = var / var var . .",
        "\\varsupsetneqq\\cdots\\varsupsetneqq",
    ),
    ("< var var var", "\\prec"),
    ("> var var var", "\\succ"),
    ("< var var var . .", "\\prec\\cdots\\prec"),
    ("> var var var . .", "\\succ\\cdots\\succ"),
    ("< var var var /", "\\nprec"),
    ("> var var var /", "\\nsucc"),
    ("< var var var =", "\\preccurlyeq"),
    ("> var var var =", "\\succcurlyeq"),
    ("< var var var = . .", "\\preccurlyeq\\cdots\\preccurlyeq"),
    ("> var var var = . .", "\\succcurlyeq\\cdots\\succcurlyeq"),
    ("< var var var = /", "\\not\\preccurlyeq"),
    ("> var var var = /", "\\not\\succcurlyeq"),
    ("< var var var = var", "\\preceq"),
    ("> var var var = var", "\\succeq"),
    ("< var var var = var . .", "\\preceq\\cdots\\preceq"),
    ("> var var var = var . .", "\\succeq\\cdots\\succeq"),
    ("< var var var = var /", "\\npreceq"),
    ("> var var var = var /", "\\nsucceq"),
    ("< var var var = var / var", "\\precneq"),
    ("> var var var = var / var", "\\succneq"),
    ("< var var var = var var", "\\preceqq"),
    ("> var var var = var var", "\\succeqq"),
    ("< var var var = var var . .", "\\preceqq\\cdots\\preceqq"),
    ("> var var var = var var . .", "\\succeqq\\cdots\\succeqq"),
    ("< var var var = var var /", "\\not\\preceqq"),
    ("> var var var = var var /", "\\not\\succeqq"),
    ("< var var var = var var / var", "\\precneqq"),
    ("> var var var = var var / var", "\\succneqq"),
    ("< var var var ~", "\\precsim"),
    ("> var var var ~", "\\succsim"),
    ("< var var var ~ ~", "\\precapprox"),
    ("> var var var ~ ~", "\\succapprox"),
    ("< var var var ~ . .", "\\precsim\\cdots\\precsim"),
    ("> var var var ~ . .", "\\succsim\\cdots\\succsim"),
    ("< var var var ~ ~ . .", "\\precapprox\\cdots\\precapprox"),
    ("> var var var ~ ~ . .", "\\succapprox\\cdots\\succapprox"),
    ("< var var var ~ /", "\\not\\precsim"),
    ("> var var var ~ /", "\\not\\succsim"),
    ("< var var var ~ / var", "\\precnsim"),
    ("> var var var ~ / var", "\\succnsim"),
    ("< var var var ~ ~ /", "\\not\\precapprox"),
    ("> var var var ~ ~ /", "\\not\\succapprox"),
    ("< var var var ~ ~ / var", "\\precnapprox"),
    ("> var var var ~ ~ / var", "\\succnapprox"),
    ("< < var", "\\Prec"),
    ("> > var", "\\Succ"),
    ("< < var . .", "\\Prec\\cdots\\Prec"),
    ("> > var . .", "\\Succ\\cdots\\Succ"),
    ("< var var var var", "\\sqsubset"),
    ("> var var var var", "\\sqsupset"),
    ("< var var var var . .", "\\sqsubset\\cdots\\sqsubset"),
    ("> var var var var . .", "\\sqsupset\\cdots\\sqsupset"),
    ("< var var var var /", "\\not\\sqsubset"),
    ("> var var var var /", "\\not\\sqsupset"),
    ("< var var var var =", "\\sqsubseteq"),
    ("> var var var var =", "\\sqsupseteq"),
    ("< var var var var = . .", "\\sqsubseteq\\cdots\\sqsubseteq"),
    ("> var var var var = . .", "\\sqsupseteq\\cdots\\sqsupseteq"),
    ("< var var var var = /", "\\not\\sqsubseteq"),
    ("> var var var var = /", "\\not\\sqsupseteq"),
    ("< < var var", "\\Subset"),
    ("> > var var", "\\Supset"),
    ("< < var var . .", "\\Subset\\cdots\\Subset"),
    ("> > var var . .", "\\Supset\\cdots\\Supset"),
    ("- >", "\\rightarrow"),
    ("- > var", "\\downarrow"),
    ("- > var var", "\\uparrow"),
    ("- > var var var", "\\searrow"),
    ("- > var var var var", "\\nearrow"),
    ("- > . .", "\\rightarrow\\cdots\\rightarrow"),
    ("< -", "\\leftarrow"),
    ("< - var", "\\uparrow"),
    ("< - var var", "\\downarrow"),
    ("< - var var var", "\\nwarrow"),
    ("< - var var var var", "\\swarrow"),
    ("< - . .", "\\leftarrow\\cdots\\leftarrow"),
    ("< - >", "\\leftrightarrow"),
    ("< - > var", "\\updownarrow"),
    ("< - > . .", "\\leftrightarrow\\cdots\\leftrightarrow"),
    ("- - >", "\\longrightarrow"),
    ("- - > . .", "\\longrightarrow\\cdots\\longrightarrow"),
    ("< - -", "\\longleftarrow"),
    ("< - - . .", "\\longleftarrow\\cdots\\longleftarrow"),
    ("< - - >", "\\longleftrightarrow"),
    (
        "< - - > . .",
        "\\longleftrightarrow\\cdots\\longleftrightarrow",
    ),
    ("= >", "\\Rightarrow"),
    ("= > var", "\\Downarrow"),
    ("= > var var", "\\Uparrow"),
    ("= > . .", "\\Rightarrow\\cdots\\Rightarrow"),
    ("< = var var var", "\\Leftarrow"),
    ("< = var var var . .", "\\Leftarrow\\cdots\\Leftarrow"),
    ("< = >", "\\Leftrightarrow"),
    ("< = > var", "\\Updownarrow"),
    ("< = > . .", "\\Leftrightarrow\\cdots\\Leftrightarrow"),
    ("= = >", "\\Longrightarrow"),
    ("= = > . .", "\\Longrightarrow\\cdots\\Longrightarrow"),
    ("< = =", "\\Longleftarrow"),
    ("< = = . .", "\\Longleftarrow\\cdots\\Longleftarrow"),
    ("< = = >", "\\Longleftrightarrow"),
    (
        "< = = > . .",
        "\\Longleftrightarrow\\cdots\\Longleftrightarrow",
    ),
    ("= var var >", "\\Rrightarrow"),
    ("= > var var var", "\\Rrightarrow"),
    ("< = var var var var", "\\Lleftarrow"),
    ("- / var", "\\rightharpoondown"),
    ("- / var var", "\\rightharpoonup"),
    ("- / var var var", "\\downharpoonleft"),
    ("- / var var var var", "\\downharpoonright"),
    ("- / var var var var var", "\\upharpoonright"),
    ("- / var var var var var var", "\\upharpoonleft"),
    ("/ - var", "\\leftharpoonup"),
    ("/ - var var", "\\leftharpoondown"),
    ("/ - var var var", "\\upharpoonright"),
    ("/ - var var var var", "\\upharpoonleft"),
    ("/ - var var var var var", "\\downharpoonleft"),
    ("/ - var var var var var var", "\\downharpoonright"),
    ("~ >", "\\rightsquigarrow"),
    ("~ > . .", "\\rightsquigarrow\\cdots\\rightsquigarrow"),
    ("< ~ var", "\\leftsquigarrow"),
    ("< ~ . .", "\\leftsquigarrow\\cdots\\leftsquigarrow"),
    ("< ~ >", "\\leftrightsquigarrow"),
    (
        "< ~ > . .",
        "\\leftrightsquigarrow\\cdots\\leftrightsquigarrow",
    ),
    ("< - <", "\\leftarrowtail"),
    ("> - >", "\\rightarrowtail"),
    ("- > - >", "\\rightrightarrows"),
    ("- > - > var", "\\downdownarrows"),
    ("- > - > var var", "\\upuparrows"),
    ("< - < -", "\\leftleftarrows"),
    ("< - < - var", "\\upuparrows"),
    ("< - < - var var", "\\downdownarrows"),
    ("- > < -", "\\rightleftarrows"),
    ("- > < - var", "\\rightleftharpoons"),
    ("< - - > var var", "\\leftrightarrows"),
    ("< - - > var var var", "\\leftrightharpoons"),
    ("- > >", "\\twoheadrightarrow"),
    ("< < -", "\\twoheadleftarrow"),
    ("- > /", "\\nrightarrow"),
    ("< - /", "\\nleftarrow"),
    ("< - > /", "\\nleftrightarrow"),
    ("= > /", "\\nRightarrow"),
    ("< = var var var /", "\\nLeftarrow"),
    ("< = > /", "\\nLeftrightarrow"),
    ("< - @", "\\looparrowleft"),
    ("@ - >", "\\looparrowright"),
    ("@", "\\circ"),
    ("@ /", "\\varnothing"),
    ("@ +", "\\oplus"),
    ("@ + . .", "\\oplus\\cdots\\oplus"),
    ("@ -", "\\ominus"),
    ("@ .", "\\odot"),
    ("@ . .", "\\circ\\cdots\\circ"),
    ("@ x", "\\otimes"),
    ("@ x . .", "\\otimes\\cdots\\otimes"),
    ("@ *", "\\otimes"),
    ("@ * . .", "\\otimes\\cdots\\otimes"),
    ("@ * var", "\\circledast"),
    ("@ x var . .", "\\circledast\\cdots\\circledast"),
    ("@ R", "\\circledR"),
    ("@ S", "\\circledS"),
    ("@ / var", "\\oslash"),
    ("@ < var", "\\circlearrowleft"),
    ("@ > var", "\\circlearrowright"),
    ("@ @", "\\infty"),
    ("@ @ var", "\\circledcirc"),
    ("@ @ var var", "\\propto"),
    ("- @ @", "-\\infty"),
    ("@ var", "\\square"),
    ("@ var +", "\\boxplus"),
    ("@ var -", "\\boxminus"),
    ("@ var x", "\\boxtimes"),
    ("@ var *", "\\boxtimes"),
    ("@ var .", "\\boxdot"),
    ("@ var var", "\\bullet"),
    ("@ var var var", "\\blacksquare"),
    ("@ var var var var", "\\vartriangle"),
    ("@ var var var var var", "\\blacktriangle"),
    ("= var", "\\asymp"),
    ("= var . .", "\\asymp\\cdots\\asymp"),
    ("= var var", "\\equiv"),
    ("= var var . .", "\\equiv\\cdots\\equiv"),
    ("= . .", "=\\cdots="),
    ("= :", "\\eqqcolon"),
    ("~", "\\sim"),
    ("~ var", "\\backsim"),
    ("~ . .", "\\sim\\cdots\\sim"),
    ("~ ~", "\\approx"),
    ("~ ~ . .", "\\approx\\cdots\\approx"),
    ("~ ~ var", "\\Bumpeq"),
    ("~ ~ var . .", "\\Bumpeq\\cdots\\Bumpeq"),
    ("~ ~ -", "\\approxeq"),
    ("~ ~ - . .", "\\approxeq\\cdots\\approxeq"),
    ("~ ~ - /", "\\not\\approxeq"),
    ("~ ~ =", "\\approxeq"),
    ("~ ~ = . .", "\\approxeq\\cdots\\approxeq"),
    ("~ ~ = /", "\\not\\approxeq"),
    ("~ -", "\\simeq"),
    ("~ - . .", "\\simeq\\cdots\\simeq"),
    ("~ - var", "\\backsimeq"),
    ("~ - var . .", "\\backsimeq\\cdots\\backsimeq"),
    ("~ - var var", "\\bumpeq"),
    ("~ - var var . .", "\\bumpeq\\cdots\\bumpeq"),
    ("~ =", "\\cong"),
    ("~ = . .", "\\cong\\cdots\\cong"),
    ("= /", "\\neq"),
    ("= var /", "\\not\\asymp"),
    ("= var var /", "\\not\\equiv"),
    ("~ /", "\\nsim"),
    ("~ ~ /", "\\not\\approx"),
    ("~ - /", "\\not\\simeq"),
    ("~ = /", "\\ncong"),
    ("# var", "\\sharp"),
    ("# var var", "\\natural"),
    ("# var var var", "\\flat"),
    ("&", "\\wedge"),
    ("& . .", "\\wedge\\cdots\\wedge"),
    ("& var", "\\cap"),
    ("& var . .", "\\cap\\cdots\\cap"),
    ("& var var", "\\sqcap"),
    ("& var var . .", "\\sqcap\\cdots\\sqcap"),
    ("& var var var", "\\prod"),
    ("& var var var . .", "\\prod\\cdots\\prod"),
    ("& var var var var", "\\curlywedge"),
    (
        "& var var var var var . .",
        "\\curlywedge\\cdots\\curlywedge",
    ),
    ("& var var var var var var var", "\\&"),
    ("- &", "\\barwedge"),
    ("- & . .", "\\barwedge\\cdots\\barwedge"),
    ("%", "\\vee"),
    ("% . .", "\\vee\\cdots\\vee"),
    ("% var", "\\cup"),
    ("% var . .", "\\cup\\cdots\\cup"),
    ("% var +", "\\uplus"),
    ("% var + . .", "\\uplus\\cdots\\uplus"),
    ("% var + +", "\\uplus\\cdots\\uplus"),
    ("% var var", "\\sqcup"),
    ("% var var . .", "\\sqcup\\cdots\\sqcup"),
    ("% var var var", "\\amalg"),
    ("% var var var . .", "\\amalg\\cdots\\amalg"),
    ("% var var var var", "\\curlyvee"),
    ("% var var var var . .", "\\curlyvee\\cdots\\curlyvee"),
    ("% var var var var var var var var", "\\%"),
    ("% -", "\\veebar"),
    ("% - . .", "\\veebar\\cdots\\veebar"),
    ("+ var var", "\\dagger"),
    ("+ var var var", "\\ddagger"),
    ("+ . .", "+\\cdots+"),
    ("+ +", "+\\cdots+"),
    ("+ -", "\\pm"),
    ("- +", "\\mp"),
    ("+ - . .", "\\pm\\cdots\\pm"),
    ("- + . .", "\\mp\\cdots\\mp"),
    ("@ =", "\\circeq"),
    ("@ = var", "\\triangleq"),
    ("= @", "\\eqcirc"),
    ("- . .", "-\\cdots-"),
    ("- @", "\\multimap"),
    (". =", "\\doteq"),
    (". +", "\\dotplus"),
    (". -", "\\dotminus"),
    (". *", "\\dottimes"),
    (". .", "\\ldots"),
    (". . var", "\\cdots"),
    (". . var var", "\\cdots"),
    (". . var var var", "\\vdots"),
    (". . var var var var", "\\ddots"),
    (". . var var var var var", "\\iddots"),
    (":", ":"),
    (": var var var", "\\ni"),
    (": =", "\\coloneqq"),
    ("/ var var", "\\div"),
    ("*", "*"),
    ("* . .", "*\\cdots*"),
    ("* *", "*\\cdots*"),
    ("* var", "\\times"),
    ("* var . .", "\\times\\cdots\\times"),
    ("* var var", "\\ast"),
    ("* var var . .", "\\ast\\cdots\\ast"),
    ("* var var var", "\\cdot"),
    ("* var var var . .", "\\cdot\\cdots\\cdot"),
    ("* var var var var var", "\\star"),
    ("! var", "\\neg"),
    ("a var", "\\alpha"),
    ("b var", "\\beta"),
    ("b var var", "\\flat"),
    ("c var", "\\varsigma"),
    ("c var var", "\\sigma"),
    ("d var", "\\delta"),
    ("d var var", "\\mathrm{d}"),
    ("d var var var", "\\partial"),
    ("e var", "\\varepsilon"),
    ("e var var", "\\mathrm{e}"),
    ("e var var var", "\\epsilon"),
    ("e var var var var", "\\backepsilon"),
    ("g var", "\\gamma"),
    ("h var", "\\eta"),
    ("h var var", "\\hbar"),
    ("i var", "\\iota"),
    ("i var var", "\\mathrm{i}"),
    ("i var var var", "\\imath"),
    ("j var", "\\theta"),
    ("j var var", "\\jmath"),
    ("j var var var", "\\vartheta"),
    ("k var", "\\kappa"),
    ("k var var", "\\varkappa"),
    ("l var", "\\lambda"),
    ("l var var var", "\\ell"),
    ("m var", "\\mu"),
    ("n var", "\\nu"),
    ("o var", "\\omicron"),
    ("p var", "\\pi"),
    ("p var var var", "\\varpi"),
    ("q var", "\\chi"),
    ("r var", "\\rho"),
    ("r var var", "\\varrho"),
    ("s var", "\\sigma"),
    ("s var var", "\\varsigma"),
    ("t var", "\\tau"),
    ("u var", "\\upsilon"),
    ("v var", "\\phi"),
    ("v var var", "\\varphi"),
    ("f var", "\\varphi"),
    ("f var var", "\\phi"),
    ("w var", "\\omega"),
    ("w var var", "\\mho"),
    ("x var", "\\xi"),
    ("y var", "\\psi"),
    ("z var", "\\zeta"),
    ("A var", "A"),
    ("A var var", "\\forall"),
    ("A var var var", "\\aleph"),
    ("B var", "B"),
    ("B var var", "\\bot"),
    ("B var var var", "\\beth"),
    ("C var", "\\Sigma"),
    ("C var var var", "\\complement"),
    ("D var", "\\Delta"),
    ("D var var var", "\\daleth"),
    ("E var", "E"),
    ("E var var", "\\exists"),
    ("E var var var", "\\exists!"),
    ("E var var var var", "\\backepsilon"),
    ("G var", "\\Gamma"),
    ("G var var", "\\gimel"),
    ("G var var var var", "\\digamma"),
    ("H var", "H"),
    ("I var", "I"),
    ("I var var", "\\Im"),
    ("J var", "\\Theta"),
    ("K var", "K"),
    ("L var", "\\Lambda"),
    ("M var", "M"),
    ("N var", "N"),
    ("O var", "O"),
    ("P var", "\\Pi"),
    ("P var var", "\\wp"),
    ("Q var", "X"),
    ("R var", "P"),
    ("R var var", "\\Re"),
    ("S var", "\\Sigma"),
    ("T var", "T"),
    ("T var var", "\\top"),
    ("T var var var", "\\bot"),
    ("T var var var var", "\\leftthreetimes"),
    ("T var var var var var", "\\rightthreetimes"),
    ("U var", "\\Upsilon"),
    ("V var", "\\Phi"),
    ("V var var", "\\nabla"),
    ("F var", "\\Phi"),
    ("W var", "\\Omega"),
    ("W var var", "\\mho"),
    ("X var", "\\Xi"),
    ("Y var", "\\Psi"),
    ("Z var", "Z"),
    ("A A", "\\mathbb{A}"),
    ("B B", "\\mathbb{B}"),
    ("C C", "\\mathbb{C}"),
    ("D D", "\\mathbb{D}"),
    ("E E", "\\mathbb{E}"),
    ("F F", "\\mathbb{F}"),
    ("G G", "\\mathbb{G}"),
    ("H H", "\\mathbb{H}"),
    ("I I", "\\mathbb{I}"),
    ("J J", "\\mathbb{J}"),
    ("K K", "\\mathbb{K}"),
    ("L L", "\\mathbb{L}"),
    ("M M", "\\mathbb{M}"),
    ("N N", "\\mathbb{N}"),
    ("O O", "\\mathbb{O}"),
    ("P P", "\\mathbb{P}"),
    ("Q Q", "\\mathbb{Q}"),
    ("R R", "\\mathbb{R}"),
    ("S S", "\\mathbb{S}"),
    ("T T", "\\mathbb{T}"),
    ("U U", "\\mathbb{U}"),
    ("V V", "\\mathbb{V}"),
    ("W W", "\\mathbb{W}"),
    ("X X", "\\mathbb{X}"),
    ("Y Y", "\\mathbb{Y}"),
    ("Z Z", "\\mathbb{Z}"),
    ("R E", "\\Re"),
    ("I M", "\\Im"),
];
// END BINDINGS

#[cfg(test)]
mod tests {
    use super::*;
    use mathstral_render::math::Renderer;

    /// Feeds one key per character. The markers stand for the keys that are not characters, and
    /// the outcome of the last key is returned.
    fn feed(input: &mut MathInput, script: &str) -> Outcome {
        let mut last = Outcome::Ignored;
        for c in script.chars() {
            let key = match c {
                '⇥' => MathKey::Tab,
                '⇤' => MathKey::ShiftTab,
                '↵' => MathKey::Return,
                '⎋' => MathKey::Escape,
                '⌫' => MathKey::Backspace,
                '⌦' => MathKey::Delete,
                '←' => MathKey::Left,
                '→' => MathKey::Right,
                '↑' => MathKey::Up,
                '↓' => MathKey::Down,
                '⇧' => MathKey::SymbolPrefix,
                _ => MathKey::Text(c),
            };
            last = input.key(key);
        }
        last
    }

    fn typed(script: &str) -> MathInput {
        let mut input = MathInput::new(MathTree::new());
        feed(&mut input, script);
        input
    }

    fn parsed(latex: &str) -> MathInput {
        MathInput::new(MathTree::parse(latex))
    }

    #[test]
    fn parsed_letter_takes_its_tab_variant() {
        let mut input = parsed("a");
        assert_eq!(input.key(MathKey::Tab), Outcome::Edited);
        assert_eq!(input.latex(), r"\alpha");
    }

    #[test]
    fn parsed_variant_has_no_further_tab_and_shift_tab_steps_back() {
        let mut input = parsed(r"\alpha");
        assert_eq!(input.key(MathKey::Tab), Outcome::Ignored);
        assert_eq!(input.key(MathKey::ShiftTab), Outcome::Edited);
        assert_eq!(input.latex(), "a");
    }

    #[test]
    fn parsed_relation_combines_like_typed_keys() {
        let mut input = parsed("<");
        input.key(MathKey::Text('='));
        assert_eq!(input.latex(), r"\leqslant");
        assert_eq!(input.latex(), typed("<=").latex());
    }

    #[test]
    fn parsed_comma_keeps_its_typed_key_not_a_variant() {
        let mut input = parsed(",");
        feed(&mut input, ",");
        assert_eq!(input.latex(), r",\ldots,");
    }

    #[test]
    fn symbol_inside_a_parsed_fraction_takes_keys() {
        let mut input = parsed(r"\frac{a}{b}");
        for _ in 0..3 {
            input.key(MathKey::Left);
        }
        assert_eq!(input.key(MathKey::Tab), Outcome::Edited);
        assert_eq!(input.latex(), r"\frac{\alpha}{b}");
    }

    #[test]
    fn every_binding_renders() {
        let mut renderer = Renderer::with_prelude(mathstral_latex::PRELUDE);
        let failed: Vec<String> = BINDINGS
            .iter()
            .filter(|(_, latex)| crate::math::render(&mut renderer, latex).is_err())
            .map(|(keys, latex)| format!("{keys} -> {latex}"))
            .collect();
        assert!(
            failed.is_empty(),
            "bindings that do not render: {failed:#?}"
        );
    }

    #[test]
    fn bindings_fit_the_run_limit() {
        for (keys, _) in BINDINGS {
            let symbols = keys.split(' ').filter(|key| *key != "var").count();
            assert!(symbols <= MAX_RUN, "{keys} has {symbols} keys");
        }
    }

    #[test]
    fn display_latex_renders_for_every_state() {
        let mut renderer = Renderer::with_prelude(mathstral_latex::PRELUDE);
        let mut empty_fraction = MathInput::new(MathTree::new());
        empty_fraction.key(MathKey::Alt {
            key: 'f',
            shift: false,
        });
        let states = [
            MathInput::new(MathTree::new()),
            empty_fraction,
            typed("x^a^b"),
            typed(r"\al"),
        ];
        for input in states {
            let latex = input.display_latex();
            assert!(
                crate::math::render(&mut renderer, &latex).is_ok(),
                "display {latex}"
            );
        }
        assert!(
            typed(r"\al")
                .display_latex()
                .contains(r"\textcolor{#73716F}{\backslash\mathrm{al}}")
        );
    }

    #[test]
    fn texmacs_bindings_apply_to_the_symbols_before_the_caret() {
        let cases = [
            ("<=", r"\leqslant"),
            ("<", "<"),
            ("->", r"\rightarrow"),
            ("-->", r"\longrightarrow"),
            ("=/", r"\neq"),
            ("a⇥", r"\alpha"),
            ("a⇥⇥", r"\alpha"),
            ("a⇥⇤", "a"),
            ("&", r"\wedge"),
            ("&⇥", r"\cap"),
            ("&..", r"\wedge\cdots\wedge"),
            ("⇧S", r"\sum"),
            ("⇧n⇥", r"\sqcap"),
            ("<⇥/", r"\notin"),
            ("=⇥⇥/", r"\not\equiv"),
            ("<⇥⇥/", r"\not\subset"),
            ("<<⇥", r"\Prec"),
            ("<<⇥..", r"\Prec\cdots\Prec"),
            ("Q⇥", "X"),
            ("R⇥", "P"),
        ];
        for (script, expected) in cases {
            assert_eq!(typed(script).latex(), expected, "script {script:?}");
        }
    }

    #[test]
    fn backslash_commands_confirm_on_return_or_before_another_key() {
        let cases = [
            ("\\alpha↵", r"\alpha"),
            ("\\alpha^2", r"\alpha^{2}"),
            ("\\frac↵1↓2", r"\frac{1}{2}"),
            ("\\sqrt↵x", r"\sqrt{x}"),
            ("\\↵", ""),
        ];
        for (script, expected) in cases {
            assert_eq!(typed(script).latex(), expected, "script {script:?}");
        }
    }

    #[test]
    fn brackets_pair_and_closers_step_out() {
        let cases = [
            ("(x)", r"\left(x\right)"),
            ("(x]", r"\left(x\right]"),
            ("|x|", r"\left|x\right|"),
            ("[x", r"\left[x\right]"),
            ("x)", "x)"),
        ];
        for (script, expected) in cases {
            assert_eq!(typed(script).latex(), expected, "script {script:?}");
        }
    }

    #[test]
    fn option_keys_make_fractions_and_roots() {
        let mut frac = MathInput::new(MathTree::new());
        frac.key(MathKey::Alt {
            key: 'f',
            shift: false,
        });
        feed(&mut frac, "1↓2");
        assert_eq!(frac.latex(), r"\frac{1}{2}");

        let mut root = MathInput::new(MathTree::new());
        root.key(MathKey::Alt {
            key: 's',
            shift: true,
        });
        feed(&mut root, "3↓x");
        assert_eq!(root.latex(), r"\sqrt[3]{x}");

        let mut ignored = MathInput::new(MathTree::new());
        let outcome = ignored.key(MathKey::Alt {
            key: 'q',
            shift: false,
        });
        assert_eq!(outcome, Outcome::Ignored);
    }

    #[test]
    fn return_closes_escape_cancels_and_escape_drops_only_a_command() {
        assert_eq!(typed("x").key(MathKey::Return), Outcome::Close);
        assert_eq!(
            MathInput::new(MathTree::new()).key(MathKey::Escape),
            Outcome::Cancel
        );
        let mut input = typed("x");
        assert_eq!(feed(&mut input, r"\a⎋"), Outcome::Edited);
        assert_eq!(input.latex(), "x");
        assert_eq!(input.key(MathKey::Return), Outcome::Close);
    }

    #[test]
    fn navigation_and_structural_keys_edit_the_tree() {
        assert_eq!(typed("ab←⌫").latex(), "b");
        assert_eq!(typed("ab←←⌦").latex(), "b");
        assert_eq!(typed("ab←←→x").latex(), "axb");

        let mut frac = MathInput::new(MathTree::new());
        frac.key(MathKey::Alt {
            key: 'f',
            shift: false,
        });
        feed(&mut frac, "1↓2↑3");
        assert_eq!(frac.latex(), r"\frac{13}{2}");
        frac.key(MathKey::ExitLeft);
        feed(&mut frac, "x");
        assert_eq!(frac.latex(), r"x\frac{13}{2}");

        let mut args = MathInput::new(MathTree::new());
        args.key(MathKey::Alt {
            key: 'f',
            shift: false,
        });
        feed(&mut args, "1");
        assert_eq!(args.key(MathKey::NextArgument), Outcome::Moved);
        feed(&mut args, "2");
        assert_eq!(args.key(MathKey::PreviousArgument), Outcome::Moved);
        feed(&mut args, "0");
        assert_eq!(args.latex(), r"\frac{10}{2}");

        let mut whole = typed("a");
        whole.key(MathKey::Alt {
            key: 'f',
            shift: false,
        });
        whole.key(MathKey::ExitRight);
        assert_eq!(whole.key(MathKey::StructuredBackspace), Outcome::Edited);
        assert_eq!(whole.latex(), "a");

        let mut unwrapped = MathInput::new(MathTree::new());
        unwrapped.key(MathKey::Alt {
            key: 'f',
            shift: false,
        });
        feed(&mut unwrapped, "1↓2");
        assert_eq!(unwrapped.key(MathKey::Unwrap), Outcome::Edited);
        assert_eq!(unwrapped.latex(), "12");
    }

    #[test]
    fn undo_and_redo_step_through_edits_but_not_movement() {
        let mut input = typed("xy");
        assert_eq!(input.key(MathKey::Undo), Outcome::Edited);
        assert_eq!(input.latex(), "x");
        assert_eq!(input.key(MathKey::Redo), Outcome::Edited);
        assert_eq!(input.latex(), "xy");
        input.key(MathKey::Undo);
        input.key(MathKey::Undo);
        assert!(input.latex().is_empty());
        assert_eq!(input.key(MathKey::Undo), Outcome::Ignored);

        let mut moved = typed("x←");
        moved.key(MathKey::Undo);
        assert!(moved.latex().is_empty());
    }
}
