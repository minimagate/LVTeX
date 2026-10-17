---
name: rust-concurrency
description: Use when adding or changing background work, worker threads, channels, shared state, timers, cancellation, network or file I/O off the UI thread, or the event loop's wake-up path. Covers thread ownership, message passing, stale-result rejection, backpressure and panic isolation. Hands off general rules to how-we-work.
---

# Rust Concurrency

Keep the interface responsive by moving slow work to workers that own their data, talk through messages, and can be cancelled or ignored when their results go stale. Follow the [Principles](../how-we-work/SKILL.md) section for minimalism and the dependency rule.

## Keep the UI thread free

- The event thread never does I/O, network calls, file reads, heavy layout, rendering or parsing of large input.
- Each event handler does a bounded amount of work: update state, send a job, return.
- If an operation can take longer than a frame, it runs off the UI thread and reports back with a message.
- Measure the event thread's time per event during development; a long handler is a bug to fix, not to accept.

## Threads and channels

- Use plain `std::thread` workers and `std::sync::mpsc` channels by default.
- Do not add an async runtime unless the project has decided on one. If a change seems to need async, stop and raise it first.
- One worker type per kind of job. Give each worker the channels it needs and nothing else.
- Send owned data across threads (`String`, `Vec`, `Arc` of an immutable value). Do not send references.

## Ownership of state

- Give each piece of state exactly one owner. The owner is a thread or a struct on one thread.
- Prefer message passing over shared state: send a job with its input, receive a result with its output.
- Use `Arc<T>` for shared immutable data (a loaded resource, a configuration). Use `Arc<Mutex<T>>` only when the data is genuinely mutable and shared, and keep the lock scope small.
- Avoid `Arc<Mutex<_>>` sprawl: if a third struct needs the same mutex, redesign the ownership.
- Never hold a lock across I/O, a channel send, a blocking receive or a call into callbacks.

## Stale results and cancellation

- Tag each job with an id or a generation number. When a result arrives, compare it with the current generation and drop it if it is old.
- Coalesce by latest wins: when several requests for the same target queue up, process only the newest and drop the rest.
- Make cancellation cheap and cooperative: the worker checks a flag or a generation between steps, and the owner drops the handle to abandon work it no longer needs.
- Do not let a cancelled job's result change state. Check the id at the single point where results are applied.

## Waking the event loop

- Wake the event loop through its proxy or an equivalent user-event send when a worker has a result. Do not poll a channel from a timer on every frame.
- Do not busy-wait or spin on `try_recv` in a loop. Use a blocking receive on a worker thread, or a wake-up event on the UI thread.
- A timer exists only when something needs to happen later. When nothing is pending, the app should use no CPU at all.

## Backpressure and bounds

- Use bounded channels (`sync_channel`) where a producer can outrun a consumer, such as a stream of user input or log lines. Decide and document what happens when the bound is hit: block, drop the oldest, or drop the newest.
- Prefer coalescing over an unbounded queue of identical requests.
- Set a timeout on every network call. Make the timeout a named constant.

## Panic isolation

- Wrap each job's work in `std::panic::catch_unwind` at the job boundary, or run the worker so a panic restarts it. A bad input must never kill the app.
- Convert a caught panic into an error result for that job, and log it through the project's normal error path.
- Do not rely on `catch_unwind` for control flow. It is a safety net for bugs, not a way to handle expected errors.

## Shutdown

- Shut down by dropping the sender; the worker's receive loop ends and the thread returns.
- Join worker threads on shutdown when they hold resources that must be flushed, and bound the join so exit cannot hang.
- Drop the sender before joining, or the join waits forever.

## Testing

- Put the worker's logic in pure functions: input in, output out, no channels, no threads. Test those directly.
- Keep the thread and channel code a thin shell around the pure logic, and test the shell with one or two simple round-trip tests.
- For ordering and stale-result bugs, write a test that sends two jobs and proves only the newest result is applied.

## Verify

- `cargo test --workspace --locked`
- Run the app and confirm the interface stays responsive during the slow operation (see [editor-ui](../editor-ui/SKILL.md)).
- Confirm idle CPU is near zero after the work finishes (see [rust-performance](../rust-performance/SKILL.md)).
- Pass the diff to [review](../review/SKILL.md).
