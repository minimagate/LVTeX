<h1 align="center"><img src="assets/brand/lvtex.svg" width="360" alt="LVTeX"></h1>

<p align="center">Write mathematics in prose. Let Lean check it.</p>

<p align="center"><sub>Alpha 2 · native macOS app for Apple Silicon · written in Rust</sub></p>

LVTeX **alpha 2** is a native macOS editor for prose with inline math, written
in Rust. Press **⌘1** to write a formula with TeXmacs-style keys, or **⇧⌘1** to
describe one to a local Ollama model; MiTeX and Typst render formulas in the
background. Optionally, MLS, the Math Language Server, checks each definition,
statement and proof against Lean and Mathlib, and shows the result as a small dot
beside the block.

## Highlights

- **Inline math that stays prose.** ⌘1 opens a formula with TeXmacs-style keys.
  MiTeX and Typst render it in the background, with no TeX install.
- **AI formulas that stay on your Mac.** ⇧⌘1 turns a description into LaTeX with
  Gemma, run locally by Ollama. Nothing leaves the machine.
- **Lean checking with MLS.** Each definition, statement and proof is checked, and
  each block gets one dot: grey while it is checked, amber when a statement compiles
  with `sorry`, green when it is verified down to everything it uses, and red on an
  error. Paragraphs that belong together are checked as one block.
- **The Structure chart (⌘L).** A quiet chart at the top right lists each definition
  and statement, with lines to the blocks it uses. Click a row to scroll to its block.
- **Plain Markdown files.** Documents stay Markdown-compatible text. Saving also
  writes the Lean twin, `<name>.mls.lean`, beside the file.
- **Quiet by design.** Dots, panels and the chart sit over the page, so the layout
  never shifts.

## What's new in alpha 2

- **Lean checking (MLS).** Jev labels each paragraph and Claude Sonnet 5.5 writes the
  Lean, through OpenRouter. A Lean REPL with Mathlib checks it on this Mac.
- **Blocks that belong together.** A setup line and its claim, a statement and the
  display formula under it, or a proof over several paragraphs form one block.
- **Links between blocks.** A block that uses a name declared above is linked to the
  block that declares it, and a failed block turns the blocks that use it red.
- **The Structure chart (⌘L).** One row per definition and statement, with its level
  and the links between them.
- **The Lean twin.** Saving writes `<name>.mls.lean`, and opening restores labels and
  Lean from it.
- **Settings (⌘,).** The OpenRouter key is saved in the login Keychain, and a switch
  turns Lean checking on or off.

## Install

Download the release zip, unzip it, and move `LVTeX.app` to `/Applications`.
macOS on Apple Silicon is the supported platform. The app is ad-hoc signed, not
notarized; on first launch, right-click it and choose **Open**. If macOS still
blocks your downloaded copy:

```sh
xattr -dr com.apple.quarantine /Applications/LVTeX.app
```

For AI prompts, install [Ollama](https://ollama.com), run it locally on
`127.0.0.1:11434`, and download the configured model:

```sh
ollama pull gemma4:e4b
```

The model and address are set in [`app/src/config.rs`](app/src/config.rs).
No TeX distribution is required. Only AI prompts need Ollama: writing, manual math,
saving and PDF export work without it.

For Lean checking, add an [OpenRouter](https://openrouter.ai) key in
**LVTeX → Settings…** (⌘,); it is saved to your login Keychain. You can also
export `OPENROUTER_API_KEY` in the shell you launch LVTeX from. When it is set, it
takes precedence over the Keychain. Build the Lean project once (see
[Lean checking (MLS)](#lean-checking-mls)):

```sh
export OPENROUTER_API_KEY=…     # optional; for example in ~/.zshrc
open -an LVTeX               # or run target/release/lvtex
```

An installed copy checks with the Lean project in
`~/Library/Application Support/LVTeX/lean`; see [Lean project](#lean-project).

### Privacy: what runs where

| Feature | Runs | Sends |
| --- | --- | --- |
| Writing, manual math, PDF export | this Mac | nothing |
| AI formulas (⇧⌘1) | Ollama on this Mac (`gemma4:e4b`) | nothing leaves the Mac |
| Lean checking: labels and formalization | OpenRouter (Jev, Claude Sonnet 5.5) | the paragraphs being checked, with the prose and Lean above them |
| Lean checking: running the Lean | Lean REPL on this Mac | nothing |

Turning Lean checking off in Settings stops new OpenRouter requests.

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
| Settings… | ⌘, |
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

## Lean checking (MLS)

<img src="assets/brand/mls.svg" width="88" alt="MLS" align="right">

MLS, the Math Language Server, reads the document a block at a time. A
paragraph is a line; Enter closes it.
For each paragraph, Jev (`typesafe/jev-1.13`) answers three questions: what it is (a
definition, a statement (lemma, theorem, corollary, proposition), a proof, or
anything else, which gets no dot), whether it belongs to the same item as the
block above, and whether it holds several items. Jev sees that whole block, every
paragraph grouped into it so far, not only its last line. Paragraphs that belong
together form one block: a setup line and its claim, a statement and the displayed
formula under it, or a proof that runs over several paragraphs until one that is not
part of it, or until it ends with ∎.
A block's dot sits beside its first paragraph. A paragraph cut into several items
(below) still has one dot, coloured by the worst of its items: red, then grey, then
amber, then green. A proof does not need ∎: Enter closes it like any other block.
The last block waits, with no dot and no request,
only while it ends in `:` or `,`. A convention that only fixes notation ("Throughout,
$n$ is a natural number") declares nothing and gets no dot.

A paragraph that holds several items, such as a statement followed by its proof, or a
definition followed by a claim about it, needs no paragraph break. Claude Sonnet 5.5
cuts it into verbatim pieces, and each piece is read as its own definition, statement
or proof, exactly like a paragraph: a proof piece pairs with the statement piece
directly above it, even in the same paragraph, and a piece can join the paragraph
below. A piece is never cut again. A cut that does not reproduce the paragraph
exactly is rejected, and the paragraph is read whole.

Claude Sonnet 5.5 (`anthropic/claude-sonnet-5.5`) formalizes each block into Lean as
one item, given the Lean of the blocks above and the prose run above it; statements
are formalized with `sorry`, and a proof restates its statement with the real proof.
Output that uses `axiom`, `admit`, `sorry` in a definition or proof, commands such
as `#eval` or `set_option`, redefines a name from above, or changes the statement
its proof belongs to, is rejected before Lean sees it. The Lean REPL then checks the
block on top of the blocks above.

If Lean reports an ordinary error in a proof, MLS makes exactly one repair attempt.
The block stays grey while it repairs. The failed Lean and its error go, with the
original context, to the same formalizer, and the repaired Lean must pass the same
checks as the original before it is checked again on the same base. If the repair
also fails, the block turns red. There are no further retries.

After a formalization passes these rules, MLS records which names declared by the
blocks above it appear in its Lean (whole words outside comments, except names the
block declares itself). These links drive checking, with no model calls. A block that
uses a name that no block above declares any more (renamed or removed) is
formalized again. A block that uses a red block is red, with the note "Uses `X`,
which does not check.", and Lean is not run. A block whose Lean verifies but that
uses a statement that is only amber stays amber, with "Relies on `X`, which is not
proved yet." If a block above has no valid Lean at all, a missing name is red, with
"Uses `X`, which is not defined above.", and the formalizer is not asked again.

| Dot | Meaning |
| --- | --- |
| Grey | Being checked |
| Amber | The statement compiles with `sorry`, or the block relies on one that does |
| Green | Verified: a definition, or a statement together with its proof, and everything it uses |
| Red | Lean reported an error, the output was rejected, or the block uses one that does not check |

Prose, examples and remarks get no dot. Hover or click a dot to see the block's
panel: a note first, when a link explains the colour; then the assumptions in
plain language, written from the Lean; then `Uses a, b.` for the names it uses from
above. A dot over several items lists each with its label and status, then its
assumptions and uses. **Show Lean** reveals the Lean and any error of every item.
The panel opens below the block's last paragraph. Escape or a click elsewhere
closes a pinned panel.
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
checked, with a `-- lvtex:block <id>` comment per block, and a final
`lvtex:memory` comment (`version` 3). It remembers each paragraph's and piece's
label, the block above it and whether it joins that one, whether a paragraph holds
several items and its pieces, and each block's Lean, assumptions and names it uses,
or its rejection reason. Opening the document re-uses them, so unchanged text needs no
model calls; Lean re-checks every block. A version 1 or 2 twin is ignored, so its
paragraphs are asked again once. MLS needs no Ollama.

### Settings

**LVTeX → Settings…** (⌘,) opens a quiet panel over the page, under the title
bar, in two sections. Escape closes it, and so does a click outside it.

- **Writing · on this Mac** says that AI formulas (⇧⌘1) use `gemma4:e4b` through
  Ollama at `127.0.0.1:11434`, that nothing is sent anywhere else, and whether
  Ollama is running.
- **Lean checking · OpenRouter** holds the switch **Check definitions, statements
  and proofs with Lean**, a line on what runs where, the key status and the key
  field.

With the switch off, MLS sends no requests, shows no dots, and the Structure chart
says "Lean checking is off." The choice is remembered in a small `mls` file in
`~/Library/Application Support/LVTeX`, beside `last-file`, holding `on` or `off`.
A new install checks.

The key field is masked: each character shows as a bullet, up to 48. Paste (⌘V) or type the
key, then press Return. LVTeX saves it to the login Keychain (item "LVTeX
OpenRouter", account "openrouter") through the system `security` tool, which gets the
command on its standard input, so the key never touches a file or a process argument.
**Remove key from Keychain** deletes it. `OPENROUTER_API_KEY`, when it is set, takes
precedence over the Keychain, and the status line says which source is in use. If the
Keychain cannot keep the key, LVTeX uses it until it quits.

### Lean project

The Lean project is [`lean/`](lean): Lean 4.35.0 with Mathlib and the
[Lean REPL](https://github.com/leanprover-community/repl), pinned in
`lake-manifest.json`. Build it once with [elan](https://github.com/leanprover/elan)
installed:

```sh
cd lean
lake exe cache get            # downloads prebuilt Mathlib
lake build Mathlib repl
```

A source build of Mathlib without the cache takes hours. LVTeX looks for the
project in this order: `LVTEX_LEAN_PROJECT` when it is set; then
`~/Library/Application Support/LVTeX/lean` when it exists; then the `lean/` of
the checkout it was built from.

An installed copy (the release zip) has no checkout. Copy the repository's `lean/`
files (`lakefile.toml`, `lean-toolchain` and `lake-manifest.json`) to
`~/Library/Application Support/LVTeX/lean`, then run the same `lake exe cache get`
and `lake build Mathlib repl` in that folder. A source build uses the checkout's
`lean/`; `LVTEX_LEAN_PROJECT` overrides both. The model ids live in
[`crates/mls/src/lib.rs`](crates/mls/src/lib.rs).
HTTPS requests go through the system `curl`, which reads the key from its
standard input, never from its arguments.

## Files

Documents are Markdown-compatible text:

- Inline math is `$…$`; display math uses `$$` on separate lines.
- AI source prompts follow formulas in `<!--lvtex:BASE64URL-->` comments
  (UTF-8, base64url without padding).
- Pending AI prompts are saved as `<!--lvtex:pending:BASE64URL-->` comments
  (UTF-8, base64url without padding) and resume when opened.
- Literal dollar signs in prose use `\$`.
- An inline formula ending in a TeX `%` comment gains a newline before its closing
  `$`, so the delimiter is not swallowed by the comment.

Settings live in `~/Library/Application Support/LVTeX`. The Lean checking switch
is the `mls` file there, beside `last-file`. A key saved in Settings is in the login
Keychain, not in a file. The on-disk document and settings formats are compatibility
boundaries.

## Development

Build with Rust 1.92 or newer and the macOS command-line tools. Packaging also
uses `python3`, `plutil`, `codesign` and `ditto`.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --release --locked --workspace
sh scripts/build-app.sh     # target/LVTeX.app
sh scripts/install.sh       # /Applications/LVTeX.app
sh scripts/package.sh       # dist/LVTeX-<version>-alpha-macos-<arch>.zip
```

The build script uses the repo-local `.cargo-home` offline when that cache exists;
a fresh clone uses the normal Cargo installation. Tests run without a window,
Ollama, OpenRouter, Lean or a TeX installation. The optional checks are:

```sh
# Manual-preview timing.
cargo test --release --locked -p lvtex manual_preview_latency -- --ignored --nocapture
# The four sample documents with scripted model answers, checked by Lean (needs lean/ built).
cargo test --release --locked -p lvtex-mls scripted_documents -- --ignored --nocapture
# Eight documents through OpenRouter and Lean, three fresh runs each (needs OPENROUTER_API_KEY and lean/ built).
cargo test --release --locked -p lvtex-mls --test documents -- --ignored --nocapture
```

The live documents are the four scripted ones plus the supplied note, prime notation, a long
paragraph and plane energy and orthogonality. Each run writes raw results to `target/mls-live/<phase>/`: a JSON record per document and
run (every job and reply, the final dots, the chart and the pinned model ids) and the Mls twin. Narrow
a live run with environment variables:

```sh
MLS_LIVE_RUNS=1 cargo test ... --test documents -- --ignored --nocapture        # one run, not three
MLS_LIVE_FILTER=prime cargo test ... --test documents -- --ignored --nocapture  # documents whose name contains "prime"
MLS_LIVE_PHASE=retry-1 cargo test ... --test documents -- --ignored --nocapture # target/mls-live/retry-1/
```

The scripted test skips with a message when the Lean project is missing. The live test fails
explicitly when `OPENROUTER_API_KEY` or the Lean REPL is missing, because it runs only on request.
`crates/mls` builds and tests on Linux too; the app itself is macOS-only.

| Path | Responsibility |
| --- | --- |
| `app/src/app/` | Window state, events, drawing, commands, math sessions, Lean dots and panel, Structure chart, Settings |
| `app/src/document/` | Text, formulas, file encoding, cursor and undo |
| `app/src/math/` | Formula keys, tree, LaTeX parsing, Ollama and background work |
| `app/src/view/` | Typesetting, hit testing, pixel rendering and carets |
| `app/src/{config,export,file,keychain,markdown,menu}.rs` | Settings constants, PDF export, platform I/O, the login Keychain, Markdown and menus |
| `crates/latex/` | MiTeX conversion and bundled Typst compatibility definitions |
| `crates/render/` | Typst rendering and PDF compilation with embedded fonts |
| `crates/mls/` | Lean checking: block state, OpenRouter, rejection rules, Lean REPL worker |
| `lean/` | Pinned Lake project with Mathlib and the Lean REPL |
| `app/tests/`, `crates/*/tests/` | Unit tests, integration scenarios and fixtures |
| `assets/brand/` | LVTeX and MLS wordmarks |
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
