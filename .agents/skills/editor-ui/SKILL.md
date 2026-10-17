---
name: editor-ui
description: Build the native window and visual layer of a prose editor: the event-driven loop, logical and physical pixels, platform menus and shortcuts, file dialogs and atomic saves, window state restore, design tokens, quiet status states, accessibility and the manual check list. Use when changing the window, menus, drawing, theming, file commands or persisted window state.
---

# Editor UI

The window is a thin layer over the text core and the math layer. Keep drawing code free of document logic, and keep state in plain structures that the window reads.

Threading lives in `../rust-concurrency/SKILL.md`; module layout in `../rust-structure/SKILL.md`; performance measurement in `../rust-performance/SKILL.md`. Minimalism and dependency rules are in `../how-we-work/SKILL.md`.

## Event loop

- Redraw only on input, a state change, or a ready result. Idle CPU should be near zero.
- No busy loops and no polling timers. Sleep until the next event or the next due deadline, such as a debounced save.
- Draw at most once per frame. Coalesce invalidations that arrive in the same frame.
- Background results wake the loop through the event system, not by polling a shared flag.

## Coordinates and scale

- Store and lay out in logical units. Multiply by the scale factor only at the raster edge.
- On a scale-factor change (the window moves between displays), re-rasterise at the new scale. Do not keep bitmaps sized for the old one.
- Persist window geometry in logical units so it survives a change of display.
- Validate restored geometry against the connected displays. A window that is off-screen is moved back on-screen.

## Platform conventions

- Use the system menu bar with the standard shortcuts and item names. Do not reuse a shortcut the platform already assigns to something else.
- Use native file dialogs for open and save, and the native clipboard.
- Support the standard text-editing keys described in `editor-text`.
- Full screen, minimise and zoom behave as the platform expects. Quit and close have one documented meaning each.
- Keep the window movable and resizable even when the title bar or chrome is hidden. Document the drag region.

## Files and state

- Save atomically: write a temporary file in the same folder, copy permissions, then rename over the target. Never leave a half-written file.
- Resolve symbolic links before saving so the link survives.
- Track unsaved changes. New, open, close and quit ask before discarding them; the prompt is explicit and cancellable.
- Restore the last document and window state on launch. Validate both and fall back to defaults if the file is missing or corrupt.
- Keep settings in the platform's application-support location, not beside the executable.

## Visual design

- A minimal, calm, single-column writing surface. Chrome is hidden or quiet; the text is the interface.
- Put all design tokens (colours, fonts, sizes, spacing, timings) in one configuration module. Drawing code reads tokens by name, with no magic numbers.
- Check text-to-background contrast as a pair, not each colour alone.
- Pending, failed, offline and empty-document states each get a distinct, low-key treatment with a text description.
- Bundle fonts only with their licences tracked in the repository. A new font is a new dependency and needs approval.
- Keep motion minimal, and respect the system's reduced-motion setting where the platform exposes it.

## Accessibility

- Every action is reachable by keyboard alone, including opening a prompt and using the menus.
- Respect the system text-size setting where feasible. State in the editor doc when it is not.
- Name the screen-reader gaps honestly. Do not claim support that was not tested.

## Check every UI change

Build and run the app; a passing build is not enough. Check:

- Several window sizes, including a narrow one, and at least one non-default scale factor.
- An empty document, a long document, and one with pending, failed and offline math.
- Light and dark appearance if the platform supports both.
- Keyboard-only use of the changed flow.

Report what was not checked. Update the editor behaviour doc in the same change.
