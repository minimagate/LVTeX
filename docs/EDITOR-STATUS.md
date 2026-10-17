# Math writing — running native app

The native Rust editor is built and running. Launch `target/Mathstral.app`.

The window keeps the standard macOS close, minimize, and zoom/full-screen
controls with a transparent title bar, following the local Railed editor.
It opens as a normal resizable window and remembers its size and screen position
in logical pixels, including a normal window expanded or zoomed to the screen edges.
Separate macOS full-screen spaces and minimized windows do not overwrite that geometry.
Changes are saved after a short debounce and again on exit. Legacy size-only settings
remain readable, and disconnected-monitor positions are ignored.
Settings and the last-opened path live in `~/Library/Application Support/Mathstral`.

The text face is embedded Libertinus Serif at 19px with softer off-white text (#C8C6C3), replacing Cormorant at the
user's request. Font family and dimensions live in `app/src/config.rs`.

The editor uses winit, muda, rfd, cosmic-text, ropey, softbuffer,
unicode-segmentation, and arboard. Layout shapes word boxes and wraps them
greedily; future math atoms can join that stream. Drawing is on the CPU.

Lines beginning with `# ` render in the embedded Libertinus Serif Italic face at
the same 19px size and line height as body text. The hash stays dim and editable.
There are no subtitle styles; `##` and longer prefixes remain plain text.

The caret is solid saturated red (`#FF0000`), with no blinking or fading.
The idle event loop sleeps until input or a pending window-size save.

## Math writing

Type `/prompt/`. Open prompts have a subtle green tint/underline; closed prompts
are dim until Gemma returns a validated formula. Inline formulas share the text
baseline; a formula alone on a line is centered with display spacing. Backspace
from the right edge reopens the original prompt. Enter at the left edge reopens
math (or retries a failed prompt); Enter after math starts a new line. Closing an
unchanged edit reuses its rendered result. Escape leaves literal prompt text.

Slashes open at word starts, and close at punctuation/whitespace/end boundaries.
When the next typed character continues a denominator, a just-closed `/1/` is
reinterpreted as the still-open `/1/2`. URLs and `and/or` remain text.

The worker calls loopback Ollama `/api/chat`, without streaming, at temperature
zero, with a 30-minute keep-alive and launch warm-up. The user selected the
installed `gemma4:e4b`; `app/src/config.rs` holds the single model constant (the
original requested default was `gemma3n:e4b`). Strict few-shot examples cover
integrals, sums, fractions, matrices, cases, aligned equations, sets and Greek.
Responses are cleaned and actually rendered; invalid output gets two repair
attempts. Failure leaves the prompt dim with a muted-red underline. Network
failure leaves jobs queued and displays one bottom status line until reconnection.

Stable atom IDs isolate async results from document positions and history. A
render completion does not add an undo step; redo restores the available image.
Both math styles are rasterized on the worker. Prompt caches retain up to 512
entries; currently one worker serializes model requests while editing stays free.

Files use `$body$` inline or `$$` on separate lines for display. A following
`<!--mathstral:BASE64URL-->` comment stores the UTF-8 original prompt, encoded
without padding. Pending prompts are saved as `/prompt/` and resume on open.
A `<!--mathstral:literal-->` marker preserves explicit Escape cancellations.
These comments are hidden by Markdown readers. Clipboard math uses the same
portable encoding and is parsed before being displayed on paste.

Imported math without a prompt renders directly. Opening it for editing asks
Gemma for a readable description while retaining the formula, then opens the
new description as a prompt. This resolves the conflicting import instructions
in favor of never exposing raw LaTeX. A generated description should be checked
when editing especially complicated imported formulas.

## Verification

The editor suite covers Unicode editing, slash ambiguity, cancellation, atom
selection/copy/paste, undo/redo, late and deleted results, generation repair,
caching, offline queue recovery, baseline/centering geometry, and pixel-identical
save/reopen. A mixed page screenshot is in `artifacts/editor-math.png` and its
portable source in `artifacts/math-demo.md`.

Native UI checks with the installed Gemma verified mixed inline/display writing,
internal slashes, URLs, active/cancelled prompts, Enter and Backspace editing,
regeneration, undo/redo, saving pending prompts, and reopening rendered math.
The live generated document is `artifacts/live-math.md`.

The window-restoration regression was reproduced: native zoom was excluded from
persistence. The corrected running app recorded the expanded normal geometry as
`1512 949 0 33`. The user resumed editing, so a second quit/relaunch was not forced.

The measured debug editing/layout path had p95 around 0.94 ms both with zero and
ten pending prompts. The release test enforces a 16 ms p95 budget. This measures
document updates, layout and dirty checking, not display presentation latency.

## Build

```sh
CARGO_HOME="$PWD/.cargo-home" cargo test --offline --release --locked --workspace
sh scripts/build-editor.sh
```

Launch `target/Mathstral.app`. A running local Ollama service with the configured
model is needed for prompt conversion; plain writing and imported math rendering
remain available without it.
