---
name: rust-tests
description: Use when writing, changing or reviewing Rust tests, adding a regression test for a bug, testing parsers, serialisers, renderers or editing logic, or deciding where a test belongs (unit, integration, doc, golden). Covers edge-case tables, round trips, deterministic fixtures and the validation commands. Hands off code style to rust-code.
---

# Rust Tests

Test behaviour at the narrowest public boundary, cover the edge inputs that break text editors, and keep the suite fast and deterministic. Follow the [Principles](../how-we-work/SKILL.md) section for minimalism and the no-new-dependency rule.

## Where a test lives

- Unit tests for private logic go in a `#[cfg(test)] mod tests` at the bottom of the same file.
- Integration tests for a library crate's public API go in `tests/`, one file per concept.
- Doc tests only when the example is real documentation a reader should copy. They also run in CI, so keep them short.
- Golden-file tests live next to the crate that owns the output, with fixtures in a named directory.
- UI behaviour is not tested by mocking the windowing layer. Check it by running the app (see [editor-ui](../editor-ui/SKILL.md)).

## What to test

- Test the behaviour a caller depends on, through the public function or method that a caller would use.
- Do not test implementation details: private field layout, the order of internal calls, the exact allocation count.
- Do not test the standard library, dependencies, or trivial getters and constructors.
- Each test protects one distinct failure mode. Do not add a test that repeats another one at a different layer to raise coverage.

## Edge inputs

Use a table of cases for each function that takes text or ranges. Cover, where they apply:

- empty input and a single character
- unicode: multi-byte characters, combining marks, emoji with modifiers, right-to-left text, the boundary between a base and its combining mark
- line endings: `\n`, CRLF, a file that ends without a newline, a file with mixed endings
- very long lines and very large documents (at least one test that proves no quadratic blow-up at a realistic size)
- positions at the start, the end and exactly on each boundary, plus one past the end where the API allows it
- invalid input that must return an error, not panic

Write the table as a slice of `(input, expected)` tuples and loop over it with a message that names the case.

## Round trips

- Every serialise and parse pair gets a round-trip test: save then open must return the same document, byte for byte where the format promises it.
- Include the edge inputs above in the round trip, including mixed line endings and trailing whitespace.
- Test that a parse of malformed input returns an error and does not panic.

## Property-style checks without new crates

- Write small hand-rolled loops over generated inputs: a fixed seed, a simple generator (such as a linear congruential step), and an assertion that holds for every generated case.
- Print the seed and the failing input in the assertion message so a failure can be reproduced.
- Do not add a property-testing crate without approval.

## Determinism

- No network, clock, randomness or environment in tests unless it is seeded and injected.
- Never require a model server, a GPU, a display or an installed external tool in the default test run. Put such tests behind an explicit feature or an `#[ignore]` attribute with a reason, and document the command that runs them.
- Use temporary directories created by the test, and remove them afterwards. Do not depend on files left by another test.
- Tests run in parallel: no shared mutable global state, no fixed file names, no ordering between tests.

## Golden files

- Compare rendered output (layout, exported files, generated markup) against a golden file in the repository.
- Give the update path an explicit command or environment variable, such as `UPDATE_GOLDEN=1 cargo test`, and document it next to the test.
- Review every golden diff by reading it. Do not regenerate goldens to make a failing test pass without knowing why the output changed.

## Regression tests

- Every bug fix comes with a regression test that fails before the fix and passes after it.
- Name the test after the behaviour that was broken, not the issue number.
- Add the test in the same change as the fix.

## Keep tests fast

- Keep the default run quick. Use small inputs unless the test is about scale.
- Use `--release` for a slow suite and say so in the command that runs it.
- Mark a genuinely slow test `#[ignore]` with a reason only when it has a separate command; a slow test that always runs is a cost to justify.

## Commands

```sh
cargo test --workspace --locked
cargo test --workspace --locked --release     # when the suite is slow in debug
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

Run the narrowest command first (`cargo test -p <crate> <name>`), then the workspace. Report the exact commands and any failure.

## Failure integrity

- A test that expresses intended behaviour is the contract. If it fails, first check whether the code is wrong.
- Do not change an expected value, add an `#[ignore]`, or weaken an assertion to make a failure go away. Change the test only with evidence that it is wrong, and say what the evidence was.

## Verify

- Read the test you wrote and check that it would fail if the behaviour broke.
- Pass the diff to [review](../review/SKILL.md) and run the commands above before [ship](../ship/SKILL.md).
