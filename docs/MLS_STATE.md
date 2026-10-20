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

## Phase 2: what was built

**Green means "this Lean formalization is verified", not that it matches what you
wrote.** The assumptions panel, written from the Lean, is how you check that the
formalization says what you meant.

### Pieces

- `crates/mls` (builds and tests on Linux):
  - `pipeline.rs`: the per-document state. Each non-blank line is a paragraph.
    The line holding the caret is "open" and keeps its last settled text until
    Enter or a caret move closes it. A paragraph keeps its id across edits by
    diffing the common prefix and suffix. Blocks are a definition or a statement
    (one paragraph each), or a proof (paragraphs labelled proof, closed by ∎ or by
    the next non-proof paragraph). Each block has a version that changes with its
    text or label. The state yields one job at a time, in document order: label
    the paragraphs first, then formalize and check block by block. Every reply
    carries the block id and version (and, for checks, the base environment), and
    a stale reply is dropped. Editing a block re-formalizes only that block. The
    blocks below keep their Lean and are re-checked on the new environment, with
    no Haiku calls. A proof pairs with the statement directly above it (prose may
    sit between them). It is checked on that statement's own base environment,
    because its Lean restates the statement with a real proof, and it replaces
    the statement in the chain. If the statement changes, the proof is
    re-formalized.
  - `openrouter.rs`: Jev (`typesafe/jev-1.13`) is asked one typed question,
    `kind`, with the fixed choices definition, statement, proof and skip, through
    OpenRouter's Decisions endpoint. Haiku 5.5 (`anthropic/claude-haiku-5.5`) is
    asked through chat completions with a strict JSON schema
    (`{lean, lean_name, assumptions}`) and temperature 0. The request is laid out
    for caching: the system prompt and Lean conventions carry `cache_control`,
    then one segment per block above (the last one carries `cache_control`), then
    the prose above and the block. The reply is validated before use.
    Transport is the system `curl`, configured on stdin, so the key never appears
    in arguments, logs or files. The key comes from `OPENROUTER_API_KEY`; when it
    is missing, the first paragraph shows one red dot that says so.
  - `rules.rs`: rejected before Lean, marked red: `axiom` and `admit`; `sorry` in
    a definition or proof (a statement must end in `sorry`); commands that run
    code or change limits while Lean elaborates (`#eval` and every `#` command,
    `run_cmd`, `elab`, `macro`, `set_option`, `native_decide`, `unsafe`,
    `import`, …); redefining a name from a block above; a proof whose header
    differs from its statement's. Comments are ignored and string literals are
    respected.
  - `repl.rs`: leanprover-community/repl, started directly (with `LEAN_PATH` and
    `LEAN_SYSROOT` read once from `lake env`) and with a cleared environment, so
    model-written Lean never sees the API key. Mathlib is imported once per
    process (environment 0), and each block runs on the stored environment of the
    block above. Answers have deadlines: 600 s for the import, 60 s per block.
  - `worker.rs`: one background thread runs the jobs serially, with
    `catch_unwind` per job.
- App (macOS): `Document::paragraphs` provides the lines with formulas written as
  `$latex$`. `app/src/app/mls.rs` syncs MLS on every edit and caret move and
  applies replies on a winit user event, without touching the undo group. It
  draws a 6 pt dot 22 pt left of the column, in muted grey, amber, green or red
  (`config.rs`), cross-fading over 240 ms, with frames requested only while
  something fades. Hover shows a quiet panel, and click pins it: the assumptions
  in the editor's font, and **Show Lean** for the Lean and the error. Escape
  closes it, and nothing else is added to the screen. The panel and the dots
  overlay the page, so the layout never shifts.
- The Lean twin, `<name>.mls.lean`, is written beside the document on save and
  read on open. It holds `import Mathlib`, then the Lean that checked, each block
  under `-- mathstral:block <id> <label>`. A final `/- mathstral:memory … -/`
  comment holds JSON (`version` 1) with every paragraph's label and every block's
  Lean, assumptions or rejection reason. A proof's entry is keyed by its
  statement's exact Lean. On open, unchanged text needs no model calls; Lean
  re-checks every block, because REPL environments do not outlive the process.
  The Markdown file format is unchanged.

### Setting up Lean and Mathlib

```sh
curl https://raw.githubusercontent.com/leanprover/elan/master/elan-init.sh -sSf | sh
cd lean                      # in the Mathstral checkout
lake exe cache get           # prebuilt Mathlib (minutes)
lake build Mathlib repl      # the REPL executable; Mathlib is then up to date
```

`lean/lean-toolchain` pins Lean 4.35.0, and `lake-manifest.json` pins Mathlib
`v4.35.0` (`56a0058`) and REPL `v4.35.0` (`58bdde1`). The app uses `lean/` of
the checkout it was built from (`LEAN_PROJECT`); `MATHSTRAL_LEAN_PROJECT`
overrides it. Export `OPENROUTER_API_KEY` in `~/.zshrc` and launch Mathstral
from that terminal.

### Verified here (Linux cloud container)

- Local CI: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`
  and `cargo test --release --locked --workspace` pass. The app crate compiled
  and its tests ran through a local, uncommitted shim that removes five
  macOS-only calls; the committed code keeps them.
- Lean: Lean 4.35.0 came from GitHub releases, and Mathlib was built from source
  (its cache host is blocked here). The three test documents ran through the
  real worker and REPL with scripted model answers (`scripted_documents`):
  group theory ends all green, the limit lemma ends green and green, and the
  false lemma `n² ≥ 2ⁿ` ends with an amber statement and a red proof (Lean:
  unsolved goal `⊢ False` in the zero case). This takes about 25 s, including
  the Mathlib import.
- UI under Xvfb on Linux, through the shim: dots in all four colours with real
  Lean results restored from a hand-written twin, with no model calls; the panel
  with assumptions, Show Lean and the Lean error; the red "OPENROUTER_API_KEY is
  not set" dot and its panel.
- Killing the REPL stops Lean. Started through `lake env`, it was left running
  as an orphan, which is why it is now started directly.

### Not verified here: please check on your Mac

- openrouter.ai is blocked from this container and no key is set. Neither the
  live documents test (`cargo test --release --locked -p mathstral-mls --test
  documents -- --ignored --nocapture`) nor any real Jev or Haiku call has run.
  The model ids and the Decisions request shape (`model`, `state`, `questions`
  with a `choice` question and `criteria`; answer at
  `answers.kind.choice`) come from OpenRouter's public pages through web search,
  not from a live call. If Jev's request shape differs, `classify_request` and
  `parse_label` in `crates/mls/src/openrouter.rs` are the only places to change.
- OpenRouter's migration notes say Claude 5.5 models ignore `temperature`.
  OpenRouter drops unsupported parameters, so `temperature: 0` is sent and may
  have no effect. Haiku's prompt caching (`cache_control`) is passed as documented
  but unmeasured.
- The real macOS app: window, menus, Retina (2x) rendering of the dot and
  the panel, hover and click with a trackpad, the fade timing, Escape, Save
  writing `<name>.mls.lean`, reopening with no model calls, and idle CPU at zero
  once checking ends.

Manual check list on the Mac: build with `sh scripts/build-app.sh`, open
`target/Mathstral.app` from a terminal with the key exported, and paste the three
test documents from `crates/mls/tests/fixtures/documents.rs`. Expect grey, then
the expected colours. Hover each dot, and click one to pin it and toggle Lean.
Edit a statement and watch only it and the blocks below re-check. Save, quit,
reopen: the dots go grey and back with no network traffic. Unset the key and
relaunch: one red dot explains why.

### Known limitations (V1)

- Green means the formalization is verified. Whether it is faithful is up to the
  reader of the assumptions.
- A block keeps its Lean when blocks above it change. If a definition above is
  renamed, a statement that used the old name turns red until it is edited.
- A paragraph is a line: a display formula alone on a line is its own (skipped)
  block. A proof must end with ∎, or be followed by another paragraph, to be
  checked.
- One job runs at a time. Jev labels every new paragraph before any
  formalization, and a long document's first pass takes a while.
- Lean re-checks every block on open (about 10 s to import Mathlib, then the
  blocks). The twin restores labels and Lean, not verdicts.
- A service failure (no key, network, Lean not built) halts checking at that
  block until the next edit.
- No repair loops, no dependency graph and no back-translation, by design.
- The denylist of Lean commands is not a sandbox. The REPL runs with your user's
  permissions, without the API key.
