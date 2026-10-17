//! The hand-written formula session: editing a formula, its keys, live preview and finish.
use super::App;
use crate::{
    config::MATH_CACHE_LIMIT,
    math::input::{MathInput, MathKey, Outcome},
    math::worker::{Preview, Reply},
};
impl App {
    pub(super) fn edit_math(&mut self, pos: usize) {
        if self.doc.reopen(pos) {
            self.changed(true);
        } else if let Some(latex) = self.doc.reopen_manual(pos) {
            self.math = Some(MathInput::from_latex(&latex));
            self.preview();
        }
    }
    /// Feeds keys to the open formula. Edits and moves refresh the live image.
    pub(super) fn math_input(&mut self, keys: impl IntoIterator<Item = MathKey>) {
        for key in keys {
            let Some(input) = self.math.as_mut() else {
                return;
            };
            match input.key(key) {
                Outcome::Edited | Outcome::Moved => self.preview(),
                Outcome::Close => {
                    self.finish_math();
                    return;
                }
                Outcome::Cancel => {
                    if let Some(id) = self.doc.manual_id() {
                        self.worker.cancel_preview(id);
                    }
                    self.math = None;
                    self.doc.cancel_manual();
                    self.changed(true);
                    return;
                }
                Outcome::Ignored => {}
            }
        }
    }
    /// Stores the source immediately and queues a latest-wins render of its live preview.
    pub(super) fn preview(&mut self) {
        let Some(input) = &self.math else {
            return;
        };
        let latex = input.latex();
        let display_latex = input.display_latex();
        if let Some((id, revision)) = self.doc.update_manual(latex) {
            self.worker.preview(Preview {
                id,
                revision,
                display_latex,
            });
        }
        self.changed(true);
    }
    /// Stores the formula's LaTeX, lets the worker render it, and closes the edit.
    pub(super) fn finish_math(&mut self) {
        let Some(input) = self.math.take() else {
            return;
        };
        if let Some(id) = self.doc.manual_id() {
            self.worker.cancel_preview(id);
        }
        self.doc.update_manual(input.finish());
        self.doc.finish_manual();
        self.changed(true);
    }
    /// Applies the math worker's replies: connection state, finished formulas, the image cache and previews.
    pub(super) fn apply_math_replies(&mut self) {
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
}
