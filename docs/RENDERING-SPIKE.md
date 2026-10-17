# Rendering evaluation

**Recommendation: MiTeX + Typst with CPU rasterization.** All 20 representative LaTeX
expressions render successfully in both display and inline styles. Quality is good at
Retina resolution, and the measured latency fits asynchronous formula replacement. The
main cost is the 26.66 MiB executable, mostly Typst. ReX was not needed for this suite
and has not been benchmarked, so no comparative ReX performance claim is made.

The editor's current behavior is described in [EDITOR.md](EDITOR.md). The original
application scope is in [BRIEF.md](BRIEF.md).

## Measurements

Run on 2026-10-17 on Apple Silicon (`aarch64-apple-darwin`), macOS 27.0.1, Rust 1.99.0.
The exact CPU model was not recorded. Release settings at the time of measurement:
`opt-level = "s"`, thin LTO, one codegen unit, stripped symbols, `panic = "abort"`.
Release builds now use `panic = "unwind"` so that a panic in the math worker fails only
the current formula. Both benchmarks share one `Cargo.lock` and release profile.

| Measurement | Full MiTeX → Typst → pixels |
| --- | ---: |
| Formulas / successful style cases | 20 / 40 of 40 |
| Logical font size / physical scale | 22 / 2× Retina |
| Samples per case per timing mode | 21 |
| Uncached render, median of per-case medians | 0.163 ms |
| Uncached render, range of per-case medians | 0.119–0.609 ms |
| Largest per-case uncached p95 | 0.869 ms |
| Repeated render, median of per-case medians | 0.058 ms |
| Repeated render, range of per-case medians | 0.050–0.132 ms |
| Renderer initialization | 4.124 ms |
| First formula after initialization | 9.610 ms |
| Stripped ARM64 executable | 27,956,416 bytes (26.66 MiB) |
| Matched Typst-only executable | 27,744,784 bytes (26.46 MiB) |
| Full-pipeline increment over Typst-only | 211,632 bytes (0.20 MiB) |
| Empty Rust executable, same profile | 322,848 bytes (0.31 MiB) |
| Full-pipeline increment over empty executable | 27,633,568 bytes (26.35 MiB) |
| Embedded font files combined | 1,643,400 bytes (1.57 MiB) |

The executable includes the benchmark harness, PNG encoding and gallery code. These are
practical build-size comparisons, not exact per-crate attribution or the final application
size. The GUI editor adds its own costs.

`file` reports an ARM64 Mach-O executable. `otool -L` lists only macOS `libiconv` and
`libSystem`. No TeX installation, browser, JavaScript engine or GPU is used. Direct
dependencies are pinned in [`crates/latex/Cargo.toml`](../crates/latex/Cargo.toml) and
[`crates/render/Cargo.toml`](../crates/render/Cargo.toml).

The Typst-only comparison renders manually written counterparts. Its median of
per-case uncached medians was 0.109 ms. Only the full MiTeX run validates the LaTeX
inputs. The handwritten counterparts are never used as a failure fallback.

## Visual review

All four sheets were inspected:

- Display: [formulas 1–10](../artifacts/mitex/gallery-1.png),
  [formulas 11–20](../artifacts/mitex/gallery-2.png).
- Inline: [formulas 1–10](../artifacts/mitex/inline-gallery-1.png),
  [formulas 11–20](../artifacts/mitex/inline-gallery-2.png).

The suite covers integrals, sums, nested fractions, radicals, two matrix sizes, aligned
equations, piecewise functions, Greek, nested scripts, limits, partial derivatives, set
notation, scalable delimiters, accents, binomials, products, text in math and determinants.
New Computer Modern Math is clear in off-white on `#151515`. Alignment points, matrix
brackets, case spacing and nested scripts are intact. Inline limits and fractions become
more compact, as expected. The sheets show no missing glyphs, clipped strokes, overlapping
cells or renderer warnings. This is a visual assessment of these expressions, not a proof
of arbitrary LaTeX fidelity.

The math font is embedded New Computer Modern Math. Libertinus Serif supplies sample labels
and text. Their [license and provenance records](../assets/fonts/README.md) are included.
The editor font was later set to embedded Libertinus Serif. In-memory pixmaps use premultiplied RGBA. Exported PNGs keep
transparency, and the gallery composites them onto the requested background.

## Integration decisions

The [small compatibility scope](../crates/latex/src/mitex.typ) supplies MiTeX's square-root,
matrix, aligned-equation and text helpers. It restores the older partial-derivative and
angle-delimiter names, and gives cases a clear column gap. It does not rewrite text inside
formulas. Unknown helpers fail compilation rather than being shown as words or replaced
with another expression. This is an explicit subset of MiTeX's full package scope.
Additional commands may need helpers during editor integration.

The adapter rejects unclosed braces and missing or mismatched environment ends. MiTeX can
recover unfinished input, so rendering success alone would otherwise accept an incomplete
matrix. This validation is structural, not a complete TeX validator. Unknown commands and
rendering errors propagate to the caller. Macro packages and arbitrary full TeX documents
are outside the scope of this evaluation.

For inline atoms, the renderer uses full block bounds with inline math styling. This prevents
Typst paragraph metrics from clipping tall formulas at the edge of the page. The renderer's
world supplies only embedded fonts and its in-memory source. File and package access is
denied. The `prebuilt` feature on MiTeX's existing specification dependency prevents
build-time Typst CLI discovery.

The release tests cover all 40 LaTeX cases and all 40 reference Typst cases, transparent edge
padding, 1× and 2× dimensions, malformed input, escaped braces and comments, indexed roots,
symbol and text distinctions, invalid scales and denied file access. Formatting checks pass.

## Method and reproducibility

```sh
cargo build --release --locked --workspace
cargo test --release --locked --workspace
cargo run --release --locked -p mathstral-latex --bin latex-spike
cargo run --release --locked -p mathstral-render --bin typst-spike
cargo build --release --locked -p mathstral-render --bin size-baseline
```

`latex-spike` (package `mathstral-latex`) is the full pipeline benchmark. `typst-spike`
(package `mathstral-render`) measures the manually written Typst counterparts only.
`size-baseline` is an empty executable used for size comparison. A benchmark argument can
select another output folder.

The timer covers structural checks, LaTeX conversion, Typst compilation and layout, and
rasterization. PNG compression, disk writes and gallery generation are excluded. Before each
uncached sample, `comemo::evict(0)` clears Typst memoization. Font and library objects stay
initialized. Repeated samples retain internal memoization but still convert and rasterize.
No application pixmap cache is used. Percentiles use nearest rank. These are one local run's
measurements, not cross-machine guarantees. Initialization and the first formula are measured
separately. They exclude OS process launch and do not measure GUI cold start.

Evidence retained in the repository:

- [Timings for the full pipeline](../artifacts/mitex/timings.csv) and
  [run metadata](../artifacts/mitex/run.txt).
- [Converted Typst sources](../artifacts/mitex/sources.md) and
  [failure list](../artifacts/mitex/failures.txt).
- [Comparison timings](../artifacts/typst-only/timings.csv).

The console output, dependency tree and file hashes from the original run are not retained.

The pinned API versions were checked against
[MiTeX 0.2.4](https://docs.rs/mitex/0.2.4/mitex/fn.convert_math.html),
[Typst 0.15.1](https://docs.rs/typst/0.15.1/typst/trait.World.html) and
[typst-render 0.15.1](https://docs.rs/typst-render/0.15.1/typst_render/fn.render.html),
then verified by compiling the local published sources. The
[upstream MiTeX scope](https://github.com/mitex-rs/mitex/blob/main/packages/mitex/specs/latex/standard.typ)
provides the context for helper semantics.

## Known limits at the time of the evaluation

- Raster dimensions are bounded to 4096 pixels per side. There was no compilation time or
  memory limit for a single formula; the editor adds its own handling of failed and
  panicking renders.
- Results come from one machine and one run. They are not cross-machine guarantees.
