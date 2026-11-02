---
status: approved
issue: 55
spec: spec/2026-11-02-55-follow-default-output.md
coders:   # Rust threads and process handling, and Codex did the #51 capture steps cleanly
  1: codex
  2: codex
  3: codex
reviewers:   # a second read on the thread/lock/Drop ordering, on top of the Opus review
  - codex
---

# Plan: System audio follows the default output device

## Approved decisions

- Following applies only to `SystemAudio::start(None)`. `start(Some(id))` and
  `start_input` are unchanged.
- A watcher thread inside `SystemAudio`:
  - Every 250 ms it checks whether the current `parec` has exited.
  - Every 2 s it reads the default sink through an injected probe (production:
    `pactl get-default-sink`).
  - On a new default, or a dead child, it launches `parec` on
    `monitor_id(default)` into the same sample queue, swaps it in under the
    lock, then kills and reaps the old one outside the lock. It stores the new
    name for one notice.
  - If the relaunch fails, a live old recorder is kept and retried on the next
    2 s tick. With a dead old recorder, the watcher ends.
- `is_running` for a following recorder means the watcher thread is alive.
- `ListenSession::pump` turns `take_moved()` into one
  `Notice("System audio now records <name>")` and resets the echo gate.
- Polling, not `pactl subscribe`. One notice row per switch. PipeWire is never
  restarted. No live microphone in tests.

## Steps

1. `src/audio/capture/system_audio.rs`: split `launch` (lines 69–146).
   - New `fn spawn_recorder(program: &str, device: &str, queue: Arc<Mutex<SampleQueue>>) -> Result<Recorder, SystemAudioError>`
     does the spawn, the stdout take, the reader thread and the 200 ms
     stays-up check, with today's errors.
   - `struct Recorder { child: Child, reader: Option<JoinHandle<()>> }` with a
     `fn stop(&mut self)` that kills, waits and joins.
   - `SystemAudio` holds `recorder: Mutex<Recorder>` instead of
     `child` + `reader`.
   - `launch` = new queue + `spawn_recorder`.
   - Update `Drop`, `is_running` and `stopped_for_test` (line 197) to match.
   → verify: `cargo test -q system_audio` plus the full check below. All existing
   tests pass unchanged, including the `/proc/<pid>` reap test (~line 311).
   Traps: never hold the `recorder` lock while spawning, or across the 200 ms
   wait. The existing ETXTBSY retry in the test stays.

2. `system_audio.rs`: the watcher.
   - Fields: `follow: Option<Follow>`, where
     `struct Follow { stop: Arc<AtomicBool>, thread: Option<JoinHandle<()>> }`,
     and `moved: Arc<Mutex<Option<String>>>`. The recorder becomes
     `Arc<Mutex<Recorder>>`, so the thread can swap it.
   - `fn start_following(program: &str, probe: impl Fn() -> Option<String> + Send + 'static) -> Result<Self, SystemAudioError>`
     launches on the current default monitor. Pick it with today's
     `monitor_sources` + `get-default-sink` logic (lines 48–57). Production
     passes a probe that runs `pactl get-default-sink` and takes the first
     trimmed line.
   - Then it spawns the watcher loop as decided. Sleep in 250 ms steps,
     checking `stop` between them. Keep `current_sink` in the thread.
   - `start(None)` calls `start_following("parec", pactl_probe)`.
   - `is_running`: if `follow` is `Some`, return `!thread.is_finished()`.
     Otherwise use today's child check.
   - `pub fn take_moved(&self) -> Option<String>` takes `moved`.
   - `Drop`: set stop, join the watcher, then stop the recorder.
   → verify: new unit tests (see Tests 1–5), using a fake `parec` script
   written like the existing one, and a probe closure reading an
   `Arc<Mutex<Option<String>>>` that the test changes.
   Traps:
   - The fake parec must ignore its args. Record the `--device` argument by
     having the script write `"$@"` to a temp file per run, so the test can see
     which device it was started on.
   - Poll with deadlines (≤ 5 s); no fixed sleeps in assertions.
   - Tests run in parallel: unique temp names per test (pid plus test name).

3. `src/speech/session/listen.rs`: in `pump` (line 359), before
   `drop_failed_system` (line 369), drain `self.sys.as_ref().and_then(SystemAudio::take_moved)`.
   When it is `Some(name)`, push `SessionEvent::Notice(format!("System audio now records {name}"))`
   and reset the echo gate (`if let Some(gate) = &mut self.echo_gate { gate.reset(); }`).
   - Add a `#[cfg(test)] pub(crate) fn moved_for_test(name)` constructor in
     `system_audio.rs`: stopped, but with `moved` preset and `is_running` true,
     i.e. `follow: None` and a recorder from `true`'s child reaped. Or reuse
     `stopped_for_test` and set `moved`. Whichever stays smaller.
   - Test in `listen.rs`'s test module (line 567): one pump yields exactly one
     such Notice, and a second pump yields none.
   → verify: the full check.
   Traps: the pump test must not open a microphone. Build the session the way
   the existing `drop_failed_system` test does (line ~590).

4. The lead runs the full check and the Codex review
   (`codex review --base main`, read-only), then the Opus review given only this
   plan and the diff. Fix the findings, record deviations here, and open the
   PR linking intent, spec and plan.

## Tests

- Full check:
  `TMPDIR=/tmp devenv shell -- bash -c 'cargo fmt --check; cargo clippy --workspace --all-targets -q -- -D warnings; cargo test --workspace -q'`
- New unit tests in `system_audio.rs`:
  1. probe A→B: restart on `B.monitor`, `take_moved()==Some("B")` once, and
     `is_running` true throughout;
  2. child exits, probe C: relaunch on `C.monitor`;
  3. child exits, relaunch fails (program path deleted): the watcher ends and
     `is_running` becomes false;
  4. `start(Some)`/`launch` has no watcher (`follow.is_none()`);
  5. after drop, no `/proc/<pid>` for the last child.
- New `listen.rs` test: one moved → one Notice.
- The headless shell suite needs no change, since there's no QML change.
- After deploy, the owner's live check: switch output during a real call.

## Rollback

Revert the merge commit and bump the nixos lock back. There's no schema or
settings change.

## Deviations

- Step 2 (Codex): when the default sink is known, it is used directly
  (`monitor_id(sink)`), even if `pactl list sources` fails or doesn't list
  it, so the injected probe works without an audio server. A default sink
  without a monitor now fails the start instead of falling back to the first
  monitor in the list.
- Step 2 (lead): the new tests retry "Text file busy" (ETXTBSY) like the
  existing reap test. In the full suite, `ai::chat::cli::tests::cancel_kills_the_whole_group`
  then failed once: `run` returned before cancel, because its fake script hit
  ETXTBSY, which the extra script-writing tests made likelier. That test now
  retries ETXTBSY up to 5 times (`src/ai/chat/cli.rs`).
