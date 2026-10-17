use mathstral_render_spike::{benchmark, fixtures::Formula};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    benchmark::run("typst-only", |formula: &Formula| Ok(formula.typst.into()))
}
