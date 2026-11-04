---
status: approved
issue: 56
intent: intent/2026-11-02-56-untested-fixes.md
---

# Spec: Tests for the untested #51 fixes (A7, A9/C8, C5)

## Design

### A7: the chunking guard resets the flag after a panic

`Daemon::spawn_chunk` (`src/daemon/session.rs:1971`) sets `chunking` and
relies on the `Chunking` drop guard to clear it, even if the task panics.

- Add `#[cfg(test)] chunk_panic: AtomicBool` to `Daemon`, next to `fail_open`
  (line 343, initialised at line 847). At the top of the spawned task, after
  `let _chunking = chunking;`, add
  `#[cfg(test)] if me.chunk_panic.load(Ordering::Relaxed) { panic!("test: chunk task panicked"); }`.
- Test `a_panicking_notes_chunk_frees_the_chunking_flag`: set `chunk_panic`,
  call `spawn_chunk("m")`, and poll for up to 2 s until `chunking` is false.
  Then clear `chunk_panic`, call `spawn_chunk` again, and assert the flag was
  taken (`chunking` true right after the call, or the task ran).
- Without the guard, the flag stays true after the panic and the poll times
  out.

### A9/C8: a closed stream reports `STREAM_ENDED`

The daemon (`session.rs:2354`) treats `SttError::Socket(STREAM_ENDED)` from
`push` as a normal close rather than an error. Both sides reference the same
constant, so the strings cannot drift. What can still break is the producer
no longer returning it.

- Test in `src/speech/stt/transcription.rs`'s test module
  `a_push_after_the_worker_ended_reports_stream_ended`. Build a batch
  transcriber the way the existing tests do (`connect_batch`, ~line 1713),
  `abort()` its worker, and wait for the worker to finish so `pcm_rx` is
  dropped. Then `push` 50 ms of silence until it errors (the 32-slot channel
  may accept a few sends first; bound it at 64). Assert the error is
  `SttError::Socket(m)` with `m == STREAM_ENDED`.

### C5: a failed system-audio toggle doesn't take down the loop

The toggle runs `ListenSession::set_system_audio` in `spawn_blocking`
(`session.rs:2320`). On a `JoinError`, the loop reports it, sets `stop` and
`continue`s, so the closing path still drains the transcriber.

A daemon test can't reach this branch: it needs a live `ListenSession`, which
opens the real microphone, and the intent's constraints forbid that.

- Extract the closure-plus-await into a small free function in `session.rs`:
  `async fn off_thread<T: Send + 'static, R: Send + 'static>(value: T, f: impl FnOnce(&mut T) -> R + Send + 'static) -> Result<(T, R), String>`.
  It maps a `JoinError` to its message. The loop calls it, and the
  `Err` arm (`self.error`, `stop.store(true)`, `continue`) is unchanged.
- Test `off_thread_turns_a_panic_into_an_error`: a closure that panics gives
  `Err` containing "panic", and doesn't hang (wrapped in a 5 s timeout).
  A normal closure gives back the value and the result.
- The `Err` arm's `stop` plus `continue` stays verified by reading only. The
  PR says so.

## Alternatives rejected

- **A fake `ListenSession` behind a trait** to drive the whole loop in a test.
  That means a trait plus a mock for one branch: more code than the fix it
  guards.
- **Testing A9/C8 by comparing the daemon's literal string.** There is no
  literal any more; both sides use the constant.
- **Fixed sleeps.** Ruled out by the intent's constraints. Everything polls
  with a deadline.

## Risks

- **The panicking test task prints a panic message** to the test output. That
  is expected; Tokio catches it.
- **`abort()` timing:** the test waits on the worker handle before pushing, so
  the closed channel is certain.
- **No behaviour change.** `chunk_panic` is test-only, and `off_thread` is a
  pure extraction.

## Verification

- `TMPDIR=/tmp devenv shell -- bash -c 'cargo fmt --check; cargo clippy --workspace --all-targets -q -- -D warnings; cargo test --workspace -q'`
- Mutation check: temporarily remove each fix to see its test fail, then
  restore it. For A7, drop the `Chunking` guard. For A9/C8, make `Closed` map
  to another string. For C5, have `off_thread` `unwrap()` the `JoinError`.
  The PR reports each result.
