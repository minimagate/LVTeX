//! The Structure chart: the document's definitions and statements and what each uses.
use super::mls::{contains, status_color};
use super::*;
use mathstral_mls::Node;

/// The open chart. Areas are (x, y, w, h) in logical window coordinates, as the last draw left
/// them.
#[derive(Default)]
pub(super) struct Chart {
    area: (f32, f32, f32, f32),
    /// Each drawn row's paragraph and area, in node order.
    rows: Vec<(usize, (f32, f32, f32, f32))>,
}

/// Each node's dot centre, measured from the left of the graph column.
fn dot_offsets(nodes: &[Node]) -> Vec<f32> {
    nodes
        .iter()
        .map(|node| node.level.min(CHART_MAX_LEVEL) as f32 * CHART_INDENT + DOT_RADIUS)
        .collect()
}

/// The dependency edges as (row of the used node, row of the node that uses it). A key that names
/// no node is skipped.
fn edges(nodes: &[Node]) -> Vec<(usize, usize)> {
    let rows: HashMap<u64, usize> = nodes
        .iter()
        .enumerate()
        .map(|(row, node)| (node.key, row))
        .collect();
    let mut edges = Vec::new();
    for (row, node) in nodes.iter().enumerate() {
        for key in &node.uses {
            if let Some(&used) = rows.get(key) {
                edges.push((used, row));
            }
        }
    }
    edges
}

/// The text cut to one line in `width`, with a trailing ellipsis when anything is cut.
fn fit(typesetter: &mut Typesetter, text: &str, width: f32, mono: bool, scale: f32) -> String {
    let one_line = typesetter.plain("M", width, mono, scale, 0).height();
    let mut kept: Vec<char> = text.chars().collect();
    let mut shown = text.to_owned();
    while typesetter.plain(&shown, width, mono, scale, 0).height() > one_line {
        if kept.pop().is_none() {
            break;
        }
        shown = kept.iter().collect::<String>() + "…";
    }
    shown
}

impl App {
    /// Opens the chart, or closes it when it is open.
    pub(super) fn toggle_chart(&mut self) {
        self.chart = self.chart.is_none().then(Chart::default);
        self.repaint();
    }

    /// Closes the chart. Returns false when none was open.
    pub(super) fn close_chart(&mut self) -> bool {
        if self.chart.take().is_none() {
            return false;
        }
        self.repaint();
        true
    }

    /// True when (x, y) lies inside the open chart, in logical window coordinates.
    pub(super) fn chart_covers(&self, x: f32, y: f32) -> bool {
        self.chart
            .as_ref()
            .is_some_and(|chart| contains(chart.area, x, y))
    }

    /// The paragraph of the chart row under (x, y), in logical window coordinates.
    pub(super) fn chart_paragraph_at(&self, x: f32, y: f32) -> Option<usize> {
        self.chart
            .as_ref()?
            .rows
            .iter()
            .find(|(_, area)| contains(*area, x, y))
            .map(|(paragraph, _)| *paragraph)
    }

    /// Paints the chart over the cached page, at the top right, and records its areas.
    pub(super) fn draw_chart(&mut self, width: usize, height: usize, scale: f32) {
        if self.chart.is_none() {
            return;
        }
        let nodes = self.mls.chart();
        let view_width = width as f32 / scale;
        let view_height = height as f32 / scale;
        let w = CHART_WIDTH.min(view_width - 2.0 * PANEL_EDGE).max(0.0);
        let x = view_width - PANEL_EDGE - w;
        let y = CHART_TOP;
        let inner = (w - 2.0 * PANEL_PADDING).max(0.0);
        let deepest = nodes
            .iter()
            .map(|node| node.level.min(CHART_MAX_LEVEL))
            .max()
            .unwrap_or(0);
        let graph = (deepest + 1) as f32 * CHART_INDENT;
        let name_width = (inner - graph - CHART_GAP).max(0.0);
        let header =
            self.typesetter
                .plain("Structure", inner, false, PANEL_CODE_SCALE, MARKER_COLOR);
        let header_height = header.height();
        // With no nodes, one row says so.
        let body = nodes.len().max(1);
        let wanted = 2.0 * PANEL_PADDING + header_height + body as f32 * CHART_ROW;
        let h = wanted.min(view_height - y - PANEL_EDGE).max(0.0);
        let shown = (((h - 2.0 * PANEL_PADDING - header_height) / CHART_ROW)
            .floor()
            .max(0.0) as usize)
            .min(body);
        let row_top = |row: usize| y + PANEL_PADDING + header_height + row as f32 * CHART_ROW;
        let row_centre = |row: usize| row_top(row) + CHART_ROW / 2.0;
        let offsets = dot_offsets(&nodes);
        let centre = |row: usize| (x + PANEL_PADDING + offsets[row], row_centre(row));

        // Lay every line out before painting, so the typesetter is not borrowed while painting.
        let text_left = x + PANEL_PADDING + graph + CHART_GAP;
        let mut labels = vec![(header, x + PANEL_PADDING, y + PANEL_PADDING)];
        for (row, node) in nodes.iter().enumerate().take(shown) {
            let name = fit(
                &mut self.typesetter,
                &node.name,
                name_width,
                true,
                PANEL_CODE_SCALE,
            );
            let layout =
                self.typesetter
                    .plain(&name, name_width, true, PANEL_CODE_SCALE, TEXT_COLOR);
            let top = row_centre(row) - layout.height() / 2.0;
            labels.push((layout, text_left, top));
        }
        if nodes.is_empty() && shown > 0 {
            let message = fit(
                &mut self.typesetter,
                "No definitions or statements yet.",
                inner,
                false,
                PANEL_TEXT_SCALE,
            );
            let layout =
                self.typesetter
                    .plain(&message, inner, false, PANEL_TEXT_SCALE, MARKER_COLOR);
            let top = row_centre(0) - layout.height() / 2.0;
            labels.push((layout, x + PANEL_PADDING, top));
        }

        render::rounded_rect(
            &mut self.pixels,
            width,
            height,
            x * scale,
            y * scale,
            w * scale,
            h * scale,
            PANEL_RADIUS * scale,
            PANEL_COLOR,
        );
        for (from, to) in edges(&nodes) {
            if from >= shown || to >= shown {
                continue;
            }
            let (x0, y0) = centre(from);
            let (x1, y1) = centre(to);
            render::line(
                &mut self.pixels,
                width,
                height,
                x0 * scale,
                y0 * scale,
                x1 * scale,
                y1 * scale,
                CHART_EDGE_WIDTH * scale,
                CHART_EDGE_COLOR,
            );
        }
        for (row, node) in nodes.iter().enumerate().take(shown) {
            let (cx, cy) = centre(row);
            render::dot(
                &mut self.pixels,
                width,
                height,
                cx * scale,
                cy * scale,
                DOT_RADIUS * scale,
                status_color(node.status),
                1.0,
            );
        }
        for (layout, left, top) in &labels {
            render::paint(
                &mut self.pixels,
                width,
                height,
                scale,
                layout,
                &mut self.typesetter,
                &mut self.cache,
                *left,
                *top,
            );
        }
        self.chart = Some(Chart {
            area: (x, y, w, h),
            rows: nodes
                .iter()
                .enumerate()
                .take(shown)
                .map(|(row, node)| (node.paragraph, (x, row_top(row), w, CHART_ROW)))
                .collect(),
        });
    }
}

#[cfg(test)]
#[path = "../../tests/unit/app/chart.rs"]
mod tests;
