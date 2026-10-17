//! Menu actions and file commands: save, export, and closing with unsaved changes.
use super::App;
use crate::{
    config::MATH_CACHE_LIMIT,
    document::Document,
    export, file,
    math::input::{MathInput, MathKey},
    math::{tree::MathTree, worker::Reply},
    menu::Action,
};
use winit::event_loop::ActiveEventLoop;
impl App {
    fn save(&mut self, save_as: bool) -> bool {
        self.doc.break_group();
        let path = if save_as || self.path.is_none() {
            file::choose_save(self.path.as_deref())
        } else {
            self.path.clone()
        };
        let Some(path) = path else { return false };
        match file::write(&path, self.doc.markdown()) {
            Ok(()) => {
                self.path = Some(path);
                self.doc.mark_saved();
                file::remember(self.path.as_deref());
                self.changed(false);
                true
            }
            Err(error) => {
                file::error("Could not save the document.", error);
                false
            }
        }
    }
    fn export_pdf(&mut self) {
        self.doc.break_group();
        let n = export::unrendered(&self.doc);
        if n > 0 && !file::confirm_unrendered(n) {
            return;
        }
        let Some(path) = file::choose_export(self.path.as_deref()) else {
            return;
        };
        if let Err(error) = export::pdf(&self.doc)
            .and_then(|bytes| file::write(&path, bytes).map_err(|e| e.to_string()))
        {
            file::error("Could not export the PDF.", error);
        }
    }
    pub(super) fn may_discard(&mut self) -> bool {
        if !self.doc.dirty() {
            return true;
        }
        use rfd::{MessageButtons, MessageDialog, MessageDialogResult};
        match MessageDialog::new()
            .set_title("Save changes?")
            .set_description("Your changes will be lost if you don’t save them.")
            .set_buttons(MessageButtons::YesNoCancelCustom(
                "Save".into(),
                "Don’t Save".into(),
                "Cancel".into(),
            ))
            .show()
        {
            MessageDialogResult::Custom(s) if s == "Save" => self.save(false),
            MessageDialogResult::Custom(s) if s == "Don’t Save" => true,
            MessageDialogResult::Yes => self.save(false),
            MessageDialogResult::No => true,
            _ => false,
        }
    }
    pub(super) fn action(&mut self, event_loop: &ActiveEventLoop, action: Action) {
        self.doc.break_group();
        self.goal_x = None;
        if self.math.is_some() {
            match action {
                Action::MathReady => {}
                Action::Undo | Action::Redo => {
                    let key = if matches!(action, Action::Undo) {
                        MathKey::Undo
                    } else {
                        MathKey::Redo
                    };
                    self.math_input([key]);
                    return;
                }
                Action::InsertFormula
                | Action::AskAiFormula
                | Action::Copy
                | Action::Cut
                | Action::Paste
                | Action::SelectAll => return,
                _ => self.finish_math(),
            }
        }
        match action {
            Action::MathReady => {
                let mut changed = false;
                while let Ok(reply) = self.worker.rx.try_recv() {
                    match reply {
                        Reply::Connection(online) => {
                            self.online = online;
                            changed = true;
                        }
                        Reply::Done(id, result) => {
                            if let Ok(image) = &result
                                && let Some(prompt) = self
                                    .doc
                                    .formulas
                                    .values()
                                    .find(|f| f.id == id)
                                    .and_then(|f| f.prompt.clone())
                            {
                                if self.math_cache.len() >= MATH_CACHE_LIMIT {
                                    self.math_cache.clear();
                                }
                                self.math_cache.insert(prompt, image.clone());
                            }
                            changed |= self.doc.complete(id, result);
                        }
                        Reply::PreviewDone(id, revision, result) => {
                            changed |= self.doc.complete_manual_preview(id, revision, result);
                        }
                    }
                }
                if changed {
                    self.changed(true);
                    self.reveal = false;
                }
            }
            Action::InsertFormula => {
                let in_prompt = self
                    .doc
                    .open_range()
                    .is_some_and(|r| r.start <= self.doc.caret && self.doc.caret <= r.end);
                if !in_prompt {
                    self.doc.open_manual();
                    self.math = Some(MathInput::new(MathTree::new()));
                    self.preview();
                }
            }
            Action::AskAiFormula => {
                let in_prompt = self
                    .doc
                    .open_range()
                    .is_some_and(|r| r.start <= self.doc.caret && self.doc.caret <= r.end);
                if !in_prompt {
                    self.doc.open_ai_prompt();
                    self.changed(true);
                }
            }
            Action::Quit => {
                if self.may_discard() {
                    event_loop.exit();
                }
            }
            Action::New => {
                if self.may_discard() {
                    self.doc = Document::new("");
                    self.path = None;
                    self.scroll = 0.0;
                    file::remember(None);
                    self.changed(true);
                }
            }
            Action::Open => {
                if !self.may_discard() {
                    return;
                }
                if let Some(path) = file::choose_open() {
                    match file::read(&path) {
                        Ok(text) => {
                            self.doc = Document::new(&text);
                            self.path = Some(path);
                            self.scroll = 0.0;
                            file::remember(self.path.as_deref());
                            self.changed(true);
                        }
                        Err(error) => file::error("Could not open the document.", error),
                    }
                }
            }
            Action::Save => {
                self.save(false);
            }
            Action::SaveAs => {
                self.save(true);
            }
            Action::ExportPdf => self.export_pdf(),
            Action::Undo | Action::Redo => {
                self.doc.undo(matches!(action, Action::Redo));
                self.changed(true);
            }
            Action::SelectAll => {
                self.doc.anchor = 0;
                self.doc.caret = self.doc.rope.len_bytes();
                self.changed(false);
            }
            Action::Copy | Action::Cut => {
                if !self.doc.selection().is_empty() {
                    let result = self
                        .clipboard
                        .as_mut()
                        .ok_or_else(|| "Clipboard unavailable".to_owned())
                        .and_then(|c| c.set_text(self.doc.selected()).map_err(|e| e.to_string()));
                    match result {
                        Ok(()) => {
                            if matches!(action, Action::Cut) {
                                self.doc.replace("", false);
                                self.changed(true);
                            }
                        }
                        Err(error) => file::error("Could not copy text.", error),
                    }
                }
            }
            Action::Paste => {
                let text = self.clipboard.as_mut().and_then(|c| c.get_text().ok());
                if let Some(text) = text {
                    self.doc
                        .paste(&text.replace("\r\n", "\n").replace('\r', "\n"));
                    self.changed(true);
                }
            }
        }
    }
}
