use super::*;
use lvtex_mls::{Label, Status};

fn node(key: u64, level: usize, uses: Vec<u64>) -> Node {
    Node {
        key,
        paragraph: 0,
        label: Label::Definition,
        name: String::new(),
        status: Status::Checking,
        uses,
        level,
    }
}

#[test]
fn dot_offsets_follow_levels_and_clamp_at_the_deepest_column() {
    let nodes = [
        node(1, 0, vec![]),
        node(2, 2, vec![]),
        node(3, CHART_MAX_LEVEL + 3, vec![]),
    ];
    let indent = |level: usize| level as f32 * CHART_INDENT + DOT_RADIUS;
    assert_eq!(
        dot_offsets(&nodes),
        vec![indent(0), indent(2), indent(CHART_MAX_LEVEL)]
    );
}

#[test]
fn edges_map_keys_to_rows_and_skip_unknown_keys() {
    let nodes = [
        node(10, 0, vec![]),
        node(20, 1, vec![10, 99]),
        node(30, 2, vec![10, 20]),
    ];
    assert_eq!(edges(&nodes), vec![(0, 1), (0, 2), (1, 2)]);
}
