//! End-to-end LaTeX benchmark. Never substitute the hand-written Typst fixtures.
use mathstral_render_spike::{benchmark, fixtures::Formula};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    benchmark::run_with_prelude("mitex", latex_spike::PRELUDE, |formula: &Formula| {
        latex_spike::convert(formula.latex)
    })
}
