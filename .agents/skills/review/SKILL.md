---
name: review
description: Run an adversarial multi-lens code review of a branch, diff or named files. Use when the user asks for a review, or when one is offered after verify and commit (how-we-work) completes a branch. Dispatches parallel read-only reviewer subagents, one per lens, then verifies and synthesizes their findings.
---

# Review

Coordinate read-only reviewer subagents, one per lens, then verify and synthesize their findings. The orchestrator never reviews the code itself.

## Roles

- Reviews are done only by subagents on the cheapest capable model, never the orchestrator.
- Reviewers are read-only: no file edits, no git write commands.
- The orchestrator scopes, dispatches, verifies, filters and synthesizes.
- Keep each reviewer to one lens and a small diff scope. Require evidence and falsification, and spot-check high-severity findings yourself.

## Scope

- Determine the exact diff: the branch against its base (`git diff main...HEAD` for this repo), uncommitted changes, or named files.
- If the base branch is unclear, ask before dispatching reviewers.
- Give every reviewer the same diff scope, as an exact command or file list.

## Lenses

Each lens is defined in `references/lenses.md`: `overengineering`, `over-standardisation`, `conventions`, `cleanliness`, `correctness`, `structure`.

- Small diff (a few files, no logic change): conventions, cleanliness, correctness.
- Normal diff: all six.
- Large diff: all six, one reviewer per lens per crate or area.

Dispatch one reviewer per lens in parallel. State which lenses ran and why.

## Reviewer brief

Reviewers have no conversation context, so each brief is self-contained and includes:
- `references/lenses.md` and the lens name. The reviewer applies the shared rules and its own lens section.
- A read-only instruction (no file edits, no git write commands) and an instruction to falsify each finding before reporting it.
- The diff scope and the exact command that produces it.
- Guideline paths: `AGENTS.md`, the `rust-*` and `editor-*` skills (as paths under `.agents/skills/`) for the areas the diff touches, and `how-we-work/SKILL.md`.
- An instruction to return findings in the format at the end of `references/lenses.md`, and nothing else.

## Verification

- Spot-check every blocker and major finding, and every doubtful finding, against the actual code. Use a quick read yourself or a cheap subagent.
- Drop findings that do not hold up. Report how many were dropped.

## Synthesis

- Merge duplicate findings from several lenses into one, listing all lenses.
- Rank by severity. Group by file or theme.
- Surface conflicts between lenses explicitly. Example: overengineering says inline it, over-standardisation says keep it separate.
- Give a recommendation for each conflict, but leave the decision to the user.

## Output

1. Overview: verdict, counts by severity, top themes, lenses run and why.
2. Findings grouped by severity, each with `path:line`.
3. Conflicts between lenses.
4. Recommended action list.

## After the review

- Do not fix anything during review.
- Once the user picks findings: delegate the fixes and verify and commit them (`how-we-work`). Use a new commit if the branch is pushed. Then re-review only the touched files, with the relevant lenses.
- Never include tool, harness or model names in PR comments or commits.
