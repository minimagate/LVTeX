# Milestone 1 — rendering spike

**Complete. Recommend MiTeX + Typst with CPU
rasterization.** All 20 representative LaTeX expressions render successfully in
both display and inline styles. Quality is good at Retina resolution and the
measured latency fits asynchronous formula replacement. The main cost is the
26.66 MiB executable, mostly Typst. ReX was not needed for this suite and has
not been benchmarked; no comparative ReX performance claim is made.

Milestone 2 has since started; see [editor status](EDITOR-STATUS.md). The
complete application scope is preserved in [the brief](BRIEF.md).

## Measurements

The table records the original milestone-1 run. A later launch check regenerated
`artifacts/mitex/` and again passed all 40 cases; its timing CSV and run metadata
are a new sample, so their timings differ from this historical table. The console
log retains the original run. Formula sources and rendering code are unchanged.

Run on 2026-10-17 on Apple Silicon (`aarch64-apple-darwin`), macOS 27.0.1,
Rust 1.99.0. Exact CPU model was unavailable in the sandbox. Release settings:
`opt-level = "s"`, thin LTO, one codegen unit, stripped symbols, panic abort.
Both benchmarks share one Cargo.lock and release profile.

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

The executable includes the benchmark harness, PNG encoding and gallery code.
These are practical build-size comparisons, not exact per-crate attribution or
the final application size. GUI code and Cormorant will add their own costs.
`file` confirms an ARM64 Mach-O executable; `otool -L` lists only macOS
`libiconv` and `libSystem`. No TeX installation, browser, JavaScript engine or GPU
is used. Direct dependencies and their purpose are listed in [README](../README.md).

The Typst-only comparison renders manually written counterparts. Its median of
per-case uncached medians was 0.109 ms. Only the full MiTeX run validates the
LaTeX inputs; the handwritten counterparts are never used as a failure fallback.

## Visual review

All four final sheets were inspected:

- Display: [formulas 1–10](../artifacts/mitex/gallery-1.png),
  [formulas 11–20](../artifacts/mitex/gallery-2.png).
- Inline: [formulas 1–10](../artifacts/mitex/inline-gallery-1.png),
  [formulas 11–20](../artifacts/mitex/inline-gallery-2.png).

The suite covers integrals, sums, nested fractions, radicals, two matrix sizes,
aligned equations, piecewise functions, Greek, nested scripts, limits, partial
derivatives, set notation, scalable delimiters, accents, binomials, products,
text in math and determinants. New Computer Modern Math looks clear in off-white
on #151515. Alignment points, matrix brackets, case spacing and nested scripts
are intact. Inline limits and fractions become more compact as expected.
There are no missing glyphs, clipped strokes, overlapping gallery cells or
renderer warnings in the final suite. This is a visual assessment of these
expressions, not a proof of arbitrary LaTeX fidelity.

The math font is embedded New Computer Modern Math. Libertinus Serif supplies
sample labels and text. Their [license/provenance records](../assets/fonts/README.md)
are included. Cormorant Garamond remains the editor font for milestone 2.
In-memory pixmaps use premultiplied RGBA; exported PNGs retain transparency and
the gallery composites them onto the app's requested background.

## Integration decisions

The [small compatibility scope](../tools/latex-spike/src/mitex.typ) supplies
MiTeX's square-root, matrix, aligned-equation and text helpers, restores the
older partial-derivative and angle-delimiter names, and gives cases a clear
column gap. It does not rewrite text inside formulas. Unknown helpers fail
compilation rather than being silently displayed as words or substituted with
another expression. This is an explicit subset of MiTeX's full package scope;
additional commands may need helpers during editor integration.

The adapter rejects unclosed braces and missing/mismatched environment ends.
MiTeX itself can recover unfinished input, so rendering success alone would
otherwise accept an incomplete matrix. This validation is structural, not a
complete TeX validator. Unknown commands and rendering errors still propagate
for the later retry/error UI. Macro packages and arbitrary full TeX documents
are outside this expression-body spike.

For inline atoms, the renderer uses full block bounds with inline math styling.
This prevents Typst paragraph metrics from clipping tall formulas at the page
edge. The renderer's world supplies only embedded fonts and its in-memory
source; file and package access is denied. The `prebuilt` feature on MiTeX's
existing specification dependency prevents build-time Typst CLI discovery.

The release test suite passes four tests covering all 40 LaTeX cases and all
40 reference Typst cases, transparent edge padding, 1×/2× dimensions, malformed
input, escaped braces and comments, indexed roots, symbol/text distinctions,
invalid scales and denied file access. See [test output](../artifacts/tests.txt).
Formatting checks also pass.

## Method and reproducibility

```sh
CARGO_HOME="$PWD/.cargo-home" cargo build --offline --release --locked --workspace
CARGO_HOME="$PWD/.cargo-home" cargo test --offline --release --locked --workspace
target/release/latex-spike
target/release/typst-spike
```

The local cache is optional outside this sandbox; ordinary Cargo can download
all locked dependencies. A benchmark argument can select another output folder.

The timer covers structural checks, LaTeX conversion, Typst compilation/layout,
and rasterization. PNG compression, disk writes and gallery generation are
excluded. Before each uncached sample, `comemo::evict(0)` clears Typst memoization;
font/library objects stay initialized. Repeated samples retain internal
memoization but still convert and rasterize. No application pixmap cache is
used. Percentiles use nearest rank. These are one local run's measurements,
not cross-machine guarantees. Initialization and the first formula are measured
separately; they exclude OS process launch and do not measure GUI cold start.

Evidence: [full timings](../artifacts/mitex/timings.csv),
[run metadata](../artifacts/mitex/run.txt),
[actual converted sources](../artifacts/mitex/sources.md),
[console output](../artifacts/mitex-console.txt),
[comparison timings](../artifacts/typst-only/timings.csv),
[dependency tree](../artifacts/dependency-tree.txt), and
[source/binary/output hashes](../artifacts/checksums.txt).

The pinned API versions were checked against
[MiTeX 0.2.4](https://docs.rs/mitex/0.2.4/mitex/fn.convert_math.html),
[Typst 0.15.1](https://docs.rs/typst/0.15.1/typst/trait.World.html) and
[typst-render 0.15.1](https://docs.rs/typst-render/0.15.1/typst_render/fn.render.html),
then verified by compiling the local published sources. The
[upstream MiTeX scope](https://github.com/mitex-rs/mitex/blob/main/packages/mitex/specs/latex/standard.typ)
provides the context for helper semantics.

## Next milestone

Stop here for feedback on the rendering and size tradeoff. After that, milestone
2 is the borderless #151515 window, native menus/dialogs, file operations and
plain-text editing with the specified movement keys. Baseline extraction,
bounded math caches and Cormorant/math matching belong to math integration in
milestone 3. Worker isolation, model-output resource budgets and Ollama arrive
in the later milestones. The spike bounds raster dimensions to 4096 pixels
per side, but does not yet impose a compilation time/memory limit.
