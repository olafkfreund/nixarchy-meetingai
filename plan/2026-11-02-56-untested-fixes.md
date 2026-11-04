---
status: approved
issue: 56
spec: spec/2026-11-02-56-untested-fixes.md
coders:   # small test-only steps; agy runs in parallel with Codex on #55
  1: agy
  2: agy
  3: agy
---

# Plan: Tests for the untested #51 fixes (A7, A9/C8, C5)

## Approved decisions

- One test per fix, and each must fail with its fix reverted. Behaviour
  doesn't change.
- Test hooks are `#[cfg(test)]` only. No real audio, network or live
  microphone. Poll with deadlines; no fixed sleeps in assertions.
- C5 is covered by extracting `off_thread` and testing it. The loop's
  `stop` + `continue` arm is verified by reading, and the PR says so.

## Steps

1. A7, `src/daemon/session.rs`:
   - Add `#[cfg(test)] chunk_panic: AtomicBool` beside `fail_open` (line 343)
     and initialise it beside line 847.
   - In `spawn_chunk` (line 1971), right after `let _chunking = chunking;`
     inside the task, add
     `#[cfg(test)] if me.chunk_panic.load(Ordering::Relaxed) { panic!("test: chunk task panicked"); }`.
   - Test `a_panicking_notes_chunk_frees_the_chunking_flag` in the test module
     (from line 5043): build a daemon the way the neighbouring tests do, set
     `chunk_panic`, call `d.spawn_chunk("m".into())`, and poll `d.chunking`
     until false (2 s deadline). Then clear `chunk_panic`, call `spawn_chunk`
     again, and assert `d.chunking` was true right after the call.
   → verify: `cargo test -q a_panicking_notes_chunk`. Mutation: delete the
   `Drop` impl body, and the test must fail. Restore it.
   Traps: the daemon must be an `Arc<Daemon>`, since `spawn_chunk` takes
   `self: &Arc<Self>`. Use the existing test constructor.

2. A9/C8, `src/speech/stt/transcription.rs` test module (line 1372):
   - Test `a_push_after_the_worker_ended_reports_stream_ended`: build a batch
     transcriber as at ~line 1713, call `live.worker.abort()`, and await the
     handle (by taking it: `std::mem::replace` with a finished
     `tokio::spawn(async {})`, or by adding a `#[cfg(test)]` helper that aborts
     and awaits).
   - Then push `vec![0.0; 800]` up to 64 times until it errors. Assert
     `matches!(err, SttError::Socket(ref m) if m == STREAM_ENDED)`.
   → verify: `cargo test -q a_push_after_the_worker_ended`. Mutation: change
   the `Closed` arm (line 971) to another string, and the test must fail.
   Restore it.
   Traps: the config must not reach the network. Copy the existing test's
   config (it uses a local or unroutable endpoint).

3. C5, `src/daemon/session.rs`:
   - Add a free fn near the recording loop:
     `async fn off_thread<T: Send + 'static, R: Send + 'static>(value: T, f: impl FnOnce(&mut T) -> R + Send + 'static) -> Result<(T, R), String>`,
     which does `spawn_blocking(move || { let mut v = value; let r = f(&mut v); (v, r) }).await.map_err(|e| e.to_string())`.
   - Replace lines 2320–2335's `spawn_blocking` block with
     `match off_thread(s, move |s| s.set_system_audio(want)).await { Ok((s, changed)) => …, Err(error) => … }`,
     keeping both arms' bodies exactly.
   - Test `off_thread_turns_a_panic_into_an_error`: wrapped in a 5 s
     `tokio::time::timeout`, a panicking closure gives `Err(m)` with
     `m.contains("panic")`, and `off_thread(2, |v| *v * 3)` gives `Ok((2, 6))`.
   → verify: the full check. Mutation: make `off_thread` `.unwrap()` the
   join result, and the test must fail (the panic propagates). Restore it.
   Traps: the `Err` arm must still `stop.store(true)` and `continue`. Don't
   simplify it.

4. The lead runs the full check, reviews the diff, does the three mutation
   checks itself, then the Opus review given only this plan and the diff.
   Then the PR, which records the mutation results and the C5 read-only part.

## Tests

- `TMPDIR=/tmp devenv shell -- bash -c 'cargo fmt --check; cargo clippy --workspace --all-targets -q -- -D warnings; cargo test --workspace -q'`
- Three mutation checks as above, each failing its test, then restored.

## Rollback

Revert the merge commit. Tests and test hooks only, apart from the
behaviour-neutral `off_thread` extraction.

## Deviations

- Step 3 (agy): `off_thread` sits after `impl Daemon` (`session.rs` ~4670), not
  next to the recording loop. Only its placement differs.
- Step 4 results (lead): full check passed. Mutation checks: A7 (guard
  body removed), A9/C8 (`Closed` mapped to "closed") and C5 (`off_thread`
  panicking on the JoinError) each made their test fail, and were restored.
  The Opus review found no defects.
