---
name: editor-math
description: Build inline and display math inside a prose editor: formula objects with stored source, the prompt lifecycle, asynchronous generation and rendering, validation of model output, offline behaviour, typesetting, caret semantics, file format and fixture-based tests. Use when changing how formulas are created, edited, generated, rendered, saved or moved through.
---

# Editor Math

A formula is an object in the text flow with a stored source. The source is the truth; the rendered result is a cache. The user's words are never lost.

Threading and cancellation live in `../rust-concurrency/SKILL.md`. Test layout is in `../rust-tests/SKILL.md`; timing and benchmarks in `../rust-performance/SKILL.md`. Format changes follow `../how-we-work/SKILL.md`.

## Object model

- A formula is one atomic object in the text flow, inline or display. It has an id, a stored source (LaTeX is the usual choice), a display mode, and an optional human prompt kept beside the source.
- The rendered image or layout is derived. Cache it keyed by source, size, scale factor and style. A change to any key is a cache miss, never an edit to the object.
- The buffer stores the object's id. Look objects up through one table.

## Prompt lifecycle

Use an explicit state enum. Every state has a visible form and a way out.

- `Open`: the user is typing a prompt. The text is literal and visibly distinct.
- `Pending`: a request is in flight. The text stays visible and quiet; typing elsewhere is unaffected.
- `Rendered`: source and layout are valid. The prompt is kept for reopening.
- `Failed`: generation or rendering failed. The prompt stays, marked as failed, and can be retried or edited.

Rules:

- Escape in an open prompt turns the text back into literal characters. Nothing is sent.
- A failure never rewrites or deletes the user's text. Show the failure beside it.
- Closing an edit whose text is unchanged reuses the existing result without a request.
- Transitions are explicit functions. No state is implied by the absence of a field.

## Asynchronous generation and results

- Model requests and rendering run off the UI thread. Typing never waits on either.
- Each request carries the object id and a generation number. A result is applied only if both still match; otherwise it is dropped.
- Deleting or editing an object cancels or ignores its in-flight work.
- Keep at most one request per object in flight. Coalesce rapid edits to the same object.

## Validating model output

Treat model output as untrusted input.

- Strip code fences, surrounding prose and delimiters before parsing.
- Accept output only if it both parses and renders. A parse-only check is not enough.
- Cap length and nesting depth before parsing.
- On any failure, keep the prompt and show the failure. Never insert partial or unvalidated output.

## Offline and slow server

The model server may be absent, slow or busy. Opening, editing, saving and rendering existing formulas must work with no server.

- New prompts queue while the server is unreachable and resume when it is reachable, or move to `Failed` with a clear reason.
- Timeouts are explicit and live in one configuration place.
- Model choice and endpoint are configuration, never literals in code.

## Typesetting

- Match the surrounding text size and colour. Derive both from the text style, not from constants in the renderer.
- Align the formula baseline with the text baseline for inline math. Inline math must not noticeably grow line height.
- Centre display math on its own line with vertical spacing above and below.
- Render crisply at any scale factor: rasterise at device resolution or emit vector output.
- A renderer error shows the source or the prompt in place of the formula, never a blank.

## Caret and selection

- A formula is one unit for movement, deletion and selection. The caret steps over it whole, and one delete removes it whole, as one undo step.
- Reopening a formula's prompt or source is one explicit, documented gesture, consistent wherever the caret meets a formula. Retrying a failed prompt uses the same gesture.
- Document every key that acts on a formula in the editor behaviour doc.

## File format

- Use plain, diff-friendly, interoperable text, such as Markdown with `$...$` inline and `$$` display blocks.
- Escape literal delimiters so the file round-trips.
- Keep prompts and other metadata in a form other tools ignore, such as an HTML comment.
- Save then open must return the same document. Test this for every formula in the corpus.
- Version the format when it changes. Treat a format change as a one-way door.

## Fixture corpus and tests

- Keep a corpus of representative formulas: fractions, integrals, sums, matrices, aligned equations, Greek letters, accents, nested scripts, very long formulas and invalid input.
- Each entry has golden outputs for conversion and for rendering. Update goldens deliberately, with review.
- Benchmark rendering and caching on the same corpus.
- Prompts sent to the model live in one versioned place. When a prompt changes, run the corpus and compare results before merging.

Keep the corpus separate from unit tests so it can run on its own.
