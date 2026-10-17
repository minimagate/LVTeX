# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.0-alpha] - 2026-10-17

First public alpha.

### Added

- Native macOS editor for prose with inline and display math, for Apple Silicon.
- Math prompts typed as `/…/`. A local Ollama model converts each prompt to LaTeX,
  which is rendered in the background with MiTeX and Typst. Plain writing never waits
  for generation.
- Prompt states: open, pending, rendered and failed. Failed prompts can be retried, and
  Enter or Backspace reopens rendered formulas for editing.
- Escape to keep a prompt as literal text.
- Markdown-compatible file format with hidden prompt comments, so prompts survive
  save and reopen. Pending prompts resume when a file is opened.
- Imported formulas render directly. Opening one for editing requests a generated description.
- Standard File and Edit menu commands, native open and save dialogs, and atomic saves.
- Window size and position are restored between launches. The last-opened file is reopened.
- Titles with `# ` lines in Libertinus Serif Italic.
- Undo and redo with grouped typing.

### Fixed / hardened

- The test suite compiles and runs again.
- A literal `$` in prose is escaped as `\$` on save, so it is not read back as math.
- Stray U+FFFC characters are dropped from input.
- A panic in the renderer or the math worker fails only the current formula instead of
  crashing the app. Release builds unwind on panic to make this possible.
- LaTeX input nested more than 64 braces deep is rejected.
- Disabling input-method composition no longer blocks typing.
- The `-psn_` argument that Finder passes at launch is ignored.
- Defensive guards in the rendering path.
- The workspace is restructured into `app/`, `crates/render` and `crates/latex`.
- Unused fonts are removed from the bundle.
