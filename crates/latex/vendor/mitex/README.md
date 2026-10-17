# Vendored MiTeX Typst specification

- Source: https://github.com/mitex-rs/mitex, directory `packages/mitex/specs`, commit `c5e9f9bf2` (the MiTeX 0.2.4 release).
- License: Apache-2.0 (see `LICENSE` in this directory, copied verbatim from upstream).
- Files: `prelude.typ` (unmodified), `standard.typ` (modified, see below).

These files are the Typst side of the LaTeX command library that the `mitex` crate's converter targets. `crates/latex/src/lib.rs` concatenates them into the Typst prelude, followed by `crates/latex/src/mitex.typ`, which overrides entries of the resulting scope.

## Modifications

`standard.typ`:

1. Removed `#import "@preview/xarrow:0.2.0": xarrow` and `#import "../prelude.typ": *`. The renderer has no package or file access, and the files are concatenated into one prelude instead.
2. Added the header comment line at the top of the file.
3. Upstream commit `e28ef8d1e` (applied to `standard.typ` only): `arrow-handle` uses Typst's native `stretch` instead of `xarrow`, and the arrow symbols are the non-`.long` variants. The `xarrow` import removal is item 1.
4. Upstream commit `c8f5af5e3`: `KaTeX` renders `K A T E X` instead of `kai A T E X`.
5. Upstream commit `985d8e725`: the deprecated `gt.tri` / `lt.tri` symbol names are replaced by `gt.closed` / `lt.closed`. The converter emits the old names from its prebuilt specification, so this change does not affect converted output. Those commands are listed in the spec allowlist in `lib.rs`.

`prelude.typ` is unmodified.

Upstream commits after `c5e9f9bf2` that change the converter specification were not applied, because they would no longer match the specification prebuilt into the crate.

The upstream `mitex.typ` glue (WASM plugin, `mitex-convert`, `mimath`) is not vendored. `lib.rs` evaluates the converted body directly.
