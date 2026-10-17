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
            .filter(|(_, latex)| crate::math::worker::render(&mut renderer, latex).is_err())
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
                crate::math::worker::render(&mut renderer, &latex).is_ok(),
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
        let preview = input.display_latex();
        assert_eq!(feed(&mut input, r"\a⎋"), Outcome::Edited);
        assert_eq!(input.latex(), "x");
        assert_eq!(input.display_latex(), preview);
        assert_eq!(input.key(MathKey::Return), Outcome::Close);
    }

    #[test]
    fn finishing_confirms_pending_commands() {
        for (keys, expected) in [
            (r"\alpha", r"\alpha"),
            (r"\unknownname", r"\unknownname"),
            (r"\frac", r"\frac{}{}"),
            (r"\sqrt", r"\sqrt{}"),
        ] {
            assert_eq!(typed(keys).finish(), expected, "command {keys:?}");
        }
    }

    #[test]
    fn undo_clearing_a_pending_command_requests_redraw() {
        let mut input = typed(r"\alpha");
        assert_eq!(input.key(MathKey::Undo), Outcome::Edited);
        assert!(input.latex().is_empty());
        assert_eq!(
            input.display_latex(),
            MathInput::new(MathTree::new()).display_latex()
        );
        assert_eq!(input.key(MathKey::Undo), Outcome::Ignored);
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

    #[test]
    fn imported_latex_survives_noop_motion_and_undo() {
        let cases = [("whitespace", " x ^2 "), ("unbraced script", "x^2")];
        for (name, source) in cases {
            let mut input = MathInput::from_latex(source);
            assert_eq!(input.latex(), source, "{name}");
            assert_eq!(input.key(MathKey::Left), Outcome::Moved, "{name}");
            assert_eq!(input.key(MathKey::Right), Outcome::Moved, "{name}");
            assert_eq!(input.latex(), source, "{name} after motion");
            assert_eq!(input.finish(), source, "{name} after finish");
        }

        let mut input = MathInput::from_latex("x^2");
        assert_eq!(input.key(MathKey::Text('y')), Outcome::Edited);
        assert_eq!(input.latex(), r"x^{2}y");
        assert_eq!(input.key(MathKey::Undo), Outcome::Edited);
        assert_eq!(input.latex(), "x^2");
        assert_eq!(input.finish(), "x^2");
    }

    #[test]
    fn imported_latex_edits_and_pending_commands_finish_canonically() {
        let mut edited = MathInput::from_latex("x^2");
        edited.key(MathKey::Text('y'));
        assert_eq!(edited.finish(), r"x^{2}y");

        let mut command = MathInput::from_latex("x^2");
        for c in r"\alpha".chars() {
            command.key(MathKey::Text(c));
        }
        assert_eq!(command.finish(), r"x^{2}\alpha");
    }

    #[test]
    fn imported_latex_preserves_comments_when_edited() {
        let mut input = MathInput::from_latex("x% comment\n+y");
        assert_eq!(input.latex(), "x% comment\n+y");
        input.key(MathKey::Text('z'));
        assert_eq!(input.latex(), "x% comment\n+yz");

        let mut terminal = MathInput::from_latex("x% naïve");
        assert_eq!(terminal.latex(), "x% naïve");
        terminal.key(MathKey::Text('y'));
        assert_eq!(terminal.latex(), "x% naïve\ny");
    }
}
