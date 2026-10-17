//! End-to-end LaTeX benchmark. Never substitute the hand-written Typst fixtures.
use mathstral_render::{benchmark, fixtures::Formula};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    benchmark::run_with_prelude("mitex", mathstral_latex::PRELUDE, |formula: &Formula| {
        mathstral_latex::convert(formula.latex)
    })
}
