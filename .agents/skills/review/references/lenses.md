# Review Lenses

## How every reviewer works

- Read the guideline files you were given before reviewing (`AGENTS.md`, the `rust-*` and `editor-*` skills, `how-we-work`), and neighbouring files when your lens needs them.
- Review only the given diff scope, plus the code it touches.
- Stay in your lens. Do not report what other lenses cover.
- Read-only: do not edit files, run git, or change state.
- Falsify first: try to disprove each finding by tracing callers, searching for re-exports and external references, and checking for more specific overrides. Drop it if it fails.
- Return findings in the format at the end of this file. No praise, no restating the diff.

## Overengineering

Find machinery the current requirement does not justify, and name the simpler direct form. Apply the Principles section of `how-we-work`.

Look for:
- Options, parameters, feature flags, config keys or extension points that no caller uses today.
- Wrapper, adapter, builder or newtype types that only delegate to one call.
- Structs with one method where a plain function does the job.
- Traits with one implementation and no planned second, or trait objects where a concrete type works.
- Generics, lifetimes or macros that nobody needs at this call site.
- Abstractions for hypothetical providers, scale or future reuse; state machines for two states.
- Defensive checks for cases the types or callers already prevent.
- Compatibility shims, aliases or deprecation paths for callers that do not exist here.
- New crates, crate features or build scripts (`Cargo.toml`, `Cargo.lock`) that a small own implementation or std could replace.

Not a finding:
- Complexity the requirement demands: concurrency, security, file-format compatibility, platform integration.
- Boundaries and crate layers the repo already established and uses consistently.
- An abstraction with a real second caller in the repo today.
- Preference for a different but equally simple style. Report only when the simpler form is clearly less machinery.

Report: name the machinery, state why the current requirement does not need it, and suggest the simpler direct form. Search for callers and requirements first; drop the finding if a real one exists.

## Over-standardisation

Find sharing and uniformity that couple things which should be free to diverge, and the rare copy that has real drift. This lens is meant to sometimes disagree with overengineering. Report both sides of a tension honestly instead of hedging.

Look for:
- Shared functions, modules, traits or utility types extracted from two or fewer real callers (rule of three).
- Shared code that gains flags, `Option` parameters or branches to serve its callers.
- Look-alike code from different domains, such as editor layout and file export, changing for different reasons or likely to diverge.
- A cross-cutting change forced through one abstraction when each caller needs a different outcome.
- Renames or restructures done only to make things look uniform.
- A `common`, `shared` or `util` module that many unrelated places depend on.
- Under-sharing: a copy that now exists three or more times and has started to drift in behavior.

Not a finding:
- Duplication that costs less than the coupling a shared helper would add.
- Sharing that the language, the crate API or repo guidelines require.
- Copies identical by design and expected to change together, when the diff makes that explicit.
- Identical copies with no drift and no third caller.
- Taste about whether a helper should exist, without a concrete divergence or coupling cost.

Report: for over-sharing, name the shared unit, list its callers, and state the concrete divergence or forced flag. For under-sharing, list every copy with its path and the specific drift. Falsify by checking that the callers really diverge or the copies really drift.

## Conventions

Check that the diff follows the rules the repo has written down and the patterns its neighbouring code already uses.

Look for:
- Violations of an explicit rule in `AGENTS.md`, the `rust-*` or `editor-*` skills, or repo docs.
- Formatting and lint: `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` must pass; a diff that bypasses them (`#[allow(...)]` added to silence a lint) is a finding.
- Rust API guidelines: naming, conversions named `from_`/`as_`/`to_`/`into_` with the matching ownership, `From`/`TryFrom` for conversions, `Debug` on public types.
- Error handling and return shapes that differ from neighbouring code doing the same job.
- Module layout and naming that differs from sibling modules without a reason.
- A second way to do something the repo already does one way, such as a second error type, a second channel between threads, or a second way to write files.
- Tests that contradict the repo's testing rules: location, style or forbidden patterns.
- User-visible behavior changed without `docs/EDITOR.md` updated in the same diff.
- New dependencies or a `Cargo.lock` change that the Principles section of `how-we-work` does not allow.

Not a finding:
- Personal taste with no written rule and no neighbouring pattern.
- Patterns only in legacy code the repo is moving away from.
- Style the formatter enforces, unless the diff bypasses it.
- A rule the diff does not touch that was already broken.
- Choices the guidelines explicitly leave open.

Report: quote the rule text, or cite the neighbouring file path that sets the convention. Name the convention, show the diff location that breaks it, and state the conforming form. Falsify by checking that the rule still applies to this file and that no more specific document overrides it.

## Cleanliness

Find hygiene problems that the diff adds or leaves behind, and nothing it did not touch.

Look for:
- Dead code added or left behind: unused functions, parameters, variables, imports, constants, enum variants and fields.
- `#[allow(dead_code)]`, `#[allow(unused)]` or similar added to hide dead code.
- Leftovers from a superseded implementation: old paths, aliases, always-on feature flags, commented-out code.
- Debug output added: `dbg!`, `println!`, `eprintln!` used for debugging, temporary logging.
- TODO, FIXME, XXX or stray notes without an owner or ticket.
- Unclear or misleading names, and comments that restate the code or no longer match it.
- A missing comment only where a non-obvious why exists and the code would mislead without it.
- Duplicated logic inside the change itself, such as the same block twice in one file.
- Functions or nesting the change made overly long or deep.
- Files added that nothing in the repo references.
- Unrelated edits mixed in: reformatting, or renaming code the feature did not need to touch.

Not a finding:
- Pre-existing mess in lines the diff did not touch.
- Formatting the formatter handles.
- A comment explaining a real non-obvious constraint, even if long.
- Code unused in this diff but kept as an intentional public API of a library crate.
- Naming consistent with neighbouring code, even if you would name it differently.

Report: the file, line range and specific artifact. For unused code, show no reference exists by searching the repo, and say what you searched. Falsify by searching for re-exports, macros and other dynamic references.

## Correctness

Find code in the diff that produces wrong results, unsafe behavior or lost data. Prove each one with a concrete scenario.

Look for:
- Panics reachable from user input or file contents: indexing, slicing on non-char boundaries, `unwrap`/`expect` on I/O, parsing or user-driven paths.
- Byte, character and grapheme offset mix-ups, especially where a cursor, a selection or a formula boundary is computed.
- Integer overflow or underflow on positions, lengths and sizes, including `as` casts that truncate.
- Data loss on save: non-atomic writes, a write that replaces the file before the new content is complete, or a lossy round-trip through the file format.
- Blocking work on the UI thread: file reads of unbounded size, model requests, rendering loops on a key event.
- Stale worker results applied to the wrong state: a result for a formula that was edited, deleted or re-requested.
- `unsafe` without a sound `// SAFETY:` argument, or with one that does not hold.
- Logic errors: wrong conditions, inverted checks, off-by-one, wrong precedence in boolean expressions.
- Error paths that swallow failures, or report success after a failed step.
- Lock poisoning or a held lock across a long call.
- Tests that would still pass if the changed code were broken, or that skip the new branch.

Not a finding:
- Hypothetical failures with no plausible input or state that reaches them.
- Style, naming, structure or formatting.
- Performance on code that runs once, on tiny bounded data, or outside the changed paths.
- Checks that the language, types or a documented caller contract already guarantee.
- Races already serialised by a single worker thread or a channel with one consumer.

Report: a concrete failing scenario (exact inputs or state, and the wrong output or effect), the faulty file and line range, and for test findings the test that would still pass. Falsify by tracing the scenario through the real path, including callers and validation.

## Structure

Ask the structural questions that reviewers repeatedly block on, and report only where the diff fails one. This is not a style review; formatting belongs to the formatter.

Scope:
- Run all 12 questions when the repo's guidelines (`AGENTS.md`, `rust-*`, `editor-*`) state the relevant mechanisms.
- Without stated mechanisms, limit to questions 1, 2, 8, 9 and 12.
- Leave abstraction size, hygiene, conventions and bugs to their lenses.

Look for:
1. Is every new branch, option, flag, error variant, public item or log line used now? If not, what is it for?
2. Does the diff add a second way to do something the codebase already does (a second error type, a second way to hold a formula's state, a second channel for worker results)?
3. Is the logic in the right crate? Pure logic belongs in library crates or in platform-free modules of the app crate; platform code (windowing, menus, dialogs, pixel buffers) does not leak into the library crates.
4. Does each piece of state have one owner? The document owns text and formula state, the worker owns in-flight requests, and the UI reads from them.
5. Are mutually exclusive states enums, not several `bool` flags or `Option` tuples?
6. Are derived values (offsets, layout, prompt states) computed by their owner, not recomputed in the render path or by callers?
7. Does work that can take long stay off the UI thread, and does the worker hand results back as messages that the app applies?
8. Is each public item in a library crate needed by a caller outside its module? Does a public type expose an internal dependency's type?
9. Would an existing module, function or type do the job instead of a new one?
10. Does any change to the saved document format or the settings files keep old files readable and the save-and-reopen round trip exact, and is it documented?
11. Is each deleted function, module, fallback or compatibility path justified as safe to remove, and is the old path removed in the same change? Does the diff bring unrelated `Cargo.lock` churn?
12. Do tests cover each new branch at the narrowest public boundary, without tests that restate standard-library or dependency behavior?

Not a finding:
- Formatting, naming taste with no collision, comment wording.
- Thin tests on an early slice of a larger feature, when the diff states what follows.
- Known debt the diff names and parks with a stated deferral.
- Small duplication that avoids coupling, or complexity the requirement demands.

Severity: `major` for what the project's reviewers would mark as blocking. Never `blocker`, which is reserved for wrong behavior, security holes or data loss. `minor` for what should be fixed in this change. `nit` for what can be deferred.

Report: confirm the existing mechanism exists and fits, and that no real caller or requirement justifies the machinery.

## Finding format

Every reviewer returns a list of findings in this format and nothing else.

Fields, in this order:
- `lens`: the short lens name, for example `correctness`.
- `severity`: `blocker`, `major`, `minor` or `nit`.
- `location`: `path:line`.
- `claim`: one sentence stating the problem.
- `evidence`: the specific code or fact that shows it, quoted or referenced.
- `falsification`: what you checked that could have disproven the claim, and why it still holds.
- `suggestion`: the smallest fix, in one or two sentences.

Rules:
- Report only findings you can point to in the diff or in the code it touches.
- Do not raise style opinions that the repo guidelines do not support.
- No praise. Do not restate the diff.
- If there are no findings, reply `No findings` and list what you checked.
- Cap at 15 findings, most severe first.

Severity guide:
- `blocker`: wrong behavior, security hole or data loss.
- `major`: clear guideline violation, or a structural problem that will cost soon.
- `minor`: worth fixing in this change.
- `nit`: optional.

Example:

```
- lens: correctness
  severity: blocker
  location: src/document.rs:412
  claim: Slicing the line at the cursor's byte offset panics when the cursor sits inside a multi-byte character.
  evidence: `let head = &line[..cursor]` runs on the offset from `move_right`, which steps by one byte.
  falsification: `move_right` at line 398 steps by `len_utf8`, but the Backspace path at line 430 does not; a document with "é" and Backspace reaches the slice with cursor 1.
  suggestion: Step the cursor by a character in the Backspace path, or use `line.get(..cursor)` and handle `None`.
```
