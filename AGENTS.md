# AGENTS.md

Mathstral is a native macOS editor (Apple Silicon) for writing prose with inline math, written in pure Rust as a Cargo virtual workspace. Typing `\` opens the TeXmacs-style manual math editor; Edit → Ask AI for Formula (⇧⌘1) opens an optional natural-language prompt. A local Ollama model turns AI prompts into LaTeX, and MiTeX and Typst render formulas in the background, so writing never waits.

## Layout

```text
app/                   The `mathstral` binary (the macOS app)
  src/main.rs          Entry point
  src/app/             Window and event loop: mod (state, drawing), input, commands, math_session
  src/document/        Document model: mod, codec (file format), cursor, history, manual, prompt, tests
  src/math/            Formula tree, keys and background work: mod, bindings, input, parse, tree, model (Ollama), worker
  src/view/            Drawing: mod, caret, layout (typesetting), render (pixel buffer)
  src/config.rs        Constants: typography, colors, model, Ollama address
  src/export.rs        PDF export through Typst, formulas as vector math
  src/file.rs          Open, save (atomic), dialogs, saved settings
  src/markdown.rs      Markdown line structure: block markers and inline styles
  src/menu.rs          Native menu and its actions
  src/verification.rs  cfg(test) checks of document and render behavior
crates/render/         mathstral-render: Typst-based math renderer, benchmark binaries
crates/latex/          mathstral-latex: LaTeX to Typst conversion through MiTeX
crates/latex/vendor/   Vendored MiTeX Typst command library
assets/fonts/          Bundled fonts and their licences
scripts/               Build and packaging scripts
docs/                  EDITOR.md (behavior), RENDERING-SPIKE.md, BRIEF.md (design brief)
artifacts/             Sample document, rendering benchmark output
.agents/skills/        Procedures and guidance; start with how-we-work
```

## Commands

```sh
cargo fmt --all --check                               # formatting
cargo clippy --workspace --all-targets -- -D warnings # lint
cargo test --release --locked --workspace             # tests; the three lines above are the local CI
sh scripts/build-app.sh                               # builds target/Mathstral.app (ad-hoc signed)
sh scripts/package.sh                                 # builds and writes a release zip to dist/
```

Run the scripts from the repo root. There is no hosted CI configuration in the repo.

## Runtime

- Ollama serves on `127.0.0.1:11434`. The model is `OLLAMA_MODEL` in `app/src/config.rs` (currently `gemma4:e4b`).
- Only new `\…\` prompts need the model. Writing, saving and rendering existing formulas work without it.
- The model server handles one request at a time; other prompts queue.
- Settings and the last-opened path live in `~/Library/Application Support/Mathstral`.

## File format

Files are Markdown-compatible text.

- Inline math is `$…$`. Display math is `$$` on its own lines.
- Each formula's source prompt sits in a hidden comment, `<!--mathstral:BASE64URL-->`.
- A literal `$` in prose is saved as `\$`. A pending prompt is saved as `\prompt\` and resumes when the file opens.

The format is a one-way door (see `.agents/skills/how-we-work/SKILL.md`, Principles).

## Docs

- `docs/EDITOR.md` is the behavior reference. Update it in the same change as any user-visible behavior change.
- `README.md` covers installation and basic use. Keep it accurate when a feature it describes changes.

## Dependencies

- Each dependency is exact-pinned with `=` in its `Cargo.toml`. `Cargo.lock` is committed.
- Edition 2024. The minimum Rust version is `rust-version` in the workspace `Cargo.toml` (1.92).
- Adding a crate needs the user's approval (see `.agents/skills/how-we-work/SKILL.md`).
