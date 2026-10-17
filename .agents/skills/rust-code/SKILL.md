---
name: rust-code
description: Use when writing or changing Rust functions, types, enums, error handling, traits, iterators, unsafe blocks, doc comments or lint settings. Covers borrowing, invalid-state modelling, Result-based errors, match hygiene, and clippy/rustfmt discipline. Hands off structure and naming to rust-structure.
---

# Rust Code

Write Rust that the compiler can check for you. Prefer types that make wrong states impossible, errors that carry their cause, and code that a reader can follow in one pass. Follow the [Principles](../how-we-work/SKILL.md) section for minimalism and the dependency rule.

## Borrowing and parameters

- Borrow before cloning. Ask whether the callee needs ownership before taking a `String` or `Vec`.
- Take `&str`, `&[T]`, `&Path` in parameters. Take `impl AsRef<str>` or `impl Into<String>` when the function stores or converts the input.
- Return owned data only when the caller needs to own it. Return borrowed slices or iterators otherwise.
- Do not `clone()` a large structure to satisfy the borrow checker. Restructure the borrows, split the struct, or pass an index.

## Invalid states

- Use an enum instead of a `bool` flag or a pair of `Option`s that must agree. `enum Mode { Viewing, Editing(Edit) }` beats `editing: bool, edit: Option<Edit>`.
- Wrap units in newtypes when mixing them is a plausible bug: byte offsets and character offsets, logical and physical pixels, sorted and unsorted indices.
- Do not newtype everything. Skip the wrapper for a value with no confusable twin, for a short-lived local, or when the wrapper only forwards every method.
- Make illegal construction impossible: a constructor that validates, a private field that protects an invariant.

## Matching and control flow

- Match exhaustively. Do not use `_ =>` on an enum you own; a new variant must produce a compile error where it matters.
- `_` is acceptable for foreign enums that are `#[non_exhaustive]` or for a catch-all that is truly the same behaviour.
- Use `let ... else` for early exits and `if let` for a single case. Use `match` when more than one arm matters.
- Prefer `?` to nested matches on `Result`.

## Errors

- Return `Result` anywhere input can be bad: user text, files, configuration, network responses, parsed data.
- Library crates define a typed error enum that implements `std::error::Error` and `Display`. Do not add an error crate without approval.
- Add context at the boundary where the operation is named: "reading document X" beats a bare `io::Error`. Keep the source chain.
- Never panic on user, file or network input. Turn it into an error the caller can show.
- Use `expect("reason")` only for a true invariant the code itself guarantees, and say why in the message. Never use a bare `unwrap()` outside tests.
- Do not turn an error into `None` or a default silently. If a fallback is correct, comment why.

## Combinators and iterators

- Use iterator chains when they say what happens in fewer words. Use a loop when the chain needs a fold with state, early exits and side effects.
- Prefer `Option` and `Result` combinators (`map`, `and_then`, `ok_or_else`, `unwrap_or_else`) when each step is one expression. Stop before the chain needs a comment to read.
- Use `impl Trait` in argument and return position. Reach for `Box<dyn Trait>` only when the set of types is open at runtime or a heterogeneous collection is required.

## Traits and derives

- Derive `Debug` on public types. Derive `Clone`, `PartialEq`, `Eq`, `Hash` when the type is a value.
- Implement `Default` when there is a natural empty value; derive it when the fields' defaults are the right ones.
- Implement `From` for infallible conversions and `TryFrom` for fallible ones. Do not implement `Into` directly.
- Implement `Display` for user-facing text; keep `Debug` for developers.
- Mark a function `#[must_use]` when ignoring its result is a bug (builders, validators, pure computations).

## Unsafe

- Avoid `unsafe`. Reach for a safe API first, even a slower one, and measure before accepting unsafe for speed (see [rust-performance](../rust-performance/SKILL.md)).
- When unavoidable, keep the block minimal, wrap it in a safe function that establishes the invariants, and write a `// SAFETY:` comment stating each invariant.
- Test the safe wrapper, and run the tests under Miri when the toolchain supports it.

## Documentation and comments

- Document public items: what they do, their invariants, the errors they return, and any panic. Write `# Errors` and `# Panics` sections when they apply.
- Add a doc example only when it is real documentation; it is also a test.
- Comments explain why, not what. Delete a comment that restates the code, and remove commented-out code.
- Remove `dbg!`, stray `println!` and `eprintln!` debugging before finishing. Use the project's logging path if one exists.

## Lints

- Keep code clean under `cargo clippy --workspace --all-targets -- -D warnings`.
- Use `#[allow(...)]` only on the narrowest item, with a reason on the same line (`#[allow(clippy::too_many_arguments)] // mirrors the format's fixed field list`). Never allow a lint crate-wide to silence one site.
- Run `cargo fmt --all` before handing over, and check with `--check`.

## Verify

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace --locked` (see [rust-tests](../rust-tests/SKILL.md))
- Read the diff yourself, then pass it to [review](../review/SKILL.md).
