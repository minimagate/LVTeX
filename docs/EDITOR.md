# Editor reference

This document describes the behavior of the Mathstral editor. The
[README](../README.md) covers installation and basic use.

## Window

- The window uses a transparent title bar with a full-size content view. The title is
  hidden; the standard close, minimize and zoom buttons remain.
- The window opens at its last size and position. Size and position are stored in
  logical pixels, including a normal window that has been zoomed to the screen edges.
  Full-screen spaces and minimized windows do not overwrite the stored geometry.
- Geometry is saved about 400 ms after a resize and again on exit. Legacy size-only
  settings are still read. A saved position is ignored if it lies outside every
  connected display.
- Settings and the last-opened path are stored in `~/Library/Application Support/Mathstral`.
  On launch, the last-opened file is reopened.
- The idle event loop sleeps until input arrives, a pending window-size save is due, or
  a math result is ready.

Menus:

| Menu | Items |
| --- | --- |
| Mathstral | Hide, Hide Others, Show All, Quit Mathstral (⌘Q) |
| File | New (⌘N), Open… (⌘O), Close Window (⌘W), Save (⌘S), Save As… (⇧⌘S), Export as PDF… (⌥⌘E) |
| Edit | Undo (⌘Z), Redo (⇧⌘Z), Cut (⌘X), Copy (⌘C), Paste (⌘V), Select All (⌘A), Insert Formula (⌘1), Ask AI for Formula (⇧⌘1) |
| Window | Minimize, Zoom, Enter Full Screen |

Close Window (⌘W) quits the application, the same as Quit.

The window can be moved by dragging the top 32 logical pixels or by holding ⌘ and
dragging anywhere in the window.

## Typography and colors

- Text uses embedded Libertinus Serif at 19 px, with a line height of 1.6.
- The column is 680 px wide and starts 76 px from the top. Long lines wrap greedily at word boundaries.
- Colors: background `#151515`; text `#C8C6C3`; markers `#73716F`; selection `#343434`;
  pending text `#868583`; open prompt text `#C8CFCB` with a green tint `#3C5147`;
  failed prompt underline `#70484A`; pending marker `#292B2A`; inline code `#B9C3C6`;
  links `#9FB6C9`.
- The caret is solid `#FF0000`, with no blinking or fading.
- Font family and sizes are set in `app/src/config.rs`.

### Markdown

The editor styles the Markdown source as you type. Markers stay visible and editable,
drawn in the marker color; only the styling changes.

- Headings: `#` is bold, `##` is italic, and `###` to `######` are bold italic, all at body size.
- Bold (`**`), italic (`*` or `_`) and bold italic (`***`) use the Libertinus Serif faces.
- Inline code (`` ` ``) uses DejaVu Sans Mono at 85% of body size, in `#B9C3C6`.
- Strikethrough (`~~`) draws a 1 px line through the struck text.
- Links `[text](url)` show the text in `#9FB6C9`; the brackets and URL stay dimmed.
- Bullets (`-`, `*`, `+`), numbers (`1.` or `1)`) and tasks (`- [ ]`, `- [x]`) hang wrapped
  rows under their content. Done tasks are dimmed.
- Quotes (`>`) are italic.
- A line of three or more `-`, `*` or `_`, with optional spaces, draws a 1 px rule across the column.
- Nothing is styled inside an open math prompt. Fenced code blocks, tables and images are not styled.
- Backslash escapes are not supported. Typing `\` opens the hand-written math editor; use Ask AI for Formula for a natural-language prompt.
  A literal `*` or `_` that cannot open emphasis, such as in `2 * 3`, stays plain.

## Basic editing

- Arrow keys move by character, and ↑/↓ move by visual line while keeping the goal column.
- ⌥← / ⌥→ move by word. ⌘← / ⌘→ move to the start or end of the visual line.
- ⌘↑ / ⌘↓ move to the start or end of the document. Home and End do the same.
- Shift extends the selection with any of these movements.
- Backspace and Delete remove one character. ⌥ with either removes a word; ⌘ with either
  removes to the end of the visual line in that direction.
- Tab inserts four spaces.
- Enter in a list item, task or quote continues its marker (numbers increase, tasks restart
  unchecked). Enter on an empty item removes the marker and ends the list.
- Typing runs are grouped into one undo step. Losing window focus ends the current group.
- Mouse: click places the caret, drag selects, Shift-click extends the selection, and the
  wheel or trackpad scrolls.
- During input-method composition, key events are ignored and the committed text is inserted.

File commands:

- Open accepts `.md`, `.markdown` and `.txt`. CRLF and CR line endings are read as LF.
- Save As adds `.md` when the chosen name has no extension.
- Save writes to a temporary file in the same folder, copies the file permissions, and
  renames it over the target. Symbolic links are resolved first.
- New, Open, Close Window and Quit ask "Save changes?" when the document has unsaved edits.
- Export as PDF writes an A4 PDF typeset with Typst: black text on white, formulas as vector
  math. The PDF typesets the Markdown styles the editor shows (headings, bold, italic, code,
  strikethrough, links, lists, tasks, quotes and rules) with the markers left out. Prompts that
  have not rendered are exported as their `\prompt\` text after a confirmation.

## AI formula prompts

Choose Edit → Ask AI for Formula (⇧⌘1) to open an AI prompt. Selected text becomes the
initial prompt; with no selection, the prompt starts empty. At a position inside a word,
the editor adds a space around the prompt so the saved `\prompt\` form remains readable.

- Press Enter to submit the prompt. Escape leaves the prompt text as literal text.
- Existing `\prompt\` files and copied prompts remain supported.
- An open prompt has a subtle green tint and underline.
- A closed prompt is dim until the model returns a formula that renders successfully.
- A prompt whose generation fails stays as text with a muted red underline. Enter retries it.
- Inline formulas share the text baseline. A formula alone on its line is centered with
  display spacing.
- Backspace just after a formula reopens it. A prompt returns to its prompt text, and a
  hand-written or imported formula opens in the hand-written editor.
- Enter at the left edge of a formula reopens it, or retries a failed prompt. Enter after a formula starts a new line.
- Closing an edit whose text is unchanged reuses the existing rendered result without a model request.
- Escape inside an open prompt cancels it and keeps the text as literal characters.
- Formulas are atoms for caret movement and selection. Copy and paste carry the portable file encoding.

## Hand-written math

Typing `\` starts a named command in a new formula. For example, `\alpha` then Enter
inserts α; press Enter again to close the formula. Pressing ⌘1 (Edit → Insert Formula)
opens a blank formula for ordinary TeXmacs-style keys such as `x` `^` `2`. The formula previews as
you type on the local render worker; key handling never waits for typesetting. Keys follow
TeXmacs's `math-kbd.scm`.

- Enter closes the formula. With a `\command` pending, Enter confirms the command instead;
  press Enter again to close. Closing by click or a file command also confirms a pending command.
- Escape cancels a pending `\command`. Otherwise it abandons the edit: a new formula is
  removed, and a reopened formula returns to its previous source.
- A click in the text, Save, Open, New, Export as PDF or Quit closes the formula first.
  Insert Formula, Copy, Cut, Paste and Select All do nothing inside a formula.
- ⌘Z and ⇧⌘Z undo and redo edits inside the formula.
- The whole edit is one undo step.
- Saving stores the formula as plain `$latex$`, with no prompt comment. A formula alone
  on its line displays.

| Keys | Result |
| --- | --- |
| `_` `^` | Subscript and superscript. |
| `'` | Prime. `"` gives a double prime. |
| ⌥F | Fraction. The caret starts in the numerator. |
| ⌥S | Square root. |
| ⌥⇧S | Nth root. The caret starts in the index. |
| `(` `[` `{` `\|` | Opens a bracket pair. Typing the closer at the end of the body sets the closing delimiter and steps out. Elsewhere the closer is a plain character. |
| `a` Tab | Next variant of the symbol before the caret: `a` Tab gives α. Shift Tab steps back. |
| `<` `=` | ⩽. `<` `=` Tab gives ≤. |
| `-` `>` | → |
| `=` `/` | ≠ |
| ⇧F5, then a key | Big operators and symbols: ⇧F5 `S` gives ∑, ⇧F5 `I` gives ∫. |
| `\name` Return | Named symbol or command: `\alpha` Return gives α. `\frac` and `\sqrt` make a fraction and a root. |
| ← → ↑ ↓ | Move through the formula. ↑ and ↓ move between the numerator and denominator, or the index and radicand. |
| ⌘⌥← / ⌘⌥→ | Previous and next argument of the enclosing box. |
| ⌘⌥⌫ / ⌘⌥⌦ | Move the caret out of the enclosing box, before or after it. |
| ⌫ / ⌦ | Backspace after a box enters it, and Backspace removes it once it is empty. Delete mirrors this. |
| ⌥⌫ | Remove the box before the caret. |
| ⌃⌫ | Replace the enclosing box by its contents. |
| ⌘Z / ⇧⌘Z | Undo and redo inside the formula. |

## Ollama worker

- The worker sends `POST /api/chat` to `127.0.0.1:11434` with `stream: false`,
  `temperature: 0` and `keep_alive: 30m`. The model is the `OLLAMA_MODEL` constant in
  `app/src/config.rs` (currently `gemma4:e4b`).
- A warm-up request is sent at launch. While Ollama is unreachable, the worker retries
  periodically, and queued jobs resume when it connects.
- A status line reading "Ollama unavailable · math will resume when it connects" is shown
  while Ollama is unreachable.
- One worker serializes model requests. Editing continues while requests are pending.
- Local formula renders use a separate worker, so saved math and live previews continue while
  Ollama is slow or unavailable. Live previews keep only the newest queued edit; older results
  are discarded. The current LaTeX source stays visible while its preview is pending or fails.
  A native caret is shown until the first preview bitmap arrives; preview bitmaps contain their
  own math caret. After closing an edit, its last preview stays visible until the final render
  replaces it.
- The system prompt and few-shot examples are in `app/src/math.rs`. They cover integrals,
  sums, fractions, matrices, cases, aligned equations, sets and Greek letters.
- Responses are cleaned of fences, delimiters and surrounding prose. Output containing
  prose is rejected. The result is rendered before it is accepted.
- If rendering fails, the error is sent back to the model. There are at most three attempts
  in total: the first request and two repairs.
- The LaTeX converter rejects input nested more than 64 braces deep.
- Each render or model job carries a stable atom ID. Results are matched by that ID, so late
  results for edited or deleted formulas are discarded without changing the document.
- Rendered results are cached by prompt. The cache holds up to 512 entries and is cleared when full.
- Both inline and display formulas are rasterized on the worker.

## Undo and redo

- Model and render results do not add undo steps.
- Redo restores a formula's rendered image when it is available.
- A hand-written formula edit is one undo step. ⌘Z inside the formula undoes its own edits first.

## File format

- Inline math: `$body$`.
- Display math: `$$` on its own line, the body on the lines between, and `$$` on its own line.
- Prompt source: a comment `<!--mathstral:BASE64URL-->` directly after the formula. The payload
  is the UTF-8 original prompt, encoded in base64url without padding. Markdown readers hide it.
- The reserved `<!--mathstral:literal-->` comment protects literal backslashes inside prompts,
  backslashes cancelled with Escape, and dollar signs in copied prompts from delimiter parsing.
- Pending AI prompts are saved as `\prompt\` and resume on open.
- Literal `$` in prose is saved as `\$`. Reading `\$` produces `$`.

## Imported math

- A formula in a file with no prompt comment renders directly from its LaTeX source.
- Backspace just after it, or Enter at its left edge, opens it in the hand-written editor.
  Its LaTeX is parsed into the editor. Syntax the editor does not model, including standalone
  brace groups, stays as typed. Reopening it without changing its structure preserves its
  original source form. Input nested more than 64 levels remains raw.
- Imported math is never sent to the model. Renders of imported math and cached prompts work
  without Ollama.

## Verification

Run the test suite with:

```sh
cargo test --release --locked --workspace
```

The editor tests cover:

- Unicode editing, backslash ambiguity and cancellation.
- Hand-written formula keys, tree editing and save as plain LaTeX.
- Atom selection, copy and paste.
- Undo and redo.
- Late and deleted results.
- Generation repair and caching.
- Offline queue recovery.
- Baseline and centering geometry.
- Pixel-identical save and reopen.

Sample output:

- [`artifacts/editor-math.png`](../artifacts/editor-math.png): a mixed inline and display page.
- [`artifacts/math-demo.md`](../artifacts/math-demo.md): the portable source of that page.
- [`artifacts/live-math.md`](../artifacts/live-math.md): the document produced during
  native checks with the installed model, which exercised prompts, cancellation,
  regeneration, undo and redo, and reopening rendered math.

## Performance

- Document updates, layout and dirty checking measured p95 of about 0.94 ms in a debug build,
  with zero and with ten pending prompts.
- The release test suite enforces a p95 budget of 16 ms for the same path.
- These measurements do not include display presentation latency.

## Build

```sh
cargo test --release --locked --workspace
sh scripts/build-app.sh      # writes target/Mathstral.app
sh scripts/package.sh        # writes the release zip to dist/
```

Launch `target/Mathstral.app`. Prompt conversion needs Ollama running locally with the
configured model. Writing, file operations and rendering of imported math work without it.
