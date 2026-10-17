use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

mod codec;
mod editing;
mod manual;
mod prompt;

fn type_text(d: &mut Document, s: &str) {
    for c in s.chars() {
        d.replace(&c.to_string(), true);
    }
}
fn assert_round_trip(d: &Document) {
    let reopened = Document::new(&d.markdown());
    assert_eq!(reopened.text(), d.text());
    assert_eq!(reopened.formulas.len(), d.formulas.len());
    for (a, b) in d.formulas.values().zip(reopened.formulas.values()) {
        assert_eq!(a.prompt, b.prompt);
        assert_eq!(a.latex, b.latex);
    }
    assert!(!reopened.dirty());
}
fn pending_comment(prompt: &str) -> String {
    format!(
        "<!--mathstral:pending:{}-->",
        URL_SAFE_NO_PAD.encode(prompt)
    )
}
fn image(latex: &str) -> Arc<FormulaImage> {
    let pixels = || crate::math::render::Image {
        rgba: vec![],
        width: 0,
        height: 0,
        baseline: 0.0,
        caret: None,
    };
    Arc::new(FormulaImage {
        latex: latex.into(),
        inline: pixels(),
        display: pixels(),
    })
}

impl Document {
    pub fn set_manual(&mut self, latex: String, image: Option<Arc<FormulaImage>>) {
        let Some((id, _)) = self.update_manual(latex) else {
            return;
        };
        if let Some(formula) = self.formulas.values_mut().find(|formula| formula.id == id) {
            formula.image = image;
            formula.failed = false;
        }
    }
}
