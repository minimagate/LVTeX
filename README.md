# Mathstral

Mathstral **alpha 2** is a native macOS editor for prose with inline math, written
in Rust. Press **⌘1** to write a formula with TeXmacs-style keys, or **⇧⌘1** to
describe a formula to a local Ollama model. MiTeX and Typst render formulas in the
background. Writing, manual math, saving and PDF export work without Ollama.
Optionally, MLS checks each definition, statement and proof against Lean and
Mathlib, and shows the result as a small dot beside the block.

## Install

Download the release zip, unzip it, and move `Mathstral.app` to `/Applications`.
macOS on Apple Silicon is the supported platform. The app is ad-hoc signed, not
notarized; on first launch, right-click it and choose **Open**. If macOS still
blocks your downloaded copy:

```sh
xattr -dr com.apple.quarantine /Applications/Mathstral.app
```

For AI prompts, install [Ollama](https://ollama.com), run it locally on
`127.0.0.1:11434`, and download the configured model:

```sh
ollama pull gemma4:e4b
```

The model and address are set in [`app/src/config.rs`](app/src/config.rs).
No TeX distribution is required.

For Lean checking, export an [OpenRouter](https://openrouter.ai) key in the shell
you launch Mathstral from, and build the Lean project once (see
[Lean checking](#lean-checking)):

```sh
export OPENROUTER_API_KEY=…     # for example in ~/.zshrc
open -a Mathstral               # or run target/release/mathstral
```

## Writing

The editor styles Markdown while leaving its source markers visible: headings,
bold, italic, inline code, strikethrough, links, lists, tasks, quotes and rules.
Enter continues a list or quote; Enter on an empty item ends it. Fenced code
blocks, tables and images are not styled.

Standard macOS editing shortcuts work. Option-arrow moves by word, Command-arrow
moves to a line or document boundary, and Shift extends a selection. Tab inserts
four spaces. Drag to select text; drag the top strip or Command-drag to move the
window. The app remembers its window geometry and last-opened file.

| Command | Shortcut |
| --- | --- |
| New / Open / Save | ⌘N / ⌘O / ⌘S |
| Save As | ⇧⌘S |
| Export as PDF | ⌥⌘E |
| Undo / Redo | ⌘Z / ⇧⌘Z |
| Cut / Copy / Paste / Select All | ⌘X / ⌘C / ⌘V / ⌘A |
| Insert Formula | ⌘1 |
| Ask AI for Formula | ⇧⌘1 |
| Structure chart (View → Structure) | ⌘L |
| Close / Quit | ⌘W / ⌘Q |

Saving replaces the file atomically and preserves its permissions. Opening
normalizes CRLF and CR line endings to LF. New, Open and Quit ask about unsaved
changes. PDF export produces A4 pages with vector formulas and Markdown styling,
without source markers. Unrendered formulas export as text after confirmation.

## Formulas

**Manual math:** press ⌘1, then type `x^2`, for example. The caret is blue. Inside
a formula, `\alpha` followed by Enter inserts α; another Enter finishes the
formula. In prose, a backslash is an ordinary character.

| Keys inside a formula | Action |
| --- | --- |
| `_` / `^` / `'` | Subscript / superscript / prime |
| ⌥F / ⌥S / ⌥⇧S | Fraction / square root / indexed root |
| `(`, `[`, `{`, `\|` | Open paired delimiters; a closer at the body's end exits |
| Tab / ⇧Tab | Next / previous symbol variant (`a` Tab gives α) |
| `-` then `>` / `=` then `/` | Arrow / not-equal symbol |
| ⇧F5, then `S` or `I` | Sum or integral |
| `\name` then Enter | Named command; `\frac` and `\sqrt` open their boxes |
| Arrow keys | Move through the formula and between stacked arguments |
| ⌘⌥← / ⌘⌥→ | Previous / next argument |
| ⌘⌥Backspace / ⌘⌥Delete | Exit before / after the enclosing box |
| Backspace / Delete | Delete a symbol, enter a box, or remove an empty box |
| ⌥Backspace / ⌃Backspace | Remove the preceding box / unwrap the enclosing box |
| ⌘Z / ⇧⌘Z | Undo / redo within the formula |

Enter finishes an edit; Escape cancels a pending command first, then cancels the
formula edit. A click in prose or a file command finishes it. The whole formula
edit is one document undo step. Clipboard commands are unavailable inside manual
math in this prototype.

**AI math:** press ⇧⌘1; selected text becomes the prompt. The caret is green.
Enter submits; Escape keeps the prompt as literal prose. Pending prompts are
dimmed, and failed prompts have a red underline. One model request runs at a
time; queued prompts resume when Ollama and the model are available. Manual math
and existing formulas render on a separate worker.

Backspace just after a formula, or Enter at its left edge, reopens it. AI formulas
return to their prompt; manual and imported formulas open in the manual editor.
Enter on a failed prompt retries it. A formula alone on its line is centered.
Formulas are single units for selection and copying. Unsupported imported LaTeX
is retained as raw source; reopening without edits preserves the original source.

## Lean checking

MLS reads the document a block at a time. A paragraph is a line; Enter closes it.
For each paragraph, Jev (`typesafe/jev-1.13`) answers two questions: what it is (a
definition, a statement (lemma, theorem, corollary, proposition), a proof, or
anything else, which gets no dot), and whether it belongs to the same item as the
paragraph above. Paragraphs that belong together form one block: a setup line and
its claim, a statement and the displayed formula under it, or a proof that runs
over several paragraphs until one that is not part of it, or until it ends with ∎.
A block's dot sits beside its first paragraph. The last block gets no dot and no
request while it is an unfinished proof or ends in `:` or `,`.

Claude Haiku 5.5 (`anthropic/claude-haiku-5.5`) formalizes each block into Lean as
one item, given the Lean of the blocks above and the prose run above it; statements
are formalized with `sorry`, and a proof restates its statement with the real proof.
Output that uses `axiom`, `admit`, `sorry` in a definition or proof, commands such
as `#eval` or `set_option`, redefines a name from above, or changes the statement
its proof belongs to, is rejected before Lean sees it. The Lean REPL then checks the
block on top of the blocks above.

After a formalization passes these rules, MLS records which names declared by the
blocks above it appear in its Lean (whole words outside comments, except names the
block declares itself). These links drive checking, with no model calls. A block that
uses a name that no block above declares any more (renamed or removed) is
formalized again. A block that uses a red block is red, with the note "Uses `X`,
which does not check.", and Lean is not run. A block whose Lean verifies but that
uses a statement that is only amber stays amber, with "Relies on `X`, which is not
proved yet." If a block above has no valid Lean at all, a missing name is red, with
"Uses `X`, which is not defined above.", and Haiku is not asked again.

| Dot | Meaning |
| --- | --- |
| Grey | Being checked |
| Amber | The statement compiles with `sorry`, or the block relies on one that does |
| Green | Verified: a definition, or a statement together with its proof, and everything it uses |
| Red | Lean reported an error, the output was rejected, or the block uses one that does not check |

Prose, examples and remarks get no dot. Hover or click a dot to see the block's
panel: a note first, when a link explains the colour; then the assumptions in
plain language, written from the Lean; then `Uses a, b.` for the names it uses from
above. **Show Lean** reveals the Lean and any error. The panel opens below the
block's last paragraph. Escape or a click elsewhere closes a pinned panel.
Checking runs in the background and never blocks typing. Editing a block
re-formalizes that block, and any block below it that used a name the edit removed
or renamed; the blocks below are re-checked with Lean. Editing a paragraph also asks
Jev again about it and about the paragraph below; until Jev answers, the blocks and
dots stay where they were.

**View → Structure** (⌘L) opens a quiet chart at the top right of the window: one
row per definition and statement, in document order, with its Lean name (or the
start of its text, when it has none) and its status colour. A row's dot is indented
by its level: 0 for a block that uses no other block, otherwise one more than the
deepest block it uses. Thin lines run from each block to the blocks it uses, and a
proof adds what it uses to its statement's row. Click a row to scroll to its block.
Escape closes the panel first, then the chart. The chart only shows Lean that
exists, so a block with no Lean yet has no links.

Green means *this Lean formalization is verified*, not that it says what you
wrote. The assumptions panel is how you check that it does. Green also requires
everything the block uses, directly or through other blocks, to be verified.

A missing key, an unreachable OpenRouter or a Lean project that is not built shows
as one red dot whose panel explains what to do; the next edit retries. Saving
also writes the document's Lean twin, `<name>.mls.lean`, beside it: the Lean that
checked, with a `-- mathstral:block <id>` comment per block, and a final
`mathstral:memory` comment (`version` 2). It remembers each paragraph's label, the
paragraph above it and whether it joins that one, and each block's Lean,
assumptions and names it uses, or its rejection reason. Opening the document re-uses
them, so unchanged text needs no model calls; Lean re-checks every block. A version 1
twin is ignored, so its paragraphs are asked again once. MLS needs no Ollama.

The Lean project is [`lean/`](lean): Lean 4.35.0 with Mathlib and the
[Lean REPL](https://github.com/leanprover-community/repl), pinned in
`lake-manifest.json`. Build it once with [elan](https://github.com/leanprover/elan)
installed:

```sh
cd lean
lake exe cache get            # downloads prebuilt Mathlib
lake build Mathlib repl
```

A source build of Mathlib without the cache takes hours. Mathstral uses `lean/`
of the checkout it was built from; set `MATHSTRAL_LEAN_PROJECT` to use another
project. The model ids live in [`crates/mls/src/lib.rs`](crates/mls/src/lib.rs).
HTTPS requests go through the system `curl`, which reads the key from its
standard input, never from its arguments.

## Files

Documents are Markdown-compatible text:

- Inline math is `$…$`; display math uses `$$` on separate lines.
- AI source prompts follow formulas in `<!--mathstral:BASE64URL-->` comments
  (UTF-8, base64url without padding).
- Pending AI prompts are saved as `<!--mathstral:pending:BASE64URL-->` comments
  (UTF-8, base64url without padding) and resume when opened.
- Literal dollar signs in prose use `\$`.
- An inline formula ending in a TeX `%` comment gains a newline before its closing
  `$`, so the delimiter is not swallowed by the comment.

Settings live in `~/Library/Application Support/Mathstral`. The on-disk document
and settings formats are compatibility boundaries.

## Development

Build with Rust 1.92 or newer and the macOS command-line tools. Packaging also
uses `python3`, `plutil`, `codesign` and `ditto`.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --release --locked --workspace
sh scripts/build-app.sh     # target/Mathstral.app
sh scripts/install.sh       # /Applications/Mathstral.app
sh scripts/package.sh       # dist/Mathstral-<version>-alpha-macos-<arch>.zip
```

The build script uses the repo-local `.cargo-home` offline when that cache exists;
a fresh clone uses the normal Cargo installation. Tests run without a window,
Ollama, OpenRouter, Lean or a TeX installation. The optional checks are:

```sh
# Manual-preview timing.
cargo test --release --locked -p mathstral manual_preview_latency -- --ignored --nocapture
# The four sample documents with scripted model answers, checked by Lean (needs lean/ built).
cargo test --release --locked -p mathstral-mls scripted_documents -- --ignored --nocapture
# The same documents through OpenRouter at temperature 0 (needs OPENROUTER_API_KEY too).
cargo test --release --locked -p mathstral-mls --test documents -- --ignored --nocapture
```

Each optional MLS test skips with a message when its key or Lean project is missing.
`crates/mls` builds and tests on Linux too; the app itself is macOS-only.

| Path | Responsibility |
| --- | --- |
| `app/src/app/` | Window state, events, drawing, commands, math sessions, Lean dots and panel, Structure chart |
| `app/src/document/` | Text, formulas, file encoding, cursor and undo |
| `app/src/math/` | Formula keys, tree, LaTeX parsing, Ollama and background work |
| `app/src/view/` | Typesetting, hit testing, pixel rendering and carets |
| `app/src/{file,menu,markdown,export,config}.rs` | Platform I/O, menus, Markdown, PDF export and settings constants |
| `crates/latex/` | MiTeX conversion and bundled Typst compatibility definitions |
| `crates/render/` | Typst rendering and PDF compilation with embedded fonts |
| `crates/mls/` | Lean checking: block state, OpenRouter, rejection rules, Lean REPL worker |
| `lean/` | Pinned Lake project with Mathlib and the Lean REPL |
| `app/tests/`, `crates/*/tests/` | Unit tests, integration scenarios and fixtures |
| `assets/fonts/` | Required fonts and license notices |
| `scripts/` | App build and release packaging |
| `.agents/skills/`, `AGENTS.md` | Repository guidance for coding agents |

Unit tests are external files included with `#[cfg(test)]` and `#[path]`, keeping
private implementation access without widening the production API. Library
integration tests use Cargo's normal `tests/` targets. Test fixtures stay outside
production libraries. There is no hosted CI; the first three commands above are
the required local checks.

## License

[Apache-2.0](LICENSE). Bundled fonts retain their
[upstream licenses](assets/fonts/README.md), and the vendored MiTeX definitions
retain their [license and attribution](crates/latex/vendor/mitex/README.md).
