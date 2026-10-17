# Build "Mathstral" — a minimal native math writing app for macOS, in pure Rust

## What it is
A distraction-free writing app for fast math: solving problems, taking notes, or any prose that contains math. There is a single centred column of text on a flat #151515 window, and nothing else. The core feature is that I press ⌘1, type a *rough* description of a formula ("integral from 0 to inf of e^-x^2 dx", "sum k=1..n of 1/k^2", "matrix 2x2 a b c d"), and keep writing. A local LLM turns that rough text into LaTeX in the background, and the app replaces my prompt with the beautifully rendered formula, either inline or as a display block.

## Hard constraints
- Pure Rust, macOS only (Apple Silicon first). No Electron, no webview, no JS engine, no bundled TeX distribution.
- Lightweight: fast cold start, low idle CPU, small binary. Keep dependencies few and justify each one.
- Native macOS menu bar (the system one at the top of the screen) with File → New (⌘N), Open… (⌘O), Save (⌘S), Save As… (⇧⌘S), plus standard Edit (undo/redo/cut/copy/paste/select all) and Quit (⌘Q). Use native file dialogs.
- No visible title bar. The window is one flat #151515 colour edge to edge: transparent/hidden titlebar, full-size content view, title hidden, traffic lights hidden. The window must stay draggable, from the top strip or by ⌘-drag anywhere, and resizable.

## Suggested stack (verify current crate versions and APIs before committing; tell me if you'd deviate)
- Windowing: `winit` with its macOS extensions for the hidden titlebar and full-size content view.
- Menu bar: `muda`. File dialogs: `rfd`.
- Text: `ropey` for the buffer and `cosmic-text`/`swash` for shaping and glyph rasterisation.
- Drawing: `tiny-skia` + `softbuffer` (simple, CPU), or `wgpu`/`vello` if needed for smooth scrolling. Start simple.
- Math rendering (the risky part, so spike it FIRST, before building the editor):
  - Preferred: `mitex` (LaTeX → Typst math) + `typst` layout + render to vector/pixmap. This is fully Rust with high-quality math typesetting.
  - Fallback: `ReX` or another pure-Rust LaTeX math renderer.
  - Write a small benchmark that renders ~20 representative formulas (fractions, integrals, matrices, aligned equations, Greek, sub/superscripts). Report quality, render time, and binary-size cost, then pick one.
- Ollama HTTP: `ureq` (blocking) on a worker thread plus `serde_json`. No tokio unless truly needed. Wake the UI with `EventLoopProxy` when results arrive.

## Look
- Background #151515. Text off-white (~#E8E6E3). Secondary/dimmed text ~#6B6B6B.
- Font: Cormorant Garamond, bundled via `include_bytes!` (OFL licensed), ~22px, generous line height (~1.6). Keep the font family and size in one config constant so I can swap to a sans.
- Single centred column, max width ~680px, comfortable top/bottom padding. The column scrolls; the cursor stays in view with some breathing room.
- Rendered math is drawn in the same off-white, sized to match the text. Display math is centred on its own line with vertical spacing.
- A thin, softly blinking caret. Selection is a subtle lighter grey.
- No chrome, no status bar, no line numbers, no scrollbar (or an auto-hiding hairline at most).

## Editing (terminal/macOS-style movement)
- ← → ↑ ↓ for basic movement. ↑/↓ move by *visual* lines, keeping the goal column.
- ⌥← / ⌥→: previous/next word.
- ⌘← / ⌘→: start/end of the visual line.
- ⌘↑ / ⌘↓: start/end of the document.
- ⌫: delete char. ⌥⌫: delete previous word. ⌘⌫: delete to line start.
- Forward delete (fn⌫ / Del): delete next char. ⌥Del: delete next word.
- Every movement combined with ⇧ extends the selection.
- Undo/redo with sensible grouping (typing runs are grouped).
- Mouse click to place the caret, drag to select, trackpad scrolling with momentum.
- Soft wrap at the column width.

## The math feature
1. **Starting a math prompt.** ⌘1 inserts a math prompt span at the caret. If the caret is on an empty line, the span becomes **display** math; otherwise it is **inline**. ⇧⌘1 forces the other mode. The span shows my typed text dimmed and italic, with a subtle left marker so I know I'm "in math".
2. **Committing.** Enter, or ⌘1 again, closes the span and the caret continues right after it. For inline math, I keep typing on the same line; for display math, a new line follows. Esc cancels and turns the span into plain text.
3. **Async conversion.** On commit, send the prompt to Ollama on a worker thread, never blocking typing. While pending, the prompt stays visible with a gentle pulse/shimmer. Multiple pending requests at once must work.
4. **Replacement.** When LaTeX comes back, validate it by actually rendering it. On success, swap the prompt for the rendered formula, keeping layout shifts minimal. On failure, retry once with the error fed back to the model. If it still fails, mark the span with a muted red underline and keep my prompt; ⌘1 on it retries.
5. **Editing existing math.** A rendered formula behaves as one atom for caret movement. Moving the caret onto it and pressing Enter (or double-clicking) expands it to show its LaTeX source inline for direct editing. Leaving the span re-renders it locally, with no LLM call needed for raw LaTeX edits.

## Ollama
- Endpoint `http://localhost:11434/api/chat`, `stream: false`.
- Model in a config constant, default `gemma3n:e4b` (I'll confirm the exact tag with `ollama list`).
- `temperature: 0`. Set a `keep_alive` so the model stays warm, and send a tiny warm-up request at launch.
- System prompt (tune it, but keep it strict):
  > You convert rough, informal math descriptions into LaTeX math. Output ONLY the LaTeX expression body: no dollar signs, no \[ \], no code fences, no explanations. Use standard amsmath. If the input is already LaTeX, clean it up and return it.
- Add 5–8 few-shot examples covering integrals, sums, fractions, matrices, piecewise functions, aligned multi-line equations, and set notation.
- Post-process defensively: strip code fences, `$`, `$$`, `\[ \]`, and surrounding whitespace or prose.
- If Ollama isn't running, show a single unobtrusive dimmed line at the bottom of the window ("ollama not reachable") that disappears once it connects. Plain writing must keep working.

## File format
- Plain Markdown-compatible text: inline math saved as `$...$`, display math as `$$...$$` on its own lines. The files must stay readable and portable in other tools.
- On open, parse `$`/`$$` back into rendered math atoms. Handle escaped `\$`.
- Pending (unconverted) prompts at save time are saved as `$$?prompt text?$$` (or a similar unambiguous marker) and resume conversion on open.
- Default extension `.md`. Remember the last-opened file and reopen it on launch. Mark the document dirty and confirm before discarding unsaved changes.

## Architecture notes
- Write my own paragraph layout: tokenise each paragraph into words and math boxes, shape the words with cosmic-text/swash, and wrap greedily, with math boxes as unbreakable atoms carrying a width, ascent, and descent. Align them on the text baseline. Display math gets its own block.
- Cache rendered math as pixmaps keyed by (latex, font size, scale factor). Handle Retina scale factor changes.
- Keep modules small: `buffer`, `layout`, `render`, `math` (render + cache), `llm` (worker + prompt), `input` (keymap), `file`, `app`.

## Milestones (stop after each, show me, and wait for feedback)
1. Math-rendering spike + benchmark, then a recommendation.
2. Borderless #151515 window, native menu bar, file open/save, plain text editing with all movement keys.
3. Math atoms in the layout from `$...$` in loaded files (no LLM yet).
4. ⌘1 prompt spans + Ollama conversion + async replacement.
5. Polish: editing existing math, error states, scrolling feel, warm-up, release build size check.

Don't add features I didn't ask for. When a decision is genuinely mine (taste, UX tradeoffs), ask; otherwise pick the simplest option and note it.