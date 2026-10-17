---
name: plan
description: Use when a request to build, fix, change or design something arrives, before any delegation; not for implementation subagents. Small changes: analyze in the main thread and agree the plan with the user before handing off to delegate. Big features (a new document concept, a file-format change, a new crate or dependency, a new worker or thread, a public crate API, or work across several crates): say so, offer multi-agent planning, and run it only after the user confirms.
---

# Plan

Agree the scope and approach with the user before any code is written.

## Phase rules

- The main thread only analyzes and plans. Write no code and make no edits.
- Read the relevant code first: the crate's public API, its callers, the nearest tests, and the repo guidelines (`AGENTS.md`, `.agents/skills/`).
- No implementation, scaffolding or "quick prototypes" to explore.

## Output (exactly four parts)

1. **Understanding**: current state and the goal, in a few sentences.
2. **Open decisions**: each with 2-3 options and your recommendation. Keep it short.
3. **Proposed changes**: an ordered list of scoped changes. Each becomes one commit and lists the files it touches.
4. **Out of scope / deleted**: what is deliberately not done and which old paths are removed.

## Guidelines

- Give a recommendation, not an exhaustive survey of options.
- Do not re-litigate decisions the user already made. Build on them.
- Ask only questions that are genuinely the user's to make. Resolve the rest from the code or sensible defaults, and state each default you chose.
- Split work into independent scoped changes. Parallel slices need disjoint files. Keep each change small enough to review as one commit.
- Order and slice per `../ship/SKILL.md`. The guideline skills `../rust-structure/SKILL.md` and `../editor-text/SKILL.md` (and the others that match the change) say where code belongs.
- Follow `../how-we-work/SKILL.md` (Principles). Any new dependency is an explicit user decision: list it under Open decisions with the reason.

## Escalate big features

- If the request introduces a new document concept, changes the file format, adds a new crate or dependency, adds a worker or thread, changes a public crate API, or spans several crates, say so.
- Offer multi-agent planning (below).
- Start it only after the user confirms, because it costs considerably more.

## Big features (multi-agent)

Read-only explorers and critics run in parallel, then the result is cut down to the simplest plan that meets the requirement. The core principle (future-proofing means cheap options, not machinery) is in `../how-we-work/SKILL.md` (Principles, Future-proofing).

1. **Confirm.** Get the user's agreement, because the run costs tokens. State how many explorers and critics will run and which model tier. Do not start without a yes.

2. **Explore.** Dispatch several read-only explorers in parallel on the cheapest capable model. Each maps one area: current code, callers, crate boundaries, tests, or repo guidelines (`AGENTS.md`, `.agents/skills/`). Explorers return facts with `path:line` and no opinions; opinions are discarded. Merge the results into one fact sheet. Drop duplicates and any claim without a `path:line`.

3. **Base plan.** Draft the simplest plan that satisfies the stated requirement, in dialogue with the user. Write it to `tmp/plans/<slug>.md` in the repo, untracked and never committed.
   - Sections, in order: Goal, Non-goals, Decisions, Slices, Out of scope / deleted, Triage, Deferred ledger. Triage and Deferred ledger stay empty until step 5.
   - Each slice is commit-sized and lists its files. Slices meant to run in parallel must have disjoint files; sequential slices may overlap.
   - Slice order follows `../ship/SKILL.md`: prep slice first, library crates bottom-up, integration, UI last, and an Out-of-scope list.

4. **Critics.** Dispatch four read-only critics in parallel on the cheapest capable model, one per lens: Simplicity, Future-proofing, Edge cases, Fit. Brief each with its lens name and `references/critics.md`, the plan file path, the fact sheet, the repo guideline paths (`AGENTS.md`, and the `rust-*` and `editor-*` skills as paths under `.agents/skills/` that match the change), and a read-only instruction.

5. **Triage.** Label every observation Adopt, Cheap hook, Defer with trigger, or Reject, using the Triage rubric in `references/critics.md`. Record each label in the plan's Triage section.
   - Spot-check doubtful observations against the code before accepting them.
   - Where critics conflict (usually Simplicity against Future-proofing), show both sides to the user with your recommendation. Never resolve it silently.
   - Write each deferred item into the Deferred ledger with its trigger.

6. **Bloat check.** Run one pass only. Send the revised plan to the Simplicity critic in revision mode, with the original requirement, the revised plan and the triage table, to catch plan creep introduced by triage. Apply or reject each cut and record the decision. Any cut of an Adopt or Cheap hook item goes to the user instead of being decided silently. Even if the critic finds more, stop here and surface the rest as a user decision.

7. **Present and gate.** See Presenting and Gate below.

8. **Hand off.** The approved slices become the briefs for the delegate procedure in `../how-we-work/SKILL.md`, one commit per slice. Copy from the plan file; do not retype it.

## Briefs

- Every explorer and critic brief is self-contained. Subagents have no conversation context.
- Each brief names the exact files or directories to read, the requirement in full, and the read-only instruction.
- Paste the same requirement text into every brief. Do not paraphrase it between agents.

## Presenting

- Lead with the decisions that are theirs, each with your recommendation.
- Then the revised plan, the triage table (observation, label, one-line reason), and the Deferred ledger.
- Name the model tiers and the number of agents that ran, so cost is visible.
- Do not dump raw critic output. Present the triaged result.
- Keep the plan short. If it grows past about one page, cut it before presenting it.
- Reject any explorer or critic output that edits files.

## Gate

- Applies to small and big plans. Wait for explicit agreement before moving to delegate.
- Approval covers only the plan as written. If scope, files or decisions change, re-plan and get agreement again.
- Do not treat silence or a partial "ok" as approval of new items.
