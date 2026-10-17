---
name: rust-performance
description: Use when changing code on an interactive hot path (per keystroke, per frame, per scroll), caching or incremental recomputation, allocation-heavy loops, startup or idle behaviour, binary size, compile time, or release profile settings, or when someone asks to make something faster. Covers measuring first, invalidation by edit range, and budgets.
---

# Rust Performance

Make the editor feel instant by doing less work on each interaction, not by tuning code that runs rarely. Correctness and simplicity come first; measure before and after every optimisation. Follow the [Principles](../how-we-work/SKILL.md) section for minimalism and the dependency rule.

## Measure first

- Write down the number you are trying to move before changing code: keystroke latency, frame time, cold start, idle CPU, binary size or build time.
- Benchmark with `--release` only. A debug build's timings say nothing about the product.
- Use a bench binary or `cargo bench` with representative input: a long document, long lines, many formulas, unicode-heavy text. A toy input hides the problem.
- Report numbers before and after, with the input and the machine. A change without a number is a guess.
- Profile before optimising (a sampling profiler such as `cargo flamegraph` if the project has it, or timed sections). Optimise the hot spot the profile shows.

## Hot paths

- Treat per-keystroke, per-frame and per-scroll code as hot. In those paths, do no work proportional to the whole document.
- Never recompute the whole document, re-layout every block or re-render every visible item on a single edit. Find the affected range and update only that.
- Keep the work for a scroll proportional to what becomes visible, not to what exists.
- Move anything that can take longer than a frame off the interaction path (see [rust-concurrency](../rust-concurrency/SKILL.md)).

## Incremental invalidation

- Track dirty regions by edit range. An edit marks the span it touched and the spans whose derived data depends on it, nothing more.
- Store derived results with the range they came from, so a stale entry is easy to find and drop.
- Verify incremental results against a full recompute in a test on small inputs. Incremental code drifts silently otherwise.

## Caches

- Key a cache by its full input: the source text, the settings that affect output, and the version of the renderer. A key that omits an input returns wrong results.
- Bound every cache and evict. Decide the policy (least recently used, or a size cap) and write it down in the type.
- Do not cache what is cheap to recompute; a cache has a memory and a correctness cost.

## Allocations

- Reuse buffers across iterations: `clear()` and refill a `Vec` instead of allocating a new one per frame.
- Use `Vec::with_capacity` when the size is known or well estimated.
- Borrow with `&str` and slices instead of allocating substrings.
- Use `Cow<'_, str>` when a result is usually borrowed and sometimes owned.
- Avoid `format!` and `to_string()` in loops; write into a reused `String` with `write!`.
- Do not `clone()` large structures to get around a borrow. Restructure the code or store an index.

## Idle and startup

- Idle CPU should be about zero. The app is event-driven: no timer that wakes on every tick when nothing is pending, no polling loop.
- Check idle CPU in the running app after any change to the event loop, timers or workers.
- Cold start: do the minimum before the first window shows. Load heavy resources lazily, and load them on a worker thread, not on the UI thread.
- Keep a first-frame placeholder cheap; the real content can arrive a moment later.

## Budgets: size, compile time, profile

- Treat binary size and compile time as budgets. Before adding a dependency or feature, measure its effect on both and on the release binary.
- Keep the release profile deliberate. Each setting (optimisation level, LTO, codegen units, stripping, panic strategy) is a trade. Comment the reason next to any change.
- Do not change the profile to win a benchmark without checking its effect on size and build time.
- Changing a dependency needs approval (see [how-we-work](../how-we-work/SKILL.md)).

## Algorithms before micro-tuning

- Prefer an algorithmic win (a better data structure, a range query instead of a scan, a sorted index) over inlining tricks or bit hacks.
- Micro-optimisations are acceptable only after a profile shows them as the cost, and they must keep the code readable.

## Unsafe for speed

- Do not use `unsafe` for speed without a benchmark that proves the gain is needed and the safe version is too slow.
- Keep such code behind a safe API with a `// SAFETY:` comment (see [rust-code](../rust-code/SKILL.md)).

## Verify

- Run the benchmark or profile in `--release` and compare with the baseline.
- `cargo test --workspace --locked --release` if the tests are slow in debug.
- Confirm idle CPU and cold start in the running app (see [editor-ui](../editor-ui/SKILL.md)).
- Pass the change to [review](../review/SKILL.md), and to [ship](../ship/SKILL.md) only with the numbers attached.
