# Mathstral

A native macOS math-writing app. Type `/a description of mathematics/` in the
text flow; local Gemma converts it to a rendered formula in the background.
The editor, native file operations, portable Markdown, prompt editing and
MiTeX → Typst renderer run in `target/Mathstral.app`. See
[editor status](docs/EDITOR-STATUS.md) for behavior and verification.

Read the [rendering report and recommendation](docs/RENDERING-SPIKE.md), or view
[display samples](artifacts/mitex/gallery-1.png) and
[inline samples](artifacts/mitex/inline-gallery-1.png).

## Run

Rust 1.92 or newer. Tested on Apple Silicon macOS. No TeX distribution, external
Typst executable, browser, JavaScript engine, or GPU is needed. Prompt generation
uses local Ollama with the model configured in `app/src/config.rs` (currently the
user-selected `gemma4:e4b`). The renderer benchmarks do not require Ollama.

```sh
cargo build --release --locked --workspace
cargo test --release --locked --workspace
cargo run --release --locked -p latex-spike
```

The benchmark writes transparent Retina formula PNGs, four contact sheets,
per-case timings, generated Typst source and run metadata to `artifacts/mitex/`.
An optional first argument selects another output directory. Any conversion or
render failure gives a nonzero exit status.

Two comparison binaries share the same dependency lockfile and release profile:

```sh
cargo run --release --locked --bin typst-spike
cargo build --release --locked --bin size-baseline
```

`typst-spike` measures manually written Typst equivalents without LaTeX
conversion; `size-baseline` is an empty Rust executable for size comparison.
Only `latex-spike` is evidence for the complete rendering pipeline.

In this sandbox dependencies have been fetched into a workspace-local cache:

```sh
CARGO_HOME="$PWD/.cargo-home" cargo run --offline --release --locked -p latex-spike
```

## Dependencies

| Dependency | Purpose |
| --- | --- |
| `mitex` 0.2.4 | LaTeX parsing and translation to Typst math |
| `mitex-spec-gen` 0.2.4 | Selects `prebuilt` on an existing MiTeX dependency, so builds do not invoke an external Typst CLI |
| `typst` 0.15.1 | Evaluation and mathematical typesetting |
| `typst-layout` 0.15.1 | Output type required by the current compile API |
| `typst-render` 0.15.1 | CPU rasterization |
| `tiny-skia` 0.12.0 | Pixmaps, PNG output and contact-sheet composition |

Exact direct versions are pinned in Cargo.toml and transitive versions are locked
in the root Cargo.lock. Typst's transitive dependency tree is substantial; the
report measures its actual release-size cost. No async runtime or benchmarking
framework is used.

The small [compatibility scope](tools/latex-spike/src/mitex.typ) supplies helpers
needed by MiTeX's output. Unsupported helpers fail rendering rather than falling
back to the manually written fixtures. Structural validation rejects unclosed
braces and environments before conversion. This is a spike for representative
math, not a complete TeX implementation or the final editor renderer API.

New Computer Modern Math is embedded for equations; Libertinus Serif supplies
text and sample labels. [Font provenance and licenses](assets/fonts/README.md)
are included. The editor now uses embedded Libertinus Serif, per the updated font preference.
