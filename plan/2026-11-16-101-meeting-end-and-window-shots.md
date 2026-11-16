---
status: draft
issue: 101
spec: spec/2026-11-16-101-meeting-end-and-window-shots.md
coders:
  1: codex   # small, tricky: file validation and pid-probe rules that must match CallState.qml exactly
  2: codex   # small, tricky: ownership, countdown generations, locked expiry
  3: coder   # sound clock and a new setting through four layers, following an existing pattern
  4: codex   # tricky: eligibility generation and capture races
  5: coder   # QML edits across Service, BarWidget, Overlay, Client
  6: agy     # large, mechanical: rewriting the #64 shell cases and the fake daemon
reviewers:
  - codex   # an independent read of the daemon concurrency
  - agy     # an independent read of the shell and privacy surfaces
---

# Plan: Notes finish when the meeting does, and screenshots show only the meeting

## Approved decisions (self-contained)

**Intent and spec approved 2026-11-16, with all F1–F5 recommendations.**

**Ownership**
- The daemon reads `$XDG_RUNTIME_DIR/meeting-mode.json` itself, as the single
  reader that decides. The shell's `CallState.qml` is for display only.
- A recording (`meeting@since`) gets `for_call` in these cases:
  - **schema 2:**
    - it starts while `call` is `call`;
    - **or**, while it is recording, a call whose `since` has not been handled
      before reaches `call` with `by ≠ fallback` (adoption);
  - **schema 1:** `recording_since ≥ call_since − 5 s`, with no upper bound
    and no adoption.
- **Keep** clears `for_call`, and the recording is never adopted again.

**The countdown**
- **Call end:** a 30 s countdown runs while all of these hold, and is
  re-evaluated on every reading change:
  - recording;
  - `for_call`;
  - `present`;
  - `call` is not `call`.
- **Generations:** each countdown has a fresh `gen`. `record.keep {gen}` and
  expiry act only on the current `gen`, and expiry re-checks under the lock.
- **Inactivity:** a countdown also starts when, with no call, there has been no
  mic sound for N min. Call end wins when both apply.

**Inactivity clock and chips**
- **Sound:** a channel counts as sounding when it stays above `SOUND_LEVEL`
  (0.008) for at least 300 ms within 1 s. This is measured in the daemon's
  audio loop, before any engine.
- **Resets:** recording start or Play, each change of `call`, Keep, and the
  first frame after a gap of 5 s or more.
- **During a call, `maybe`, or not `present`:** after N min with no sound on
  either channel, a chip appears. Under F1, a call also gets a 30 s countdown
  after 3 × N.
- **No frames at all:** no stop.
- **The setting:** `inactivity_stop_minutes`, default 10, with 0 for off.

**Auto screenshots**
- They bind to `callWindow` (schema 2, `present`, `call`, state `bound`).
- They capture with `grim -T <stableId>`, with a 3 s timeout.
- An `elig_gen` change during a capture discards it. A change in
  `callWindowSeq` bumps `elig_gen`.
- Eligibility requires all of these:
  - the #84 checks;
  - the same bound window, with `showing`;
  - no countdown;
  - the window present in `hyprctl clients -j`.
- **A hidden window:** two fingerprints 1 s apart; if they are identical, skip.
- **A closed window:** "Paused: meeting window closed", restartable.
- **Stop → Play:** keeps the binding while the same window exists and the call
  is on.
- **The rectangle:** removed from auto screenshots; the manual screenshot key
  keeps `slurp`.

**Logging and privacy**
- **Logs:** one `tracing::info!` per lifecycle event, with fields `event`,
  `reason` and `origin` (the shell sends `"shell"`; a missing origin logs as
  `cli`). No titles, no meeting ids, no window ids.
- **`state` gains:**
  - `for_call`;
  - `stopping {gen, reason, in_s}`;
  - `inactivity_chip`;
  - `autoshots.paused`;
  - `call_window {state, class}`.
- `call_window.class` is removed from every agent reply, and in Local-only also
  from broadcasts.
- `record.keep` is not on the agent allow-lists.

**The shell**
- It drops its own end logic.
- It shows "Notes stop in N s · Keep" and "No sound for N min · Stop notes" in
  the bar and the meeting view, plus `notify-send` carrying `gen`.
- It shows the app and window state.

**Out of this repository:** meeting-mode's schema 2 (spec section A) is its own
intent → spec → plan in `nixarchy-meeting-mode`. meetingai must work with
schema 1 until then.

## Steps

1. **New `src/daemon/callstate.rs`: the call-state reader.**
   - **The reading:**
     - `pub struct CallReading {present, schema, call, since, by, window,
       window_state, window_class, showing, window_seq}`;
     - `fn parse(bytes, now, probe) -> CallReading`;
     - title fields are ignored and never stored.
   - **Validation, as in `CallState.qml:22-31`:**
     - a new pid counts as gone until `probe(pid)` (a `/proc/<pid>` exists
       check, injectable in tests) says otherwise;
     - no pid and `updated` older than 600 s means stale;
     - `call` other than `call` or `maybe` reads as `none`.
   - **The poll:** `tokio::spawn` a 1 s loop in `src/daemon/mod.rs`, next to
     the calendar loop at `:63`. It checks the file's mtime and size, and on a
     change parses it and calls `daemon.on_call_reading(r)`.
   - **`Inner`** (`session.rs:334`) gains `call: CallReading`.

   → verify: `cargo test callstate`, with fixtures for schema 1, schema 2, a
   dead pid, a new pid, stale, garbage, and a file with titles (the test
   asserts no title in `Debug`).

   Traps:
   - never hold `inner.lock()` across `.await`;
   - the probe must not block the loop (`tokio::fs::metadata`).

2. **`session.rs`: ownership, the countdown, `record.keep` and logging.**
   - **The recording** (the struct `record_start` reserves at `:3222`) gains
     `for_call`, `kept`, and `handled_since: Option<f64>`.
     - **Watch out:** `reset_session(g)` at `:3309` runs after the lock is
       released at `:3259`/`:3273`. The new fields live on the recording
       struct, and are set only after `reset_session`.
   - **`on_call_reading`:** stores the reading, then:
     - applies adoption, guarded by `handled_since`;
     - re-evaluates the countdown condition;
     - bumps `elig_gen` (step 4);
     - logs the transitions.
   - **`stopping: Option<Stopping {gen, reason, at}>` on `Inner`:**
     - `start_countdown` spawns a task that sleeps 30 s, then takes the lock
       and re-checks the `gen`, the recording id and the condition before
       calling the stop path with its reason;
     - `record_stop` (`:3392`) and `record_start` clear it.
   - **New commands in `dispatch` (`:6148` area):**
     - `record.keep {gen}`;
     - `record.stop {reason?}`, which keeps the plain stop working.
   - **Agents:** `record.keep` is added to neither allow-list (`:5875`,
     `:5899`). A test asserts the refusal.
   - **`state_value` (`:1307`):** adds `for_call`, `stopping {gen, reason,
     in_s}` and `call_window {state, class}`.
     - The agent paths (`:5899` read-only and `:6010` Local-only) remove
       `call_window.class` for every agent origin.
     - The broadcast removes it in Local-only (`emit` at `:1297` /
       `local_now` at `:1539`).
   - **Logging:** at start, stop, adopt, keep, countdown start, cancel and
     expire, and "call ended, notes not owned". The `origin` is threaded from
     `handle` (`:5843`); `None` logs as `cli`.

   → verify: `cargo test` with new tests:
   - schema 2 start and adoption once per `since`, including `maybe` → `call`;
   - no adoption on `fallback`;
   - the schema 1 rule with no upper bound;
   - kept never re-adopted;
   - countdown A and B with stale Keep and stale expiry ignored;
   - a manual stop cancels;
   - `present` lost, then back, restarts;
   - no title, meeting id or window id in the captured logs.

   Traps:
   - `session.rs` is about 13k lines with its tests in the same file: add
     tests in the existing `mod tests`;
   - `parking_lot` locks are not `Send` across await;
   - the expiry task holds an `Arc<Daemon>` and must tolerate the daemon state
     having moved on.

3. **The sound clock, inactivity, and `inactivity_stop_minutes`.**
   - **Audio:** in `recording_task`'s audio branch (`session.rs:3467-3473`),
     next to `levels.feed`:
     - a per-channel sustained-sound detector: 300 ms above 0.008 within 1 s,
       `SOUND_LEVEL` from `transcription.rs:412`, made `pub(crate)`;
     - it sets `last_sound[ch]`, and records `last_frame`, which drives the 5 s
       gap reset.
   - **Rules, on a 1 s check in the same task:**
     - the inactivity countdown (reason `inactivity`) through step 2's
       countdown;
     - mic sound cancels it;
     - the chip state `inactivity_chip`, in `state`;
     - F1: 3 × N during `call` leads to a countdown with reason
       `inactivity_call`;
     - resets at every point in the decisions above.
   - **The setting:**
     - `src/core/models/settings.rs`: a field, its default 10, `keys`, the
       reader and the writer next to `auto_record_on_call` (`:223`, `:273`,
       `:298`, `:401`, `:473`);
     - `src/daemon/nix_settings.rs:55`: the key;
     - `nix/hm-module.nix`: an option `inactivityStopMinutes` (nullable int,
       ≥ 0), next to `autoRecordOnCall` (`:55`, `:258`);
     - the Settings row in `shell/Overlay.qml`, next to the existing toggles.

   → verify:
   - `cargo test` (fake levels and a fake clock):
     - every reset point;
     - no call leads to a countdown;
     - mic sound cancels;
     - system sound is ignored with no call;
     - a chip during a call;
     - 3 × N stops a call;
     - no frames means no stop;
     - 0 means off;
   - `nix flake check` (the HM eval).

   Traps:
   - the audio branch is the live path: no allocation per frame and no lock
     beyond the existing one;
   - keep the work O(samples).

4. **Auto screenshots of the meeting window (`session.rs` 2796–3110, 1147).**
   - **The capture function:** `screen_area(area, ppm)` becomes
     `capture(target: &Target, ppm)` with `Target::Window(String)`. The
     command is `grim -T <id>`, wrapped in `tokio::time::timeout(3 s)`. The
     manual `grab_screen` (`:1121`) keeps `slurp`.
   - **`auto_start_at` (`:3015`, called from `server.rs:111`):**
     - drop `slurp`;
     - bind to `g.call.window` when schema 2, `present`, `call`, and `bound`;
       otherwise return an error naming the cause;
     - a paused-closed state is restartable.
   - **`elig_gen: AtomicU64`:** bumped in `on_call_reading` on any change of:
     - the call, the window, `showing` or `window_seq`;
     - a countdown;
     - the lock;
     - Local-only or consent.

     `auto_capture_once` (`:2928`) reads it before `grim`, and stores only if
     it is unchanged and `auto_allowed` plus the D3 checks pass under the lock.
     This is next to the existing check at `:2975`.
   - **Window checks:**
     - the window must be in `hyprctl clients -j`;
     - visible means its workspace is a monitor's active or special workspace
       (`hyprctl monitors -j`);
     - a hidden window gets two `ppm` fingerprints 1 s apart, and identical
       ones mean a skip.
   - **The pause cause:** kept on `AutoShots`, reported as `autoshots.paused`,
     and logged.
   - **Stop → Play:** keeps the binding only if the same id exists and the
     call is on (the retained state at `:3367-3373`).

   → verify: `cargo test`, with the fake capture and fake `hyprctl` injected as
   in `auto_capture`:
   - each refusal and each pause cause;
   - an `elig_gen` flip that reverts mid-capture is discarded;
   - hidden and identical skipped;
   - the timeout;
   - restart after a close;
   - Stop → Play.

   Traps:
   - #84's tests rely on `auto_capture` taking the area string: update the
     injected fakes to the target;
   - never call `grim` in tests;
   - do not run `grim` against the owner's desktop during development.

5. **The shell.**
   - **`shell/Service.qml`:**
     - remove `forCall`, `ownerSince`, `pendingStop`, `stopTimer`,
       `handledFor`, `keptFor`, `slackMs` and the takeover, writer-gone and
       Keep branches (`:20-42`, `:150-191`);
     - keep auto-record, the "Take notes?" offer, and "The call ended. Notes
       are still recording", now only for `!client.state.for_call`;
     - on a change of `client.state.stopping` (a new `gen`), send
       `notify-send -A keep=Keep recording -t 30000` carrying `gen`. Its
       answer sends `record.keep {gen}`;
     - for the inactivity chip, show one notification with `-A stop=Stop notes`.
   - **`shell/BarWidget.qml`:**
     - a chip while `state.stopping`: "Notes stop in N s · Keep", ticking
       locally from `in_s`, and a click sends `record.keep`;
     - a chip while `state.inactivity_chip`: "No sound for N min · Stop notes";
     - the tooltip shows `call_window`: the app and class, "no meeting
       window", or "several possible windows".
   - **`shell/Overlay.qml`:**
     - the same two lines in the meeting view;
     - the auto screenshots label shows `autoshots.paused` (`:467-471`), and a
       start refusal shows its reason (`:157`).
   - **`shell/Client.qml`:** add `origin: "shell"` to every request (`:86`).

   → verify:
   - `bash shell/tests/lint.sh`;
   - `bash shell/tests/headless.sh` for each harness touched.

   Traps:
   - **run Quickshell only through `shell/tests/headless.sh`.** Starting it
     directly crashed the portals before (memory);
   - no `/tmp/nm.*` wildcards;
   - titles stay shell-local, never sent.

6. **Shell tests and the fake daemon.**
   - **`shell/tests/fake-daemon.py`:**
     - `state` carries `for_call`, `stopping`, `inactivity_chip`,
       `autoshots.paused` and `call_window`;
     - new switches set them;
     - `record.keep {gen}` clears `stopping`.
   - **`shell/tests/call-state.sh` and `harness-call.qml`:**
     - the #64 cases become display cases: the chip renders, Keep sends the
       right `gen`, the notification carries `gen`, and the old offer shows
       only without `for_call`;
     - drop the cases that tested shell-side timers (`autostop`, `takeover`,
       `gone-pending`, `maybe-end`, `manual-new`); their logic now lives in
       step 2's Rust tests.
   - **`harness-autoshots.qml`:** the paused label and the refusal reason.

   → verify:
   - `bash shell/tests/ci.sh`;
   - `nix build .#checks.x86_64-linux.shell`.

   Traps: the same as step 5. The fixtures must not contain real titles.

7. **The full build** (mine). Run:
   - `cargo test`;
   - `cargo clippy -- -D warnings`;
   - `bash shell/tests/ci.sh`;
   - `nix flake check`.

   Then update `README.md`:
   - auto screenshots are window-bound and need meeting-mode schema 2;
   - the inactivity setting;
   - the countdown chip.

   → verify: all pass; `git diff --stat` touches only the files named above.

8. **meeting-mode schema 2** (mine; a separate gate). After this plan is
   approved:
   - open an issue in `nixarchy-meeting-mode`;
   - write its intent → spec → plan from spec section A, with the same review;
   - stop for the owner at each gate.

   Its implementation is tracked there. meetingai can merge before it,
   provided step 9's schema 1 checks pass.

9. **Deploy and live verification** (mine; deploying needs the owner's OK).
   - **After the owner deploys to p620,** use `scratchpad/measure/sample.sh`
     (titles reduced to categories, no images):
     - **schema 1 now:** a Teams (`omateams`) call with notes started for it,
       then leaving. Expect the countdown chip with DND on, then a stop.
     - **notes started before the call:** expect "Notes are still recording"
       (no adoption with schema 1).
     - **inactivity with no call:** set N to 1 min for the test.
   - **After meeting-mode schema 2 is deployed:**
     - join and leave a Meet tab, a Meet PWA, Teams, a Discord voice channel
       and Zoom;
     - record the end and its delay, the window state, autoshot pauses, and
       the hidden repaint skip;
     - Discord and Chrome confirm F1's 3 × N behaviour.
   - The results go into the PR as text. The samples are deleted afterwards.

   → verify: the table in the PR, each row as expected.

## Tests

- `cargo test`: all existing tests pass, plus the new ones listed in steps
  1–4.
- `cargo clippy -- -D warnings`: clean.
- `bash shell/tests/lint.sh` and `bash shell/tests/ci.sh`: pass.
- `nix flake check`: pass, including the HM option.
- Live (step 9): the expected rows.

## Rollback

- **meetingai:** revert the merge commit and redeploy.
  - Rust and QML go back to the shell-side #64 logic.
  - The new settings key `inactivity_stop_minutes` is ignored by older builds.
- **meeting-mode schema 2** is additive: older meetingai ignores the new fields.
- **No database migration,** so nothing to undo in data.
