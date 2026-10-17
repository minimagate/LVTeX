use super::*;

#[test]
fn sqrt_and_nth_root_with_up_and_down() {
    let sqrt = build(|t| {
        t.make_sqrt();
        sym(t, "2");
    });
    assert_eq!(sqrt.latex(), r"\sqrt{2}");

    let mut root = build(|t| {
        t.make_root();
        sym(t, "3");
        t.down();
        sym(t, "x");
    });
    assert_eq!(root.latex(), r"\sqrt[3]{x}");
    root.up();
    sym(&mut root, "4");
    assert_eq!(root.latex(), r"\sqrt[34]{x}");
    root.down();
    assert_eq!((root.path.clone(), root.pos), (vec![(0, 1)], 0));
}

#[test]
fn up_and_down_use_the_innermost_stacked_box() {
    let mut t = build(|t| {
        t.make_fraction();
        t.make_fraction();
        sym(t, "a");
        t.down();
        sym(t, "b");
    });
    assert_eq!(t.latex(), r"\frac{\frac{a}{b}}{}");
    t.up();
    assert_eq!((t.path.clone(), t.pos), (vec![(0, 0), (0, 0)], 1));
    t.up();
    assert_eq!((t.path.clone(), t.pos), (vec![(0, 0), (0, 0)], 1));
    t.exit(true);
    t.down();
    assert_eq!((t.path.clone(), t.pos), (vec![(0, 1)], 0));
}

#[test]
fn left_and_right_visit_every_position_and_are_symmetric() {
    let mut t = build(|t| {
        t.make_fraction();
        sym(t, "a");
        t.down();
        sym(t, "b");
        t.exit(true);
        sym(t, "+");
        sym(t, "c");
    });
    for _ in 0..10 {
        t.left();
    }
    assert_eq!((t.path.clone(), t.pos), (vec![], 0));
    let forward = walk(&mut t, true, 7);
    let expected: Vec<Position> = vec![
        (vec![], 0),
        (vec![(0, 0)], 0),
        (vec![(0, 0)], 1),
        (vec![(0, 1)], 0),
        (vec![(0, 1)], 1),
        (vec![], 1),
        (vec![], 2),
        (vec![], 3),
    ];
    assert_eq!(forward, expected);
    let backward = walk(&mut t, false, 7);
    let mut reversed = expected;
    reversed.reverse();
    assert_eq!(backward, reversed);
}

#[test]
fn exit_moves_just_outside_the_box() {
    let mut t = build(|t| {
        t.make_fraction();
        sym(t, "a");
        t.down();
        sym(t, "b");
    });
    t.exit(false);
    assert_eq!((t.path.clone(), t.pos), (vec![], 0));
    t.exit(true);
    assert_eq!((t.path.clone(), t.pos), (vec![], 0));
}

#[test]
fn next_and_previous_argument_stay_inside_the_innermost_box() {
    let mut t = build(|t| {
        t.make_fraction();
        sym(t, "a");
    });
    t.previous_argument();
    assert_eq!((t.path.clone(), t.pos), (vec![(0, 0)], 1));
    t.next_argument();
    assert_eq!((t.path.clone(), t.pos), (vec![(0, 1)], 0));
    t.next_argument();
    assert_eq!((t.path.clone(), t.pos), (vec![(0, 1)], 0));
    t.previous_argument();
    assert_eq!((t.path.clone(), t.pos), (vec![(0, 0)], 1));
}
