use super::*;
use crate::{
    math::render::{CaretBox, Image},
    view::layout::PlacedMath,
};

fn formula(caret: Option<CaretBox>) -> Arc<FormulaImage> {
    let bitmap = || Image {
        rgba: vec![],
        width: 0,
        height: 0,
        baseline: 0.0,
        caret,
    };
    Arc::new(FormulaImage {
        latex: "x".into(),
        inline: bitmap(),
        display: bitmap(),
    })
}

#[test]
fn formula_caret_maps_the_placed_caret_box_or_finds_nothing() {
    let image = formula(Some(CaretBox {
        x: 2.0,
        top: 3.0,
        height: 10.0,
    }));
    let layout = Layout {
        math: vec![PlacedMath {
            image: Arc::clone(&image),
            display: false,
            x: 10.0,
            y: 20.0,
            scale: 2.0,
        }],
        ..Layout::default()
    };
    // left + x + caret.x * scale, top + y + caret.top * scale, caret.height * scale.
    assert_eq!(
        formula_caret(&layout, &image, 5.0, 100.0),
        Some((19.0, 126.0, 20.0))
    );
    let other = formula(None);
    assert_eq!(formula_caret(&layout, &other, 5.0, 100.0), None);
    assert_eq!(formula_caret(&Layout::default(), &image, 5.0, 100.0), None);
}
