//! The caret: one I-beam for prose, math and AI prompts, differing only in color.

use crate::{
    config::*,
    math::render::FormulaImage,
    view::{layout::PlacedMath, render::rect},
};

/// The caret's colour, per editing context. Every caret is the same I-beam; only the colour differs.
#[derive(Clone, Copy)]
pub enum CaretStyle {
    Prose,
    Math,
    Prompt,
}
impl CaretStyle {
    pub fn color(self) -> u32 {
        match self {
            Self::Prose => CARET_COLOR,
            Self::Math => MATH_CARET_COLOR,
            Self::Prompt => AI_CARET_COLOR,
        }
    }
}

// An I-beam caret: a stem centred on `x` with a serif at the top and at the bottom. `x`, `top`
// and `h` are logical units.
#[allow(clippy::too_many_arguments)] // mirrors `rect`, which it draws with
pub fn caret(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    x: f32,
    top: f32,
    h: f32,
    scale: f32,
    style: CaretStyle,
) {
    let color = style.color();
    let thick = scale.max(1.0);
    let stem = x * scale;
    // Each serif is 1 logical pt thick and reaches 1.5 pt either side of the stem.
    let (serif_x, serif_w) = (stem - 1.5 * scale, 3.0 * scale);
    rect(
        pixels,
        width,
        height,
        serif_x,
        top * scale,
        serif_w,
        thick,
        color,
    );
    rect(
        pixels,
        width,
        height,
        serif_x,
        (top + h) * scale - thick,
        serif_w,
        thick,
        color,
    );
    rect(
        pixels,
        width,
        height,
        stem - thick / 2.0,
        top * scale,
        thick,
        h * scale,
        color,
    );
}

/// The hand-written caret of a formula preview, in window units (x, top, height). `top` is the
/// page's vertical offset (`TOP - scroll`). None until the bitmap has a caret box.
pub fn formula_caret(
    placed: &PlacedMath,
    image: &FormulaImage,
    left: f32,
    top: f32,
) -> Option<(f32, f32, f32)> {
    let bitmap = if placed.display {
        &image.display
    } else {
        &image.inline
    };
    let caret = bitmap.caret?;
    // Same transform as `render::page`: image pt scale by the placed scale.
    Some((
        left + placed.x + caret.x * placed.scale,
        top + placed.y + caret.top * placed.scale,
        caret.height * placed.scale,
    ))
}
