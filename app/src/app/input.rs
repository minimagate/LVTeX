//! Keyboard, text and mouse input, routed to the document or to the open formula.
use super::App;
use crate::{config::*, markdown, math::input::MathKey};
use winit::{
    event::{ElementState, KeyEvent},
    keyboard::{Key, ModifiersState, NamedKey},
    platform::modifier_supplement::KeyEventExtModifierSupplement,
};
impl App {
    pub(super) fn key(&mut self, event: KeyEvent) {
        if event.state != ElementState::Pressed || self.composing {
            return;
        }
        if self.settings.is_some() {
            self.settings_key(&event);
            return;
        }
        if self.math.is_some() {
            let keys = math_keys(&event, self.modifiers);
            self.math_input(keys);
            return;
        }
        self.refresh_layout();
        let shift = self.modifiers.shift_key();
        let command = self.modifiers.super_key();
        let option = self.modifiers.alt_key();
        let (row, x) = self.layout.caret_on_line(self.doc.caret, self.line_hint);
        let mut destination = None;
        let mut destination_row = None;
        match event.logical_key {
            Key::Named(key @ (NamedKey::ArrowLeft | NamedKey::ArrowRight)) => {
                let forward = key == NamedKey::ArrowRight;
                destination = Some(
                    if !shift && !command && !option && !self.doc.selection().is_empty() {
                        if forward {
                            self.doc.selection().end
                        } else {
                            self.doc.selection().start
                        }
                    } else if command {
                        destination_row = Some(row);
                        if forward {
                            self.layout.lines[row].end
                        } else {
                            self.layout.lines[row].start
                        }
                    } else {
                        self.doc.adjacent(forward, option)
                    },
                );
                self.goal_x = None;
            }
            Key::Named(key @ (NamedKey::ArrowUp | NamedKey::ArrowDown)) => {
                let down = key == NamedKey::ArrowDown;
                destination = Some(if command {
                    if down { self.doc.rope.len_bytes() } else { 0 }
                } else {
                    let goal = *self.goal_x.get_or_insert(x);
                    if down && row + 1 >= self.layout.lines.len() {
                        self.doc.rope.len_bytes()
                    } else if !down && row == 0 {
                        0
                    } else {
                        let target_row = if down { row + 1 } else { row - 1 };
                        destination_row = Some(target_row);
                        self.layout.hit(target_row, goal)
                    }
                });
            }
            Key::Named(NamedKey::Home) => {
                destination = Some(0);
                self.goal_x = None;
            }
            Key::Named(NamedKey::End) => {
                destination = Some(self.doc.rope.len_bytes());
                self.goal_x = None;
            }
            Key::Named(NamedKey::Escape) => {
                if self.close_panel() || self.close_chart() {
                    return;
                }
                if self.doc.cancel() {
                    self.changed(true);
                }
            }
            Key::Named(key @ (NamedKey::Backspace | NamedKey::Delete)) => {
                let in_prompt = self.doc.open_range().is_some_and(|range| {
                    range.start <= self.doc.caret && self.doc.caret <= range.end
                });
                if key == NamedKey::Backspace
                    && !command
                    && !option
                    && !in_prompt
                    && let Some(pos) = self.doc.formula_at_caret(true)
                {
                    self.edit_math(pos);
                    return;
                }
                if self.doc.selection().is_empty() {
                    let forward = key == NamedKey::Delete;
                    self.doc.anchor = if command {
                        if forward {
                            self.layout.lines[row].end
                        } else {
                            self.layout.lines[row].start
                        }
                    } else {
                        self.doc.adjacent(forward, option)
                    };
                }
                self.doc.replace("", false);
                self.goal_x = None;
                self.changed(true);
            }
            Key::Named(NamedKey::Enter) if !command => {
                if self.doc.submit_prompt() {
                    self.changed(true);
                    return;
                }
                if let Some(pos) = self.doc.formula_at_caret(false) {
                    let formula = &self.doc.formulas[&pos];
                    if formula.prompt.is_some() && formula.failed && formula.image.is_none() {
                        self.doc.retry(pos);
                        self.changed(true);
                    } else {
                        self.edit_math(pos);
                    }
                    return;
                }
                match self
                    .doc
                    .selection()
                    .is_empty()
                    .then(|| markdown::enter_edit(&self.doc.text(), self.doc.caret))
                    .flatten()
                {
                    Some((range, value)) => {
                        self.doc.anchor = range.start;
                        self.doc.caret = range.end;
                        self.doc.replace(&value, false);
                    }
                    None => self.doc.replace("\n", false),
                }
                self.goal_x = None;
                self.changed(true);
            }
            Key::Named(NamedKey::Tab) if !command => {
                self.doc.replace("    ", false);
                self.goal_x = None;
                self.changed(true);
            }
            _ if !command && !self.modifiers.control_key() => {
                if let Some(text) = event.text.filter(|s| !s.chars().any(char::is_control)) {
                    self.text_input(&text);
                }
            }
            _ => {}
        }
        if let Some(destination) = destination {
            self.doc.move_to(destination, shift);
            self.line_hint = destination_row;
            self.changed(false);
        }
    }
    pub(super) fn text_input(&mut self, text: &str) {
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        if text.is_empty() {
            return;
        }
        if self.math.is_some() {
            self.math_input(text.chars().map(MathKey::Text));
            return;
        }
        // A typed backslash is ordinary prose.
        self.doc.replace(&text, true);
        self.goal_x = None;
        self.changed(true);
    }
    pub(super) fn mouse_caret(&mut self, extend: bool) {
        self.refresh_layout();
        let left = self.dimensions().3;
        let row = self.layout.row_at(self.mouse.1 + self.scroll - TOP);
        self.doc
            .move_to(self.layout.hit(row, self.mouse.0 - left), extend);
        self.line_hint = Some(row.min(self.layout.lines.len() - 1));
        self.goal_x = None;
        self.changed(false);
    }
}

/// The keys a key press means inside a formula. Command shortcuts other than the ones listed
/// are menu actions and map to nothing.
fn math_keys(event: &KeyEvent, modifiers: ModifiersState) -> Vec<MathKey> {
    let shift = modifiers.shift_key();
    let command = modifiers.super_key();
    let option = modifiers.alt_key();
    let control = modifiers.control_key();
    // Option types brackets and symbols on some layouts, so only the keys the map binds become Alt.
    let base = match event.key_without_modifiers() {
        Key::Character(base) => base.chars().next().map(|c| c.to_ascii_lowercase()),
        _ => None,
    };
    if option
        && !command
        && !control
        && let Some(key @ ('f' | 's')) = base
    {
        return vec![MathKey::Alt { key, shift }];
    }
    let key = match event.logical_key {
        Key::Named(NamedKey::Backspace) if command && option => MathKey::ExitLeft,
        Key::Named(NamedKey::Delete) if command && option => MathKey::ExitRight,
        Key::Named(NamedKey::ArrowLeft) if command && option => MathKey::PreviousArgument,
        Key::Named(NamedKey::ArrowRight) if command && option => MathKey::NextArgument,
        Key::Named(_) if command => return Vec::new(),
        Key::Named(NamedKey::Enter) => MathKey::Return,
        Key::Named(NamedKey::Escape) => MathKey::Escape,
        Key::Named(NamedKey::Tab) if shift => MathKey::ShiftTab,
        Key::Named(NamedKey::Tab) => MathKey::Tab,
        Key::Named(NamedKey::Backspace) if option => MathKey::StructuredBackspace,
        Key::Named(NamedKey::Backspace) if control => MathKey::Unwrap,
        Key::Named(NamedKey::Backspace) => MathKey::Backspace,
        Key::Named(NamedKey::Delete) => MathKey::Delete,
        Key::Named(NamedKey::ArrowLeft) => MathKey::Left,
        Key::Named(NamedKey::ArrowRight) => MathKey::Right,
        Key::Named(NamedKey::ArrowUp) => MathKey::Up,
        Key::Named(NamedKey::ArrowDown) => MathKey::Down,
        Key::Named(NamedKey::F5) if shift => MathKey::SymbolPrefix,
        Key::Named(NamedKey::Space) => MathKey::Text(' '),
        Key::Named(_) => return Vec::new(),
        _ if command || control => return Vec::new(),
        _ => {
            return event
                .text
                .as_deref()
                .unwrap_or("")
                .chars()
                .filter(|c| !c.is_control())
                .map(MathKey::Text)
                .collect();
        }
    };
    vec![key]
}
