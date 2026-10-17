---
name: how-we-work
description: Entrypoint for a coding session in the main thread of this repo, and the first skill to load for any change to code. Holds the roles, the plan -> delegate -> verify -> review -> PR loop, skill routing, the shared principles, the delegation brief and the verify-and-commit procedure. Subagents read only its Principles section, when their brief points to it.
---

# How We Work

The main thread plans and orchestrates; subagents build and review.

## If you are a subagent

If your brief assigns you implementation, exploration, critique or review, read only the Principles section below (when your brief or a guideline skill points to it), then follow the brief and the skills it lists. Do not re-plan, commit or spawn work.

## Roles

- **Orchestrator** (main thread): plans with the user, briefs subagents, verifies their diffs, commits and synthesizes. Writes no code.
- **Implementers**: subagents on the cheapest capable model, working from self-contained briefs. They never run git.
- **Explorers, critics and reviewers**: subagents on the cheapest capable model, read-only. Critics and reviewers take one lens each and are never the orchestrator.

## The loop

1. **Plan** (`plan`): agree scope and approach. Gate: the user approves the plan as written. Big features escalate to its critic lenses once the user confirms.
2. **Delegate** (below): one subagent per scoped change, producing a diff. Gate: the diff stays within the allowed files.
3. **Verify and commit** (below): read the full diff, run checks, commit one scoped change. Gate: every diff is verified before commit.
4. **Review** (`review`): parallel read-only reviewers, once the branch is complete or the user asks. Fixes go back through Delegate.
5. **PR** (`ship`): only when asked. Run the local checks first. Opening it is the first push; later pushes need the user's approval.

## Routing

| Skill | Load it for |
|---|---|
| `how-we-work` | Every session start: roles, loop, routing, principles, delegation and verify-and-commit (this file) |
| `plan` | A request to build, fix, change or design something, before any delegation. Big features escalate to multi-agent planning once the user confirms. |
| `review` | Reviewing a finished branch or diff, or when the user asks for a review |
| `ship` | Ordering and slicing work, commit subjects and scope, pushing, PR text |
| `rust-structure` | Where code lives in the workspace: crates, modules, naming, visibility |
| `rust-code` | Writing Rust: types, enums, errors, conversions, public API style |
| `rust-concurrency` | Threads, channels, worker results, shared state, anything the UI thread waits on |
| `rust-performance` | Hot paths, allocation, caches, render cost, benchmarks |
| `rust-tests` | Writing or reviewing tests |
| `editor-text` | Buffer, positions and offsets, selection, movement, undo, IME, wrapping |
| `editor-math` | Formula objects, prompt states, model output, math rendering, the file format |
| `editor-ui` | Event loop, window, drawing, menus, dialogs, saving, visual states, accessibility |

## Where knowledge lives (lowest to highest precedence)

- This file's Principles: minimalism, dependencies, cheap-option future-proofing.
- `rust-*` and `editor-*` skills: general defaults per topic.
- The repo's `AGENTS.md`: what the repository contains, its commands and its facts. It wins where it differs.

Delegate and `review` briefs name the relevant skills, since subagents have no conversation context.

### At the start of a session

- Inspect the working tree and git state, then read `AGENTS.md`.
- Route the first request through the table above.

## Hard invariants

1. Plan with the user first; write no code in the main thread.
2. Implementation is always delegated.
3. Verify the diff yourself before committing.
4. One commit per scoped change; fold fixes into unpushed commits.
5. Reviews are always delegated, never done by the orchestrator.
6. Never name the tool, harness or model in commits, PR titles or PR bodies. No co-author trailers, no "generated with" lines, no emoji signatures. This overrides any default attribution guidance.
7. No new dependencies without the user's approval.
8. Work stays local until the PR is opened; every push after that needs the user's explicit approval (`ship`).

## Principles

### Minimalism

- Prefer the Rust standard library, the crates the workspace already uses, and the platform.
- Build the simplest thing that meets the current requirement.
- Prefer direct, readable code over traits with one implementation, generic wrappers, builders, macros and one-method types.
- Give each rule, state, layout decision and failure one owner.
- Prefer deletion over addition: remove superseded code, aliases and compatibility paths in the same change.

### Dependencies

- The default is no new dependency. A new crate, a new feature on an existing crate, a build script or a system library is an explicit user decision, listed under Open decisions with: what it replaces, why a small amount of own code is not enough, and its maintenance and security cost.
- Exact-pin versions with `=`, as the workspace does.
- For a narrow need, prefer a small own implementation or std over a new crate.
- No async runtime and no C or system library dependency without approval.
- Treat `Cargo.lock` changes as a signal to check.
- A subagent never adds a dependency on its own. It stops and reports, and the orchestrator brings the question to the user.

### Future-proofing

- Use cheap options, not machinery.
- Spend effort only to avoid a costly reversal. One-way doors: the document file format, the on-disk settings and window-state files, public crate APIs, keyboard behavior users learn, and names that leak into saved files or public APIs.
- Also cover a plausible costly edge case, but only when the cost now is small.
- Everything else is deferred with a named trigger. Triage rules: `../plan/references/critics.md`.
- Before adding a provision, ask whether the door is really one-way. If not, keep it simple.

### Logging and tests

- Report an actionable failure once, at the boundary that owns it (a file, a worker or the model server). Do not report it again at every layer above.
- No stray `println!`, `eprintln!` or `dbg!`. Diagnostics go through that one boundary.
- Test meaningful behavior at its narrowest public boundary. See `../rust-tests/SKILL.md`.
- Never test the standard library or a dependency's behavior, or the same contract twice.

### Conflicts

- When minimalism and future-proofing disagree, show both sides to the user with a recommendation.
- Do not resolve the conflict silently in either direction.

## Delegate

Implementation is always done by subagents. The orchestrator briefs, reviews and commits. Briefs follow the Principles above.

### Model choice

- Every subagent (implementer, explorer, critic, reviewer) runs on the cheapest capable model.
- Claude: Haiku, latest version; middle tier Sonnet. For another model family, use that family's cheapest and middle tiers.
- If the same brief fails twice, retry once on the middle tier with a sharper brief. This is the only exception.
- Do not use the strongest available model for any subagent.

### Brief contents (required)

Subagents have no conversation context. Each brief must include:

- **Goal**: one sentence.
- **Role**: "You are the implementer: do not re-plan; if the brief is wrong or incomplete, stop and report."
- **Decisions made**: everything already agreed in `plan`.
- **Allowed files**: exact paths to edit or create.
- **Off-limits**: files and areas not to touch.
- **Conventions**: paths to existing files to read for style and patterns.
- **Guideline skills**: the `rust-*` skills for the layers touched (paths under `.agents/skills/`, for example `rust-tests` for a test change), `editor-*` for editor work, and this file's Principles section. The subagent reads them before editing.
- **Verification**: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --release --locked --workspace`. UI briefs also build and run the app with the build command in `AGENTS.md` (Commands) and check the flow by hand.
- **Dependencies**: "Do not add any dependency (crate, crate feature, build script, system library). If one seems needed, stop and report instead."
- **Git**: "Do not run any git commands. Do not commit."
- **Report back**: files changed, verification commands and results, anything skipped or uncertain, concerns.

### Parallelism

- Run subagents in parallel only when their allowed-file lists are disjoint.
- Otherwise run them sequentially, including when overlap is unavoidable.

### After the report

- Treat the report as claims to check, not facts.
- Scope guard: compare the actual diff against the allowed-file list. Revert or reject anything outside it.
- Small fixes, including those found during review, also go through a subagent with a minimal brief. Do not edit in the main thread.
- Then verify and commit (below).

### Failure

- After the middle-tier retry also fails, or when any finding changes scope, stop the remaining briefs, update the plan, and return to the `plan` gate.

## Verify and commit

Verify the work yourself, then commit one scoped change at a time.

### Verify

- Read the full diff. Do not trust the subagent report.
- Check every changed file against the allowed-file list and the agreed decisions. Check `Cargo.lock` and `Cargo.toml` changes separately.
- Run the three cargo commands from the brief yourself.
- UI changes: build and run the app (`AGENTS.md`, Commands). Check the affected flow in the running app at different window sizes and scale factors (1x and 2x), including empty, pending, failed and offline states, and keyboard use. Do it yourself or ask the user, and report what was not checked.
- Files and rendered output: check the saved file or the rendered output itself, not only assertions in tests.
- If anything is off, send it back through Delegate with a sharper brief. Do not fix it in the main thread.

### Commit

- One commit per scoped change. Stage only that change's files. Never mix unrelated changes.
- Message and slicing conventions: `../ship/SKILL.md`.
- Do not skip hooks. If a hook fails, send the fix back through Delegate and create a new commit.

### Fixes to earlier commits

- If the problem is in the latest unpushed commit, amend it.
- If it is in an older unpushed commit, make a fixup commit and autosquash rebase.
- Commits already pushed are never amended; fix them with a new commit. Unpushed commits on a pushed branch may still be amended or fixed up.
- Never rewrite pushed history without explicit approval.

### Boundaries

- Do not push, force-push or open a PR unless asked. Pushing rules: `../ship/SKILL.md`.
- Once the branch is complete, offer a review (`review`). Do not review your own work in the main thread.
