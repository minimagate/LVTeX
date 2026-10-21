//! Settings: local writing with Ollama, and Lean checking through OpenRouter with its key and switch.
use super::mls::contains;
use super::*;
use crate::keychain;
use winit::{
    event::KeyEvent,
    keyboard::{Key, NamedKey},
};

/// The open Settings window. Areas are (x, y, w, h) in logical window coordinates, as the last draw
/// left them. The draft is the key being typed: it is shown only as bullets, and it goes with the
/// window when the window closes.
pub(super) struct Settings {
    draft: String,
    stored: bool,
    message: Option<String>,
    area: (f32, f32, f32, f32),
    switch: Option<(f32, f32, f32, f32)>,
    remove: Option<(f32, f32, f32, f32)>,
}

/// One row of the panel: the space above it, its height, its text, and what else it draws.
struct Row {
    gap: f32,
    height: f32,
    layout: Layout,
    kind: Kind,
}

/// What a row draws besides its text.
#[derive(Clone, Copy)]
enum Kind {
    Text,
    Switch(bool),
    Field,
    Remove,
}

/// The key MLS uses: OPENROUTER_API_KEY when it is set and not blank, else the Keychain's.
pub(super) fn effective_key() -> Option<String> {
    environment_key().or_else(keychain::read)
}

/// OPENROUTER_API_KEY when it is set and not blank. It takes precedence over the Keychain.
fn environment_key() -> Option<String> {
    std::env::var("OPENROUTER_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
}

/// What the key field shows for a draft: one bullet per character, up to SETTINGS_BULLETS.
fn bullets(draft: &str) -> String {
    "•".repeat(draft.chars().count().min(SETTINGS_BULLETS))
}

/// The text without its whitespace and control characters: what a typed or pasted key adds.
fn typed(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect()
}

/// The rows of the panel, top to bottom, with text laid out at the panel's inner width.
fn settings_rows(
    typesetter: &mut Typesetter,
    settings: &Settings,
    online: bool,
    on: bool,
    inner: f32,
) -> Vec<Row> {
    let field_inner = (inner - 2.0 * SETTINGS_FIELD_INSET_X).max(0.0);
    let dots = bullets(&settings.draft);
    let field = if dots.is_empty() {
        typesetter.plain(
            "Paste a new key (⌘V), then press Return",
            field_inner,
            false,
            PANEL_TEXT_SCALE,
            MARKER_COLOR,
        )
    } else {
        typesetter.plain(&dots, field_inner, true, PANEL_CODE_SCALE, TEXT_COLOR)
    };
    let field_line = typesetter
        .plain("M", field_inner, false, PANEL_TEXT_SCALE, 0)
        .height();
    let key = if environment_key().is_some() {
        "Key: OPENROUTER_API_KEY from the environment (it takes precedence)."
    } else if settings.stored {
        "Key: saved in your Keychain."
    } else {
        "Key: none yet."
    };
    let ai = format!(
        "AI formulas (⇧⌘1) use {OLLAMA_MODEL} through Ollama at {OLLAMA_ADDRESS}. Nothing is sent anywhere else."
    );
    let running = if online {
        "Ollama is running."
    } else {
        "Ollama is not running."
    };
    let label_width = (inner - SETTINGS_SWITCH_W - PANEL_GAP).max(0.0);

    let mut rows = Vec::new();
    let mut push = |gap: f32, layout: Layout, kind: Kind| {
        let height = match kind {
            Kind::Switch(_) => layout.height().max(SETTINGS_SWITCH_H),
            Kind::Field => layout.height().max(field_line) + 2.0 * SETTINGS_FIELD_INSET_Y,
            Kind::Text | Kind::Remove => layout.height(),
        };
        rows.push(Row {
            gap,
            height,
            layout,
            kind,
        });
    };
    push(
        0.0,
        typesetter.plain("Settings", inner, false, 1.0, TEXT_COLOR),
        Kind::Text,
    );
    push(
        PANEL_SECTION_GAP,
        typesetter.plain(
            "Writing · on this Mac",
            inner,
            false,
            PANEL_CODE_SCALE,
            MARKER_COLOR,
        ),
        Kind::Text,
    );
    push(
        PANEL_GAP,
        typesetter.plain(&ai, inner, false, PANEL_TEXT_SCALE, QUOTE_COLOR),
        Kind::Text,
    );
    push(
        PANEL_GAP,
        typesetter.plain(running, inner, false, PANEL_TEXT_SCALE, MARKER_COLOR),
        Kind::Text,
    );
    push(
        PANEL_SECTION_GAP,
        typesetter.plain(
            "Lean checking · OpenRouter",
            inner,
            false,
            PANEL_CODE_SCALE,
            MARKER_COLOR,
        ),
        Kind::Text,
    );
    push(
        PANEL_GAP,
        typesetter.plain(
            "Check definitions, statements and proofs with Lean",
            label_width,
            false,
            PANEL_TEXT_SCALE,
            TEXT_COLOR,
        ),
        Kind::Switch(on),
    );
    push(
        PANEL_GAP,
        typesetter.plain(
            "Jev labels each paragraph and Claude Haiku writes the Lean, through OpenRouter. Lean itself runs on this Mac.",
            inner,
            false,
            PANEL_TEXT_SCALE,
            QUOTE_COLOR,
        ),
        Kind::Text,
    );
    push(
        PANEL_GAP,
        typesetter.plain(key, inner, false, PANEL_TEXT_SCALE, MARKER_COLOR),
        Kind::Text,
    );
    push(PANEL_GAP, field, Kind::Field);
    if settings.stored {
        push(
            PANEL_GAP,
            typesetter.plain(
                "Remove key from Keychain",
                inner,
                false,
                PANEL_CODE_SCALE,
                MARKER_COLOR,
            ),
            Kind::Remove,
        );
    }
    if let Some(message) = &settings.message {
        push(
            PANEL_GAP,
            typesetter.plain(message, inner, false, PANEL_TEXT_SCALE, TEXT_COLOR),
            Kind::Text,
        );
    }
    push(
        PANEL_SECTION_GAP,
        typesetter.plain(
            "Escape closes.",
            inner,
            false,
            PANEL_CODE_SCALE,
            MARKER_COLOR,
        ),
        Kind::Text,
    );
    rows
}

impl App {
    /// Opens Settings, or closes it when it is open.
    pub(super) fn toggle_settings(&mut self) {
        if self.close_settings() {
            return;
        }
        self.settings = Some(Settings {
            draft: String::new(),
            stored: keychain::read().is_some(),
            message: None,
            area: (0.0, 0.0, 0.0, 0.0),
            switch: None,
            remove: None,
        });
        self.repaint();
    }

    /// Closes Settings, dropping the typed draft. Returns false when it was not open.
    pub(super) fn close_settings(&mut self) -> bool {
        if self.settings.take().is_none() {
            return false;
        }
        self.repaint();
        true
    }

    /// True when (x, y) lies inside the open Settings panel, in logical window coordinates.
    pub(super) fn settings_covers(&self, x: f32, y: f32) -> bool {
        self.settings
            .as_ref()
            .is_some_and(|settings| contains(settings.area, x, y))
    }

    /// A key pressed while Settings is open. Escape closes, Return saves, Backspace deletes, and
    /// typed characters join the draft. Nothing reaches the document.
    pub(super) fn settings_key(&mut self, event: &KeyEvent) {
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => {
                self.close_settings();
            }
            Key::Named(NamedKey::Enter) => self.save_key(),
            Key::Named(NamedKey::Backspace) => {
                if let Some(settings) = self.settings.as_mut() {
                    settings.draft.pop();
                }
                self.repaint();
            }
            _ => {
                // A command or control chord is a menu action, and must not type its letter.
                if self.modifiers.super_key() || self.modifiers.control_key() {
                    return;
                }
                let Some(text) = event.text.as_deref().map(typed) else {
                    return;
                };
                if let Some(settings) = self.settings.as_mut() {
                    settings.draft.push_str(&text);
                }
                self.repaint();
            }
        }
    }

    /// Pastes the clipboard into the key draft, without its whitespace.
    pub(super) fn paste_key(&mut self) {
        let text = self
            .clipboard
            .as_mut()
            .and_then(|clipboard| clipboard.get_text().ok());
        if let (Some(text), Some(settings)) = (text, self.settings.as_mut()) {
            settings.draft.push_str(&typed(&text));
        }
        self.repaint();
    }

    /// A click while Settings is open. The switch and Remove act, and a click outside the panel
    /// closes it. Every click is used, so none reaches the page.
    pub(super) fn settings_click(&mut self) {
        let (x, y) = self.mouse;
        let Some(settings) = self.settings.as_ref() else {
            return;
        };
        let on_switch = settings.switch.is_some_and(|rect| contains(rect, x, y));
        let on_remove = settings.remove.is_some_and(|rect| contains(rect, x, y));
        let inside = contains(settings.area, x, y);
        if on_switch {
            self.set_mls_on(!self.mls_on);
        } else if on_remove {
            self.remove_key();
        } else if !inside {
            self.close_settings();
        }
    }

    /// Saves the typed key to the Keychain and makes it the key MLS uses. Return on an empty draft
    /// does nothing.
    fn save_key(&mut self) {
        let Some(draft) = self
            .settings
            .as_ref()
            .map(|settings| settings.draft.trim().to_owned())
        else {
            return;
        };
        if draft.is_empty() {
            return;
        }
        if !lvtex_mls::key_is_valid(&draft) {
            self.set_message("That does not look like an OpenRouter key.");
            return;
        }
        let environment = environment_key();
        let outcome = keychain::store(&draft);
        let message = match &outcome {
            Ok(()) if environment.is_some() => {
                "Saved in your Keychain. OPENROUTER_API_KEY is set and takes precedence.".to_owned()
            }
            Ok(()) => "Saved in your Keychain.".to_owned(),
            Err(reason) => format!("{reason} The key is used until LVTeX quits."),
        };
        // The environment's key still comes first. Otherwise MLS uses the typed key, stored or not.
        self.mls_worker.set_key(environment.or(Some(draft)));
        if let Some(settings) = self.settings.as_mut() {
            settings.draft.clear();
            if outcome.is_ok() {
                settings.stored = true;
            }
            settings.message = Some(message);
        }
        self.retry_mls();
        self.repaint();
    }

    /// Removes the stored key, then makes MLS use the environment's key, if there is one.
    fn remove_key(&mut self) {
        let outcome = keychain::remove();
        let message = match &outcome {
            Ok(()) => "Removed from your Keychain.".to_owned(),
            Err(reason) => reason.clone(),
        };
        self.mls_worker.set_key(effective_key());
        if let Some(settings) = self.settings.as_mut() {
            if outcome.is_ok() {
                settings.stored = false;
            }
            settings.message = Some(message);
        }
        self.retry_mls();
        self.repaint();
    }

    /// After the key changes, MLS tries again, so a failed check or request runs once more.
    fn retry_mls(&mut self) {
        if self.mls_on {
            self.mls.retry();
            self.sync_mls();
        }
    }

    fn set_message(&mut self, message: &str) {
        if let Some(settings) = self.settings.as_mut() {
            settings.message = Some(message.to_owned());
        }
        self.repaint();
    }

    /// Paints the Settings panel over the page, centred under the title bar, and records its areas.
    pub(super) fn draw_settings(&mut self, width: usize, height: usize, scale: f32) {
        let Some(settings) = self.settings.as_ref() else {
            return;
        };
        let view_width = width as f32 / scale;
        let view_height = height as f32 / scale;
        let w = SETTINGS_WIDTH.min(view_width - 2.0 * PANEL_EDGE).max(0.0);
        let x = ((view_width - w) / 2.0).max(0.0);
        let y = CHART_TOP;
        let inner = (w - 2.0 * PANEL_PADDING).max(0.0);
        let rows = settings_rows(
            &mut self.typesetter,
            settings,
            self.online,
            self.mls_on,
            inner,
        );
        let total: f32 = rows.iter().map(|row| row.gap + row.height).sum();
        let h = (2.0 * PANEL_PADDING + total)
            .min(view_height - y - PANEL_EDGE)
            .max(0.0);
        render::rounded_rect(
            &mut self.pixels,
            width,
            height,
            x * scale,
            y * scale,
            w * scale,
            h * scale,
            PANEL_RADIUS * scale,
            PANEL_COLOR,
        );

        // Rows that do not fit in the panel are not drawn, and then none below them is.
        let limit = y + h - PANEL_PADDING;
        let left = x + PANEL_PADDING;
        let mut top = y + PANEL_PADDING;
        let (mut switch, mut remove) = (None, None);
        for row in &rows {
            let row_top = top + row.gap;
            if row_top + row.height > limit {
                break;
            }
            top = row_top + row.height;
            let text_top = row_top + (row.height - row.layout.height()) / 2.0;
            let text_left = match row.kind {
                Kind::Text => left,
                Kind::Remove => {
                    remove = Some((left, row_top, inner, row.height));
                    left
                }
                Kind::Switch(on) => {
                    let radius = SETTINGS_SWITCH_H / 2.0;
                    let pill_top = row_top + (row.height - SETTINGS_SWITCH_H) / 2.0;
                    let colour = if on { DOT_VERIFIED } else { MARKER_COLOR };
                    render::rounded_rect(
                        &mut self.pixels,
                        width,
                        height,
                        left * scale,
                        pill_top * scale,
                        SETTINGS_SWITCH_W * scale,
                        SETTINGS_SWITCH_H * scale,
                        radius * scale,
                        colour,
                    );
                    let knob_x = if on {
                        left + SETTINGS_SWITCH_W - radius
                    } else {
                        left + radius
                    };
                    render::dot(
                        &mut self.pixels,
                        width,
                        height,
                        knob_x * scale,
                        (pill_top + radius) * scale,
                        (radius - SETTINGS_KNOB_INSET) * scale,
                        TEXT_COLOR,
                        1.0,
                    );
                    switch = Some((left, row_top, inner, row.height));
                    left + SETTINGS_SWITCH_W + PANEL_GAP
                }
                Kind::Field => {
                    render::rounded_rect(
                        &mut self.pixels,
                        width,
                        height,
                        left * scale,
                        row_top * scale,
                        inner * scale,
                        row.height * scale,
                        PANEL_RADIUS * scale,
                        SETTINGS_FIELD_COLOR,
                    );
                    left + SETTINGS_FIELD_INSET_X
                }
            };
            render::paint(
                &mut self.pixels,
                width,
                height,
                scale,
                &row.layout,
                &mut self.typesetter,
                &mut self.cache,
                text_left,
                text_top,
            );
        }
        if let Some(settings) = self.settings.as_mut() {
            settings.area = (x, y, w, h);
            settings.switch = switch;
            settings.remove = remove;
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/app/settings.rs"]
mod tests;
