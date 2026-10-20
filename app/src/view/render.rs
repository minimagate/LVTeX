use crate::{
    config::*,
    view::layout::{Layout, Typesetter},
};
use cosmic_text::{Color, SwashCache};
use std::ops::Range;

#[allow(clippy::too_many_arguments)] // explicit pixel buffer and rectangle geometry
pub fn rect(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: u32,
) {
    let x0 = (x.floor() as i32).clamp(0, width as i32) as usize;
    let x1 = ((x + w).ceil() as i32).clamp(0, width as i32) as usize;
    let y0 = (y.floor() as i32).clamp(0, height as i32) as usize;
    let y1 = ((y + h).ceil() as i32).clamp(0, height as i32) as usize;
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    for row in y0..y1 {
        pixels[row * width + x0..row * width + x1].fill(color);
    }
}
/// Draws a layout's glyphs into `pixels`, with the layout's origin at (`left`, `top`) in logical pixels.
#[allow(clippy::too_many_arguments)] // the buffer, its size and the layout's origin are all inputs
pub fn paint(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    scale: f32,
    layout: &Layout,
    typesetter: &mut Typesetter,
    cache: &mut SwashCache,
    left: f32,
    top: f32,
) {
    for placed in &layout.glyphs {
        let baseline = top + placed.baseline;
        if baseline < -placed.glyph.font_size * 2.0
            || baseline > height as f32 / scale + placed.glyph.font_size * 2.0
        {
            continue;
        }
        let glyph = placed
            .glyph
            .physical(((left + placed.x) * scale, baseline * scale), scale);
        cache.with_pixels(
            &mut typesetter.fonts,
            glyph.cache_key,
            Color::rgb(
                (placed.color >> 16) as u8,
                (placed.color >> 8) as u8,
                placed.color as u8,
            ),
            |x, y, color| {
                let x = x + glyph.x;
                let y = y + glyph.y;
                if x >= 0 && y >= 0 && x < width as i32 && y < height as i32 {
                    let p = &mut pixels[y as usize * width + x as usize];
                    let a = color.a() as u32;
                    let r = (color.r() as u32 * a + ((*p >> 16) & 255) * (255 - a)) / 255;
                    let g = (color.g() as u32 * a + ((*p >> 8) & 255) * (255 - a)) / 255;
                    let b = (color.b() as u32 * a + (*p & 255) * (255 - a)) / 255;
                    *p = r << 16 | g << 8 | b;
                }
            },
        );
    }
}
#[allow(clippy::too_many_arguments)] // viewport and selection belong to the frame, not persistent state
pub fn page(
    layout: &Layout,
    typesetter: &mut Typesetter,
    cache: &mut SwashCache,
    width: usize,
    height: usize,
    scale: f32,
    left: f32,
    scroll: f32,
    selection: Range<usize>,
) -> Vec<u32> {
    let mut pixels = vec![BACKGROUND; width * height];
    let top = TOP - scroll;
    if !selection.is_empty() {
        for line in &layout.lines {
            let y = top + line.top;
            if y + line.height < 0.0 || y > height as f32 / scale {
                continue;
            }
            let start = selection.start.max(line.start);
            let end = selection.end.min(line.end);
            if start <= end && selection.start <= line.end && selection.end > line.start {
                let x0 = line
                    .stops
                    .iter()
                    .find(|p| p.0 == start)
                    .map(|p| p.1)
                    .unwrap_or(0.0);
                let x1 = line
                    .stops
                    .iter()
                    .find(|p| p.0 == end)
                    .map(|p| p.1)
                    .unwrap_or(x0)
                    + if selection.end > line.end { 5.0 } else { 0.0 };
                rect(
                    &mut pixels,
                    width,
                    height,
                    (left + x0) * scale,
                    y * scale,
                    (x1 - x0) * scale,
                    line.height * scale,
                    SELECTION_COLOR,
                );
            }
        }
    }
    paint(
        &mut pixels,
        width,
        height,
        scale,
        layout,
        typesetter,
        cache,
        left,
        top,
    );
    for placed in &layout.math {
        let image = if placed.display {
            &placed.image.display
        } else {
            &placed.image.inline
        };
        if image.width == 0 || image.height == 0 {
            continue;
        }
        let x0 = ((left + placed.x) * scale).round() as i32;
        let y0 = ((top + placed.y) * scale).round() as i32;
        let factor = scale * placed.scale / 2.0;
        let w = (image.width as f32 * factor).ceil() as i32;
        let h = (image.height as f32 * factor).ceil() as i32;
        for dy in 0..h {
            let y = y0 + dy;
            if y < 0 || y >= height as i32 {
                continue;
            }
            let sy = ((dy as f32 / factor) as usize).min(image.height - 1);
            for dx in 0..w {
                let x = x0 + dx;
                if x < 0 || x >= width as i32 {
                    continue;
                }
                let sx = ((dx as f32 / factor) as usize).min(image.width - 1);
                let rgba = &image.rgba[(sy * image.width + sx) * 4..][..4];
                let a = rgba[3] as u32;
                let p = &mut pixels[y as usize * width + x as usize];
                // Typst pixels are already premultiplied.
                let r = rgba[0] as u32 + ((*p >> 16) & 255) * (255 - a) / 255;
                let g = rgba[1] as u32 + ((*p >> 8) & 255) * (255 - a) / 255;
                let b = rgba[2] as u32 + (*p & 255) * (255 - a) / 255;
                *p = r << 16 | g << 8 | b;
            }
        }
    }
    for mark in &layout.marks {
        rect(
            &mut pixels,
            width,
            height,
            (left + mark.x) * scale,
            (top + mark.y) * scale,
            mark.width * scale,
            scale.max(1.0),
            mark.color,
        );
    }
    pixels
}
/// An anti-aliased filled circle centred at (`x`, `y`), in physical pixels, blended over the buffer with `alpha` in 0..=1.
#[allow(clippy::too_many_arguments)] // mirrors `rect`: buffer, size, geometry, colour and alpha
pub fn dot(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    radius: f32,
    color: u32,
    alpha: f32,
) {
    let x0 = ((x - radius - 1.0).floor() as i32).clamp(0, width as i32) as usize;
    let x1 = ((x + radius + 1.0).ceil() as i32).clamp(0, width as i32) as usize;
    let y0 = ((y - radius - 1.0).floor() as i32).clamp(0, height as i32) as usize;
    let y1 = ((y + radius + 1.0).ceil() as i32).clamp(0, height as i32) as usize;
    for row in y0..y1 {
        for col in x0..x1 {
            let dx = col as f32 + 0.5 - x;
            let dy = row as f32 + 0.5 - y;
            let distance = (dx * dx + dy * dy).sqrt();
            let coverage = (radius + 0.5 - distance).clamp(0.0, 1.0) * alpha;
            blend(&mut pixels[row * width + col], color, coverage);
        }
    }
}
/// An anti-aliased filled rectangle with rounded corners, in physical pixels.
#[allow(clippy::too_many_arguments)] // mirrors `rect`, plus the corner radius
pub fn rounded_rect(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radius: f32,
    color: u32,
) {
    let x0 = (x.floor() as i32).clamp(0, width as i32) as usize;
    let x1 = ((x + w).ceil() as i32).clamp(0, width as i32) as usize;
    let y0 = (y.floor() as i32).clamp(0, height as i32) as usize;
    let y1 = ((y + h).ceil() as i32).clamp(0, height as i32) as usize;
    // Corners wider than half the short side would overlap, so the radius is capped there.
    let radius = radius.min(w.min(h) / 2.0).max(0.0);
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let (hx, hy) = (w / 2.0 - radius, h / 2.0 - radius);
    for row in y0..y1 {
        for col in x0..x1 {
            // Signed distance from the pixel centre to the rounded box: negative inside.
            let qx = (col as f32 + 0.5 - cx).abs() - hx;
            let qy = (row as f32 + 0.5 - cy).abs() - hy;
            let outside = qx.max(0.0).hypot(qy.max(0.0));
            let inside = qx.max(qy).min(0.0);
            let distance = outside + inside - radius;
            let coverage = (0.5 - distance).clamp(0.0, 1.0);
            blend(&mut pixels[row * width + col], color, coverage);
        }
    }
}
// Mixes `color` into `pixel` by `coverage`, per channel, rounded to the nearest value.
fn blend(pixel: &mut u32, color: u32, coverage: f32) {
    let coverage = coverage.clamp(0.0, 1.0);
    let old = *pixel;
    let mix = |shift: u32| {
        let (o, n) = ((old >> shift) & 255, (color >> shift) & 255);
        (o as f32 + (n as f32 - o as f32) * coverage).round() as u32
    };
    *pixel = mix(16) << 16 | mix(8) << 8 | mix(0);
}

#[cfg(test)]
#[path = "../../tests/unit/view/render.rs"]
mod tests;
