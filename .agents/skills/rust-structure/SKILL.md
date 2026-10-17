---
name: rust-structure
description: Use when adding or moving Rust crates, modules, types, functions or files, changing visibility or public API, naming items, or touching Cargo manifests and workspace metadata. Covers crate boundaries, module layout, visibility and naming. Hands off the minimalism and dependency rules to how-we-work.
---

# Rust Structure

Shape the workspace so pure logic is testable on its own, the platform edge stays thin, and each name says what a thing is. Follow the [Principles](../how-we-work/SKILL.md) section for minimalism, the no-new-dependency rule and future-proofing.

## Crate boundaries

- Keep logic that needs no windowing, file system, clock or UI in a library crate. Its tests must run without a display.
- Keep the app crate thin at the platform edge: it wires the event loop, menus, dialogs and the window to the library crates, and holds little logic of its own.
- Let dependencies point one way: app depends on libraries, libraries never depend on the app. Check for cycles before adding a path dependency.
- Split a crate only for a real boundary: a unit that is reused, a unit with a distinct dependency set, or a unit whose compile time is a problem. Do not split a module into a crate because it is large.
- Merge a crate back when its only consumer is its parent and the seam has no meaning.

## Modules

- Group modules by concept (documents, rendering, input, export), not by kind (types, utils, helpers, traits).
- Keep each file about one concept. Split a file when it holds two concepts or when reading it needs scrolling through unrelated code; treat a file over roughly 500 lines as a prompt to look, not a rule.
- Avoid `utils` and `common` modules. Name the concept the function serves.
- Re-export deliberately from `lib.rs`. A re-export is part of the public API.

## Visibility

- Start private. Widen to `pub(super)` or `pub(crate)` only when a caller needs it.
- Use `pub` only for items that form the crate's API for other crates or the public docs.
- Keep struct fields private unless they are plain data with no invariant. Expose a constructor and accessors when an invariant exists.
- Do not make a type `pub` just so a test can reach it; test through the public boundary or put the test in the module.

## Naming (Rust API guidelines)

- Types and traits `UpperCamelCase`; functions, methods, modules and fields `snake_case`; constants and statics `SCREAMING_SNAKE_CASE`.
- Conversions follow cost: `as_` borrows and is cheap, `to_` allocates or computes, `into_` consumes.
- Iterators: `iter` (borrowing), `iter_mut`, `into_iter` (consuming).
- No `get_` prefix on getters: `len()`, not `get_len()`. Use `get` only for fallible keyed lookup.
- Constructors: `new` for the main one, `with_*` for variants that take options, `from_*` for conversions from a specific input.
- Boolean queries read as questions: `is_empty`, `has_selection`.
- Name by what the thing is, not how it is used today. Avoid abbreviations unless they are standard in the domain.

## Cargo and workspace

- Put shared metadata (`version`, `edition`, `rust-version`, `license`, `repository`, `authors`) in `[workspace.package]` and inherit it with `.workspace = true`.
- Pin the edition and an MSRV (`rust-version`). Raising the MSRV is a deliberate change, not a side effect.
- Declare shared dependency versions once in `[workspace.dependencies]` when more than one member uses them.
- Add a feature flag only when a real user needs to turn something off. Keep default features minimal and document each feature.
- Put binaries in `src/bin/`, benches in `benches/`, examples in `examples/`. Do not hide a runnable tool inside a library's tests.
- Keep the release profile deliberate and commented when a setting trades speed for size or build time.

## Constants and configuration

- Keep constants and tunable configuration in one module per crate, not scattered through call sites.
- Give a constant a name that states its unit (`DEBOUNCE_MS`, `MAX_LINE_BYTES`).
- Do not read configuration from the environment deep inside logic; pass it in.

## Verify

- `cargo build --workspace --locked`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo fmt --all --check`
- Check the dependency graph after a manifest change (`cargo tree -e normal -i <crate>`) and confirm no cycle or new platform dependency reached a library crate.
- Read the [review](../review/SKILL.md) checklist before handing the change over.
