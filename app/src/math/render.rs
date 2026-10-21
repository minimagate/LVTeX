//! Formula rendering: Typst-backed images of formulas and the hand-written caret marker in them.

use crate::config::*;
use lvtex_render::math::Renderer;

/// The hand-written caret's box in an image, in pt like `baseline`: the horizontal centre of its
/// stem, its top edge and its height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaretBox {
    pub x: f32,
    pub top: f32,
    pub height: f32,
}

#[derive(Debug)]
pub struct Image {
    pub rgba: Vec<u8>,
    pub width: usize,
    pub height: usize,
    pub baseline: f32,
    pub caret: Option<CaretBox>,
}
impl Image {
    pub fn width(&self) -> f32 {
        self.width as f32 / 2.0
    }
    pub fn height(&self) -> f32 {
        self.height as f32 / 2.0
    }
}
#[derive(Debug)]
pub struct FormulaImage {
    pub latex: String,
    pub inline: Image,
    pub display: Image,
}

/// Renders a formula for export, final images and generated math. Every pixel is drawn as ink, so a
/// formula may use any colour, including the caret sentinel's.
pub fn render(renderer: &mut Renderer, latex: &str) -> Result<FormulaImage, String> {
    render_formula(renderer, latex, false)
}

/// Renders a live preview of a formula being edited. It also extracts and erases the caret marker.
pub(super) fn render_preview(renderer: &mut Renderer, latex: &str) -> Result<FormulaImage, String> {
    render_formula(renderer, latex, true)
}

fn render_formula(
    renderer: &mut Renderer,
    latex: &str,
    with_caret: bool,
) -> Result<FormulaImage, String> {
    if latex.is_empty() || latex.len() > 16384 {
        return Err("empty or oversized expression".into());
    }
    let body = lvtex_latex::convert(latex)?;
    let mut image = |display| {
        let equation = if display {
            format!("$ {body} $")
        } else {
            format!("#math.equation(block: true, math.inline(${body}$))")
        };
        let pixels = renderer.render_source(format!(
            "#set page(width: auto, height: auto, margin: 2pt, fill: none)\n#set text(font: \"Libertinus Serif\", size: {FONT_SIZE}pt, fill: rgb(\"{TEXT_COLOR:06x}\"))\n#show math.equation: set text(font: \"New Computer Modern Math\")\n{}\n{equation}", lvtex_latex::PRELUDE), 2.0)?;
        let (width, height) = (pixels.width() as usize, pixels.height() as usize);
        let mut rgba = pixels.data().to_vec();
        let caret = with_caret.then(|| take_caret(&mut rgba, width)).flatten();
        Ok::<_, String>(Image {
            width,
            height,
            rgba,
            baseline: renderer.last_baseline as f32,
            caret,
        })
    };
    Ok(FormulaImage {
        latex: latex.into(),
        inline: image(false)?,
        display: image(true)?,
    })
}

/// Whether a premultiplied pixel is the caret sentinel: each channel is within one step of the
/// sentinel's channel scaled by the pixel's alpha, so anti-aliased edges still match. Pixels with
/// alpha below 4 are invisible and never match, which keeps faint text fringe out.
fn is_sentinel(pixel: &[u8]) -> bool {
    let alpha = pixel[3];
    if alpha < 4 {
        return false;
    }
    let scale = f32::from(alpha) / 255.0;
    let [_, r, g, b] = CARET_SENTINEL.to_be_bytes();
    [r, g, b]
        .into_iter()
        .zip(&pixel[..3])
        .all(|(channel, &value)| (f32::from(value) - f32::from(channel) * scale).abs() <= 1.0)
}

/// Where the math baseline falls inside the `|` bar, as a fraction of the bar's height from its top.
/// Measured in New Computer Modern Math at base size (FONT_SIZE): the bar runs from 2 pt to 21 pt and
/// the baseline is at 17 pt, so the bar sits from about -0.21 em to +0.79 em.
const BAR_BASELINE_FROM_TOP: f32 = 15.0 / 19.0;
/// The prose caret after "A" at base size, cap height to baseline, measured with `Typesetter`. The
/// math caret takes the same share of its bar's height, so a base-level math caret matches it.
const PROSE_CAP_CARET: f32 = 12.5;

/// Erases every sentinel pixel and returns the caret for the solid ones. Faint edge pixels are erased
/// but do not widen the box, so grey text fringe cannot move it. The caret sits on the bar's own
/// baseline with the prose cap-height proportion of the bar.
fn take_caret(rgba: &mut [u8], width: usize) -> Option<CaretBox> {
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (usize::MAX, 0, usize::MAX, 0);
    let mut solid = false;
    for (i, pixel) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        if !is_sentinel(pixel) {
            continue;
        }
        if pixel[3] >= 32 {
            let (x, y) = (i % width, i / width);
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
            solid = true;
        }
        pixel.fill(0);
    }
    // Pixels are 2 per pt, so a pixel index divided by 2 is pt.
    solid.then(|| {
        let bar_top = min_y as f32 / 2.0;
        let bar_height = (max_y + 1 - min_y) as f32 / 2.0;
        let bottom = bar_top + BAR_BASELINE_FROM_TOP * bar_height;
        let height = bar_height * PROSE_CAP_CARET / FONT_SIZE;
        CaretBox {
            x: (min_x + max_x + 1) as f32 / 4.0,
            top: bottom - height,
            height,
        }
    })
}

#[cfg(test)]
#[path = "../../tests/unit/math/render.rs"]
mod tests;
