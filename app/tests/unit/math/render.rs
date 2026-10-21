use super::*;

#[test]
fn hand_written_caret_is_located_erased_and_scales_with_script_size() {
    let caret = format!(r"\mathord{{\textcolor{{#{CARET_SENTINEL:06X}}}{{|}}}}");
    let mut renderer = Renderer::with_prelude(lvtex_latex::PRELUDE);
    let base = render_preview(&mut renderer, &format!(r"x{caret}")).unwrap();
    let script = render_preview(&mut renderer, &format!(r"x^{{2{caret}}}")).unwrap();
    let plain = render_preview(&mut renderer, "x^2").unwrap();
    for image in [&base.inline, &base.display, &script.inline, &script.display] {
        assert!(
            image
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| !is_sentinel(pixel)),
            "sentinel pixels remain"
        );
    }
    let base_box = base.inline.caret.expect("base caret");
    let script_box = script.inline.caret.expect("script caret");
    let prose = crate::view::layout::Typesetter::new()
        .layout("A", 680.0)
        .caret_extent(0, 1)
        .1;
    assert!(
        (base_box.height - prose).abs() <= 0.5,
        "base math caret {} vs prose cap caret {prose}",
        base_box.height
    );
    assert!(
        (base_box.top + base_box.height - base.inline.baseline).abs() <= 0.5,
        "caret bottom must sit on the baseline"
    );
    for (b, image) in [(base_box, &base.inline), (script_box, &script.inline)] {
        assert!(b.x >= 0.0 && b.x <= image.width());
        assert!(b.top >= 0.0 && b.top + b.height <= image.height());
    }
    assert!(
        script_box.height < base_box.height,
        "script {} vs base {}",
        script_box.height,
        base_box.height
    );
    assert!(plain.inline.caret.is_none() && plain.display.caret.is_none());
}

#[test]
fn magenta_ink_survives_final_renders_and_previews_still_find_the_caret() {
    let magenta = r"\textcolor{magenta}{x}+1";
    let caret = format!(r"\mathord{{\textcolor{{#{CARET_SENTINEL:06X}}}{{|}}}}");
    let magenta_pixels = |image: &Image| {
        image
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| **p == [255, 0, 255, 255])
            .count()
    };
    let mut renderer = Renderer::with_prelude(lvtex_latex::PRELUDE);
    let plain = render(&mut renderer, magenta).unwrap();
    let expected = magenta_pixels(&plain.inline);
    assert!(expected > 0, "magenta formula has no magenta ink");

    let with_marker = render(&mut renderer, &format!("{magenta}{caret}")).unwrap();
    assert!(
        with_marker.inline.caret.is_none(),
        "final render found a caret"
    );
    assert!(
        magenta_pixels(&with_marker.inline) > 0,
        "final render erased magenta ink"
    );

    let preview = render_preview(&mut renderer, &format!("{magenta}{caret}")).unwrap();
    assert!(preview.inline.caret.is_some(), "preview lost the caret");
    assert!(
        magenta_pixels(&preview.inline) > 0,
        "preview erased magenta ink"
    );
}
