---
status: draft
issue: 55
intent: intent/2026-11-02-55-follow-default-output.md
---

# Spec: System audio follows the default output device

## Design

All the following happens inside `SystemAudio`
(`src/audio/capture/system_audio.rs`), so the session pump stays
non-blocking and its callers barely change.

1. **One sample queue, swappable recorder.** `launch` is split. A new
   `spawn_recorder(program, device, queue)` starts `parec` and its reader
   thread, pushing into a queue it is given, and returns the child and reader.
   `launch` keeps its current behaviour (the 200 ms "stays up" check,
   the same errors) on top of it. `child` plus `reader` move into one
   `Mutex<Recorder>` so they can be replaced together.

2. **Following only when no device is chosen.** `start(None)` starts as today on
   the default sink's monitor, then spawns a watcher thread. `start(Some(id))`
   and `start_input` have no watcher; their behaviour is unchanged (approved
   intent).

3. **The watcher** (one thread, ended through a stop flag plus join in `Drop`):
   - Every 250 ms it checks whether the current `parec` has exited.
   - Every 2 s it asks for the default sink through a probe function. In
     production that is `pactl get-default-sink`; tests inject a closure.
     Approved intent decision 1: poll, not `pactl subscribe`.
   - When the default sink differs from the one being recorded, or the current
     `parec` has exited, it launches a new recorder on `monitor_id(default)`
     with the same shared queue. On success, it swaps the new one in under the
     lock, kills and reaps the old one outside the lock, and stores the new
     sink's name in `moved: Mutex<Option<String>>`.
   - On failure, a live old recorder is kept and the watcher retries on the
     next 2 s tick. If the old recorder has died and the relaunch fails, the
     watcher ends.

4. **`is_running`.** For a following recorder it is true while the watcher
   thread is alive. That covers the few hundred ms of a swap, so
   `drop_failed_system` (`src/speech/session/listen.rs:393`) does not go
   mic-only in the middle of a switch. A non-following recorder keeps today's
   check of its own child.

5. **Notice.** `SystemAudio::take_moved() -> Option<String>` hands over the
   stored name once. `ListenSession::pump` (`listen.rs:359`), just before
   `drop_failed_system`, turns it into
   `SessionEvent::Notice("System audio now records <name>")` and resets the
   echo gate, as `set_system_audio` does on a fresh start. That is one row per
   switch (approved intent decision 2). The daemon already shows `Notice` as an
   error-styled row without stopping the recording.

6. **Lost audio.** The audio during a switch (≤ about 250 ms plus the new
   `parec` start-up) is simply missing from the system channel. `feed`
   already pads missing system frames (C6), so timing stays aligned.

## Alternatives rejected

- **`pactl subscribe`.** A second long-lived child and an event parser to save
  a 2 s poll. Rejected by approved intent decision 1.
- **`parec --device @DEFAULT_MONITOR@`.** The #51 probe of it printed 0 bytes
  on p620, and Pulse resolves the name once, when the stream connects, so the
  stream would not follow a later change anyway.
- **Polling and restarting from `ListenSession::pump`.** `pump` runs on the
  daemon's async loop, and `pactl` plus a `parec` start take up to 200 ms
  there. That is the blocking C5 removed.
- **Restarting PipeWire, or moving the stream with `pactl move-source-output`.**
  The first is ruled out by the intent's constraints. The second needs the
  stream index and still breaks when the old sink disappears.

## Risks

- **Races between the watcher and `Drop`.** Drop sets stop, joins the watcher,
  then kills the current child. The watcher never holds the lock while
  launching, so Drop cannot deadlock.
- **A flapping default** (a headset bouncing): at most one switch per 2 s
  tick, each with a notice row. That is acceptable, and visible.
- **A default sink with no monitor** (unusual): the relaunch fails, and
  capture keeps the old recorder or ends as today.
- **Hosts:** p620 only (PipeWire with the Pulse shim). razer and p510 run
  the same code, but nobody records there.

## Verification

- Unit tests in `system_audio.rs`, with a fake `parec` script (the existing
  pattern at ~line 319) and an injected probe:
  1. When the probe changes from A to B, the recorder restarts on
     `B.monitor`, `take_moved()` returns B once, and `is_running` stays true
     throughout.
  2. When the current child exits and the probe returns C, it relaunches on
     `C.monitor`.
  3. When the child exits and the relaunch fails (the probe names a sink
     whose program is missing), the watcher ends and `is_running` becomes false.
  4. `start(Some(id))` starts no watcher; existing behaviour.
  5. After Drop, no child process is left (the existing `/proc/<pid>` check).
- A `listen.rs` unit test checks that `take_moved` becomes exactly one
  `Notice`. Tests that need a running recorder use a `#[cfg(test)]` constructor.
- The full check:
  `TMPDIR=/tmp devenv shell -- bash -c 'cargo fmt --check; cargo clippy --workspace --all-targets -q -- -D warnings; cargo test --workspace -q'`.
- After deploy, the owner checks it live: during a real call, switch the output
  in the Omarchy audio menu, and see the notice row and the other side's
  lines continuing. Agents never use the live microphone.
