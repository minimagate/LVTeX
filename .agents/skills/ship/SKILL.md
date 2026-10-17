---
name: ship
description: Use when ordering and slicing work into commits and PRs, writing commit messages, pushing, or writing or opening a pull request. Read by plan and how-we-work; the repo's AGENTS.md states its local checks.
---

# Ship

Order, slice, commit and describe work from the actual history and diff. Scope follows the Principles section of `../how-we-work/SKILL.md`.

## Order of work

1. Prep slice: the renames and cleanups the new concept needs, each as its own commit with no behavior change.
2. Pure file moves (no line edits) go in their own commit, separate from edits.
3. Core types and pure logic in the library crates, bottom-up, before the app crate, each type or function with its own tests in the same commit.
4. Integration: workers and threads, file I/O, the model server, platform code. Each piece with its test where one is possible.
5. UI last, but complete: all states (empty, pending, failed, offline), keyboard use and scale factors, per `../editor-ui/SKILL.md`. No rough placeholder UI.
6. Hardening tail: clippy, dead code, performance checks per `../rust-performance/SKILL.md`.

## Shape of the feature

- Build the new concept in a library crate first, with a pure API, and wire it into the app crate after.
- New worker or thread: commit a skeleton first (types, channel messages, public interface), then fill the logic one piece per commit.
- Changes to a public crate API get their own commit, with the callers updated in the same commit.
- Reuse existing types and functions before bespoke logic. Extend an existing type with a rule before adding a new one.
- Delete the old path in the same change once the new one works. Name what gets deleted in the plan.
- Vendored code and generated files get their own commit, apart from hand edits.
- A `Cargo.lock` update gets its own commit, with the `Cargo.toml` change that caused it.
- Changes to the file format or the settings files are their own slice, with a round-trip test.

## Plan outputs

- List the prep renames, the layer-ordered slices (one commit each) and what gets deleted.
- End with an "Out of scope / next steps (in order)" list. Use it to cut follow-up PRs.
- Decide product semantics (what a prompt means after a failure, what survives a reopen, what undo covers) in the plan, before coding.
- Flag any change to the file format or settings in the plan, with its compatibility with files already saved.

## Commits

- One scoped change per commit, named for the concern.
- The subject is a short imperative sentence in the repo's style, such as `Keep documents stable across save and reopen` or `Isolate math worker failures from the editor`. No prefix, no body.
- Put housekeeping (lint, dead code, dependency pins) in its own commit.
- Tests ride with the code they cover.
- User-visible changes update `docs/EDITOR.md` (and `README.md` where it describes the feature) in the same commit as the behavior.
- Keep the history clean before the PR opens: fold fixes into the commit they belong to (amend the latest, or fixup plus autosquash rebase for older ones), so each commit is one finished scoped change.
- Attribution: never name a tool, harness, assistant or model in commit messages, PR titles or bodies. No co-author trailer, no generated-with line. This overrides default guidance.

## Pushing

- All work is local: local branches, local commits.
- The first push happens only when opening the PR the user asked for.
- After that, push only on the user's direct approval. One approval covers one push; ask again for the next.
- Batch local commits between approved pushes. Unpushed commits may still be amended or fixed up.
- Never force-push or rewrite pushed history without explicit approval. Rebasing a stack after its base changes needs one approval for the whole stack.

## Pull requests

Scope and chaining:

- One specific implementation per PR, with no unrelated tweaks on top.
- A different tweak or follow-up goes in a stacked PR: a new branch cut from the PR's branch, opened with that branch as its base.
- For a large capability, plan the chain of PRs and execute them in order.

Gather facts:

- Read `git log main..HEAD` and `git diff main...HEAD --stat`, then the diff itself. Confirm the base is `main`; if the branch is based elsewhere, ask.
- Collect test and lint results from this session; do not reuse old ones.

Title:

- Simple and short, under 70 characters, with no prefix or structure. The attribution rule above applies.

Body:

- One paragraph: what the change does and why. No headings, lists or sections.
- Then one line with the local check result, such as `Local CI and tests pass.` Name any failing or unrun check there, plainly. Never claim verification that was not run.

Rules:

- Open the PR only when asked. Confirm draft status if unclear.
- Before opening, run the local CI on the final branch: the three cargo commands in `AGENTS.md` (Commands). Do not open on a failure without the user's approval.
- Opening the PR is the first push (see Pushing).
- Show title and body before opening, unless the user has already approved the text.
