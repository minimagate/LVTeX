//! Hit testing, caret metrics and visual row lookup.
use super::Layout;

impl Layout {
    pub fn caret_on_line(&self, byte: usize, preferred: Option<usize>) -> (usize, f32) {
        // A wrap boundary belongs to the following visual line.
        let row = preferred
            .filter(|&row| {
                self.lines
                    .get(row)
                    .is_some_and(|l| l.start <= byte && byte <= l.end)
            })
            .unwrap_or_else(|| {
                self.lines
                    .iter()
                    .rposition(|l| l.start <= byte)
                    .unwrap_or(0)
            });
        let line = &self.lines[row];
        let x = line
            .stops
            .iter()
            .find(|p| p.0 == byte)
            .or_else(|| line.stops.last())
            .map(|p| p.1)
            .unwrap_or(0.0);
        (row, x)
    }
    // Top and height of the caret on `row` at `byte`: the ink of the letter that ends there,
    // else the cap height at the line start.
    pub fn caret_extent(&self, row: usize, byte: usize) -> (f32, f32) {
        let line = &self.lines[row];
        let (ascent, descent) = line
            .carets
            .iter()
            .find(|c| c.0 == byte)
            .map_or(line.start_caret, |c| c.1);
        (line.baseline - ascent, ascent + descent)
    }
    pub fn hit(&self, row: usize, x: f32) -> usize {
        let row = row.min(self.lines.len().saturating_sub(1));
        self.lines[row]
            .stops
            .iter()
            .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
            .map(|p| p.0)
            .unwrap_or(0)
    }
    pub fn row_at(&self, y: f32) -> usize {
        self.lines
            .iter()
            .rposition(|line| line.top <= y)
            .unwrap_or(0)
    }
    pub fn height(&self) -> f32 {
        self.lines.last().map_or(0.0, |line| line.top + line.height)
    }
}
