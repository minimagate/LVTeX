# MLS state

MLS checks the mathematics in a Mathstral document with Lean. It classifies each
block, formalizes it into Lean and checks it, then shows the result as a dot
beside the block. This file records what the codebase had before MLS, what MLS
reuses, and what it adds.

## Phase 1: what exists (before MLS)

### How blocks are created and stored

There is no HTML block structure. A document is a `ropey::Rope` of plain text
(`app/src/document/mod.rs`). Each formula is one U+FFFC atom in that text, with a
`Formula` record (`latex`, optional AI `prompt`, rendered image) in a
`BTreeMap` keyed by byte offset. The file on disk is Markdown
(`app/src/document/codec.rs`): `$…$` inline, `$$` display lines, prompt comments,
`\$` and `\\` escapes. That encoding is a compatibility boundary, and MLS does
not change it.

The editor has no "block" object. Enter inserts `\n`, and the view lays out each
`\n`-separated line as a paragraph (`app/src/view/layout/`). The only block
tags are the Markdown line kinds in `app/src/markdown.rs` (`Block::Paragraph`,
`Heading`, `Bullet`, `Quote` and so on). They are computed per line for
styling, have no ids and are never stored. MLS leaves them untouched.

### Classification, Lean and versioning

- **Block classification:** none. No local classifier exists. The only model is
  the Ollama LaTeX generator (`app/src/math/model.rs`).
- **Lean:** none. There is no Lean code, prototype, toolchain or Lake project.
- **Git-like versioning:** none. Versioning is the in-memory undo/redo stack of
  whole-document snapshots (`app/src/document/history.rs`), plus the "dirty"
  comparison against the last saved Markdown. No block ids, revisions or
  history persist.

### What MLS can reuse

- The worker pattern in `app/src/math/worker.rs`: a plain `std::thread`, `mpsc`
  channels, a wake callback that posts a winit user event, `catch_unwind` per
  job and stale-result rejection by id.
- `serde_json` (already a dependency) for request and response JSON.
- `std::process` for the Lean REPL and the HTTPS transport (see below).
- The layout's per-line geometry (`Layout::lines`, with byte ranges and tops)
  for placing dots without changing the layout.
- The typesetter and pixel painter for a panel in the editor's font.
- `file::write`, the atomic save, for writing the Lean file.

### What is missing

- A notion of a block with an id and a version: added by MLS.
- An HTTPS client. The Ollama client is plain loopback HTTP over `TcpStream`, and
  the workspace has no TLS crate. Adding one (`ureq` with `rustls`, for example)
  would be a new dependency. MLS calls the system `curl` through
  `std::process` instead, passing its configuration on stdin so the key never
  appears in arguments, logs or files.
- A Lean toolchain, a Mathlib project and a REPL process: added under `lean/`
  and in the MLS crate.
- Any UI for per-block status: added as the dot and the panel.

### Platform notes from this inspection

- The app crate only builds on macOS: five AppKit-specific winit and muda calls
  fail elsewhere. The libraries build and test on Linux.
- MLS logic therefore lives in a new library crate, `crates/mls`, that builds and
  tests on Linux. The macOS layer stays thin.
