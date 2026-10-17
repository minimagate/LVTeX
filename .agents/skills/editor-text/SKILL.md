---
name: editor-text
description: Build the text core of a native prose editor: buffer, position units, selection, grapheme-aware movement and deletion, the single edit primitive, undo grouping, IME composition, line endings, wrapping and paste. Use when changing how text is stored, edited, moved through, undone, pasted or laid out, or when writing tests for those behaviours.
---

# Editor Text

The text core is the part that must be right before anything is drawn. Keep it free of window, drawing and platform types so it can be tested as plain Rust.

Threading and ownership live in `../rust-concurrency/SKILL.md`; module layout in `../rust-structure/SKILL.md`; test layout in `../rust-tests/SKILL.md`.

## Single source of truth

- The buffer is the only stored copy of the document. Layout, visual lines, glyph runs, embedded-object positions and caret geometry are derived from it.
- Every derived cache records the edit range it depends on. An edit invalidates the caches that overlap that range. Never patch derived state by hand from a call site.
- Choose the buffer structure by measurement: a plain string is fine for small documents; a rope or gap buffer suits large ones.

## Positions and units

- Pick one position unit for internal storage. A byte offset into UTF-8 is the usual choice.
- Convert explicitly at each boundary: graphemes for caret movement and deletion, UTF-16 for platform text input and accessibility APIs, visual columns for layout only.
- Never store visual columns as document positions. The goal column is a visual value kept by the selection.
- Never slice a string on an index that is not a character boundary. Route all slicing through one helper that checks the boundary.

## Movement and selection

- Character, grapheme, word and line movement are separate operations. Word movement uses Unicode word boundaries, not whitespace splitting.
- A selection is an anchor and a head. Direction matters: extending back past the anchor flips the selection.
- Vertical movement by visual line keeps a goal column. Horizontal movement and edits reset it.
- Pressing an arrow key with a selection collapses it to the start or end on that side, instead of moving from the head.

## Edits and undo

- All edits go through one edit primitive. It updates the buffer, selection, undo history and cache invalidation together. No caller updates these separately.
- Undo groups consecutive typing into one step. A group ends on a pause, loss of focus, a caret jump, or a structural edit.
- A structural edit, such as inserting or reopening an embedded object, is one atomic undo step.
- Undo and redo restore the selection along with the text.
- Bound the history and say how: by count, by bytes, or by time.

## Input, IME and paste

- While IME composition is active, marked text is displayed but not committed to the buffer or to undo. Only the committed string becomes an edit.
- Normalise pasted line endings to the document's convention and strip control characters other than tab and newline.
- A key event is handled once: either as a command or as text input, never both.
- Use platform-standard keys: the modifier for word and line movement, and the shortcuts the platform menu already advertises.

## Lines, wrapping and caret

- Normalise line endings on load (CRLF and lone CR become LF). On save, write the convention chosen on purpose and document it.
- Wrap at word boundaries when possible, falling back to grapheme boundaries for long unbroken runs.
- Relayout incrementally: re-lay out only the paragraphs an edit touched.
- Keep the caret in view with a margin when moving or typing near an edge.

## Tests to write

- Unicode: combining marks, emoji ZWJ sequences, flags, RTL text if supported, CRLF, lone CR.
- Empty document, a single newline, caret at each end, a selection that starts or ends inside a grapheme cluster.
- Very long lines and very many short lines.
- Undo and redo across grouping boundaries; an IME commit produces exactly one edit.
- Randomised edit sequences keep the buffer valid UTF-8 and every stored position on a boundary.

Keep these tests window-free so they run in the normal test target.
