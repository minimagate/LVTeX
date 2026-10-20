# AGENTS.md

Always inspect the workspace and Git state at the start of a conversation,
including voice sessions. Read this file and follow the user's current scope.

Mathstral alpha 2 is a native macOS editor for prose with inline math, written in
Rust as a Cargo virtual workspace. Apple Silicon is the supported platform.
**⌘1** opens manual math; **⇧⌘1** opens an optional AI prompt. A backslash in prose
is literal. Ollama generates LaTeX; MiTeX and Typst convert and render it off the
UI thread. MLS checks each definition, statement and proof with Lean and shows a
status dot beside it.

## Layout and boundaries

- `app/src/app/`: state and coordination in `mod.rs`; native events in `events.rs`,
  presentation in `drawing.rs`, plus input, commands, manual math sessions and the
  Lean dots and panel (`mls.rs`).
- `app/src/document/`: document model, codec, cursor, history and formula sessions.
- `app/src/math/`: key bindings, input, parsing, formula tree and serialization,
  model protocol, render preparation and workers.
- `app/src/view/`: caret and pixel drawing; `layout/` owns document typesetting,
  paragraph styles, font shaping and hit testing.
- `app/src/{config,export,file,markdown,menu}.rs`: configuration, PDF export,
  platform file operations, Markdown and native menus.
- `crates/latex/`: LaTeX conversion; `vendor/mitex/` contains required Typst
  definitions and their upstream license.
- `crates/render/`: Typst world, math rasterization and PDF compilation.
- `crates/mls/`: Lean checking (MLS): paragraph and block state, versions and job
  planning, OpenRouter requests through `curl`, rejection rules, the Lean REPL and
  its worker. It builds and tests on Linux; keep macOS code out of it.
- `lean/`: the pinned Lake project (Lean, Mathlib, REPL) that MLS checks against.
  Its `.lake/` build output is ignored.
- `app/tests/`, `crates/*/tests/`: tests and fixtures. Keep test bodies out of `src/`.
  Include private unit tests with `#[cfg(test)] #[path = "…"] mod tests;`.
  Put public library tests in ordinary Cargo integration targets. Do not widen
  production visibility solely for tests.
- `assets/fonts/`: required embedded fonts and upstream notices.
- `scripts/`: build and packaging. Generated output belongs in ignored `target/`
  and `dist/`, never in the tracked source tree.
- `.agents/skills/`: reusable coding guidance; start with `how-we-work` unless the
  user's current instructions override that workflow.

Keep files scoped to a concept. The app depends on the libraries; the LaTeX
library uses the renderer only in tests; the MLS library depends on no other
workspace crate. Avoid extra scaffolding, experiment
binaries, generated artifacts and separate design/status documents.

## Commands

Run from the repository root:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --release --locked --workspace
sh scripts/build-app.sh
sh scripts/package.sh
```

The first three commands are local CI; there is no hosted CI configuration.
The build script assembles and ad-hoc signs `target/Mathstral.app`; packaging
writes `dist/Mathstral-<version>-alpha-macos-<arch>.zip`. A populated `.cargo-home`
is an optional offline cache used by the build script. For manual Cargo commands
using that cache, prefix `CARGO_HOME="$PWD/.cargo-home"` and add `--offline`.

## Compatibility and runtime

- `README.md` is the maintained user and developer reference. Update it when
  commands, supported behavior or layout change. Keep detailed rules in code
  comments and regression tests rather than duplicating them in extra documents.
- Preserve the Markdown file encoding: `$…$`, display `$$` lines, source-prompt
  comments, `\$` and `\\` escaping and pending-prompt comments. These and the
  settings format are compatibility boundaries.
- Settings and the last-opened file live in
  `~/Library/Application Support/Mathstral`.
- Ollama runs at `127.0.0.1:11434`; `OLLAMA_MODEL` in `app/src/config.rs` selects
  the model (currently `gemma4:e4b`). Only AI prompts need it. Model requests run
  serially; local renders and previews have a separate worker.
- MLS uses OpenRouter only: `JEV_MODEL` and `HAIKU_MODEL` in `crates/mls/src/lib.rs`
  pin the models. The key comes from `OPENROUTER_API_KEY` and must never be logged,
  printed, written or passed as an argument; the Lean REPL runs without it. The
  Lean project defaults to `lean/` (`LEAN_PROJECT` in `app/src/config.rs`;
  `MATHSTRAL_LEAN_PROJECT` overrides it). Saving writes `<name>.mls.lean` beside
  the document.
- Edition 2024; minimum Rust version is `workspace.package.rust-version` (1.92).
  Exact-pin external dependencies with `=`. Share repeated dependencies through
  `[workspace.dependencies]` and commit `Cargo.lock`.
- Do not add dependencies without the user's approval. Preserve upstream font and
  vendored-code licenses when changing assets or packaging.
