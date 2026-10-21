use super::*;

const BLACK: u32 = 0x000000;
const WHITE: u32 = 0xFFFFFF;
const TINT: u32 = 0x2F7BFF;

#[test]
fn dot_fills_its_centre_and_leaves_far_pixels() {
    let (w, h) = (12, 12);
    let mut px = vec![BLACK; w * h];
    dot(&mut px, w, h, 6.0, 6.0, 3.0, TINT, 1.0);
    assert_eq!(px[6 * w + 6], TINT);
    // Inside the bounding box but beyond the radius, and far outside.
    assert_eq!(px[2 * w + 2], BLACK);
    assert_eq!(px[11 * w + 11], BLACK);
}

#[test]
fn dot_edge_is_partially_covered() {
    let (w, h) = (12, 12);
    let mut px = vec![BLACK; w * h];
    dot(&mut px, w, h, 6.0, 6.0, 3.0, WHITE, 1.0);
    // The pixel centre (8.5, 6.5) lies 2.55 from the centre: coverage 0.95.
    let edge = px[6 * w + 8];
    assert!(edge > BLACK && edge < WHITE, "{edge:#08x}");
}

#[test]
fn dot_alpha_scales_the_centre() {
    let (w, h) = (12, 12);
    let mut px = vec![BLACK; w * h];
    dot(&mut px, w, h, 6.0, 6.0, 3.0, WHITE, 0.5);
    assert_eq!(px[6 * w + 6], 0x808080);
}

#[test]
fn dot_partly_or_wholly_off_the_buffer_does_not_panic() {
    let (w, h) = (6, 6);
    let mut px = vec![BLACK; w * h];
    dot(&mut px, w, h, -1.0, -1.0, 3.0, WHITE, 1.0);
    assert_eq!(px[0], WHITE);
    dot(&mut px, w, h, 7.5, 2.0, 4.0, WHITE, 1.0);
    dot(&mut px, w, h, -50.0, -50.0, 3.0, WHITE, 1.0);
    dot(&mut px, w, h, 3.0, 300.0, 3.0, WHITE, 1.0);
}

#[test]
fn rounded_rect_fills_its_centre_and_edge_midpoints() {
    let (w, h) = (60, 60);
    let mut px = vec![BLACK; w * h];
    rounded_rect(&mut px, w, h, 10.0, 10.0, 40.0, 40.0, 10.0, WHITE);
    assert_eq!(px[30 * w + 30], WHITE);
    // Edge midpoints lie half a pixel inside the box, so they are fully covered.
    assert_eq!(px[30 * w + 10], WHITE);
    assert_eq!(px[10 * w + 30], WHITE);
    assert_eq!(px[30 * w + 49], WHITE);
    assert_eq!(px[49 * w + 30], WHITE);
}

#[test]
fn rounded_rect_corner_is_cut_away() {
    let (w, h) = (60, 60);
    let mut px = vec![BLACK; w * h];
    rounded_rect(&mut px, w, h, 10.0, 10.0, 40.0, 40.0, 10.0, WHITE);
    // The corner of the bounding box lies outside the arc.
    assert_eq!(px[10 * w + 10], BLACK);
    // A pixel on the arc is partially covered: its centre is 0.08 px inside the arc.
    let arc = px[13 * w + 12];
    assert!(arc > BLACK && arc < WHITE, "{arc:#08x}");
}

#[test]
fn rounded_rect_clipped_by_the_buffer_does_not_panic() {
    let (w, h) = (10, 10);
    let mut px = vec![BLACK; w * h];
    rounded_rect(&mut px, w, h, -5.0, -5.0, 20.0, 20.0, 4.0, WHITE);
    assert_eq!(px[0], WHITE);
    rounded_rect(&mut px, w, h, 8.0, 8.0, 50.0, 50.0, 500.0, WHITE);
    rounded_rect(&mut px, w, h, 20.0, 20.0, 5.0, 5.0, 2.0, WHITE);
    assert_eq!(px[9 * w + 9], WHITE);
}

#[test]
fn blend_with_zero_coverage_keeps_the_pixel() {
    let mut pixel = 0x123456;
    blend(&mut pixel, WHITE, 0.0);
    assert_eq!(pixel, 0x123456);
}

#[test]
fn blend_with_full_coverage_takes_the_colour() {
    let mut pixel = 0x123456;
    blend(&mut pixel, TINT, 1.0);
    assert_eq!(pixel, TINT);
}

#[test]
fn line_takes_the_colour_on_its_centre_and_leaves_pixels_far_away() {
    let (w, h) = (12, 12);
    let mut px = vec![BLACK; w * h];
    // The line runs through the centres of row 6, so those pixels are fully covered.
    line(&mut px, w, h, 1.5, 6.5, 10.5, 6.5, 1.0, TINT);
    assert_eq!(px[6 * w + 6], TINT);
    // Row 1's centre lies 5 px from the line.
    assert_eq!(px[w + 6], BLACK);
}

#[test]
fn line_partly_or_wholly_off_the_buffer_does_not_panic() {
    let (w, h) = (6, 6);
    let mut px = vec![BLACK; w * h];
    line(&mut px, w, h, -10.0, -10.0, 20.0, 20.0, 1.0, WHITE);
    // The diagonal passes through the centre of pixel (2, 2).
    assert_eq!(px[2 * w + 2], WHITE);
    line(&mut px, w, h, 50.0, 50.0, 80.0, 2.0, 2.0, WHITE);
    line(&mut px, w, h, 3.0, 300.0, 3.0, 300.0, 2.0, WHITE);
    line(&mut px, w, h, -40.0, 2.0, -30.0, 2.0, 2.0, WHITE);
}
