---
status: approved
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
intent → spec → plan in `nixarchy-meeting-mode`. This plan does not implement
it. meetingai must work with schema 1 until then.

**The merge gate** (spec, Verification): the PR stays open until meeting-mode's
schema 2 is deployed and the full live matrix in step 9 passes.

## Wire contract (shared by every step)

**`state` fields.** These are always present; when idle they hold the null or
false shown here:

```json
{
  "for_call": false,
  "stopping": null,
  "inactivity_chip": null,
  "call_window": {"state": "none", "class": ""},
  "autoshots": {"on": false, "count": 0, "at_limit": false, "paused": null}
}
```

- **`stopping`:** `null` or `{"gen": u64, "reason": "call_end" | "inactivity" |
  "inactivity_call", "in_s": u32}`.
- **`inactivity_chip`:** `null` or `{"minutes": u32}`. It is cleared when a
  countdown starts and when sound returns.
- **`call_window.state`:** `"none"`, `"bound"` or `"ambiguous"`.
  `call_window.class` is a string, `""` when stripped or unknown.
- **`autoshots.paused`:** `null` or one of `"no_call"`, `"not_showing"`,
  `"countdown"`, `"window_closed"`, `"hidden_not_updating"`, `"locked"` or
  `"ambiguous"`.
  - The shell maps each to text, for example `window_closed` → "Paused:
    meeting window closed".
  - `on` stays true while paused. The Overlay toggle at `:153-155` must
    therefore read `paused == "window_closed"` as "press to re-bind" (it sends
    `autoshots.start`), not as stop.

**Commands:**
- **`record.keep {"gen": u64}`:** `{ok}` or an error, "no such countdown".
- **`record.stop {"reason"?: string}`:** the reason is free text, logged only.
  The shell's manual stop sends `"user"`; the inactivity chip's **Stop notes**
  sends `"inactivity_chip"`.
- **`autoshots.start {}`:** the error text names the cause: "no meeting window",
  "several possible windows", "meeting-mode is too old", "not in a call".

**Notifications** (shell). The `gen` is a property on the QML `Process`, as
`key` is today (`Service.qml:64`); it is not a `notify-send` flag. The bodies:

| Reason | Body |
|---|---|
| `call_end` | "The call ended. Notes stop in 30 s." |
| `inactivity` | "No sound for N min. Notes stop in 30 s." |
| `inactivity_call` | "No sound in this call for 3×N min. Notes stop in 30 s." |

Each has the action `keep=Keep recording`. The inactivity chip's body is "No
sound for N min." with the action `stop=Stop notes`.

## Steps

1. **New `src/daemon/callstate.rs`: the call-state reader, plus the shared
   hooks later steps use.**
   - **The reading:** `pub struct CallReading`. Each field maps from a JSON key
     in the file:

     | Field | JSON key | Default |
     |---|---|---|
     | `present` | derived | |
     | `schema` | `schema` | 1 when absent |
     | `call` | `call` | `none` |
     | `since` | `callSince` | |
     | `by` | `callBy` | `""` |
     | `window` | `callWindow` | `""` |
     | `window_state` | `callWindowState` | `none` |
     | `window_class` | `callWindowClass` | `""` |
     | `showing` | `callWindowShowing` | false |
     | `window_seq` | `callWindowSeq` | 0 |

     `callTitle` and `callApp` are not read.
   - **`fn parse(bytes, now, probe) -> CallReading`** applies this validation,
     as in `CallState.qml:22-31`:
     - a new pid counts as gone until `probe(pid)` (a `/proc/<pid>` exists
       check, injectable in tests) says otherwise;
     - no pid and `updated` older than 600 s means stale;
     - `call` other than `call` or `maybe` reads as `none`.
   - **The poll:** `tokio::spawn` a 1 s loop in `src/daemon/mod.rs`, next to
     the calendar loop at `:63`.
     - It re-parses whenever the file's mtime or size changes.
     - **Every tick, unchanged file or not,** it re-runs the pid probe in every state (a `stat`, cheap; *deviation:* the shell only probes during a call, but an idle dead writer must read as not `present` too), and the 600 s age rule. A gone or stale writer gives `present: false`, never "present, no call".
     - It calls `daemon.on_call_reading(r)` only when the reading differs.
   - **The shared hooks,** so later steps only extend them:
     - **`Inner`** (`session.rs:334`) gains `call: CallReading`.
     - **`Daemon`** (`:426`) gains `elig_gen: AtomicU64`.
     - **`on_call_reading(&self, r)`:** stores `r`, bumps `elig_gen`, logs
       the transition (present, schema, call, by, window_state), and calls
       `emit_state`.
     - **`emit_state` (`:1420`)** also bumps `elig_gen`. Every state change
       goes through it: consent, Local-only, countdowns, settings.

   → verify: `cargo test callstate`, with fixtures for:
   - schema 1 and schema 2;
   - a dead pid;
   - a new pid;
   - stale;
   - **a writer that dies with the file unchanged;**
   - a pid-less file aging past 600 s while unchanged;
   - garbage;
   - a file with titles (the test asserts no title in `Debug`).

   Traps:
   - never hold `inner.lock()` across `.await`;
   - the probe must not block the loop (`tokio::fs::metadata`).

2. **`session.rs`: ownership, the countdown, `record.keep` and logging.**
   - **`struct Recording` (`session.rs:327`)** gains `for_call`, `kept`, and
     `handled_since: Option<f64>`.
     - `record_start` constructs it at `:3222`.
     - `reset_session` (`:4268`) does not touch `g.recording`, so these fields
       survive it.
     - Set `for_call` at the construction from `g.call`, and only when
       `present`, `call == "call"` and `since > 0`:
       - schema 2: set;
       - schema 1: set when `recording_since ≥ since − 5 s`, as
         `ownerSince > 1` and `nosince` in `call-state.sh` require today.
     - With schema 1, a call reaching `call` while recording applies the same
       rule. That is not adoption: it covers notes started up to 5 s before the
       call.
     - **Transitional state:** between `:3222` and `:3366`, `g.recording` is
       `Some` while `g.meeting_id` is still unset or the previous one. Ownership
       and countdown logic require `g.meeting_id.is_some()` and
       `!r.stop.load()`. `finish_recording` (`:3597`) clears `stopping`.
   - *Implementation note:* `Stopping` also keeps the meeting id (internal, never in `state` or logs) so expiry can confirm the same recording, and the stop path takes an `origin` for its log line.
   - **Call end wins:** starting a `call_end` countdown replaces any inactivity
     countdown, with a new `gen`. The inactivity tick never replaces a
     `call_end` countdown.
   - **`on_call_reading` (from step 1)** is extended, after it stores the
     reading:
     - it applies adoption, guarded by `handled_since` and `kept`;
     - it re-evaluates the countdown condition.
   - **`stopping: Option<Stopping {gen, reason, at}>` on `Inner`:**
     - `start_countdown` spawns a task that sleeps 30 s, then calls
       `expire(gen)`.
     - **Split `record_stop` (`:3392`)** into
       `fn stop_locked(&self, g: &mut Inner, reason) -> Result<Value, String>`,
       holding today's body after the lock is taken, plus a thin
       `record_stop()` that locks and calls it.
     - **`expire(gen)`** takes the lock once, re-checks the `gen`, the
       recording id and the condition, and calls `stop_locked` with the guard
       it already holds. Never call `record_stop` while locked: it would
       deadlock.
     - `stop_locked` and `record_start` clear `stopping`.
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
   - **Logging:** at start, stop, adopt, keep, kept (a re-adoption refused),
     countdown start, cancel (with its cause) and expire, and "call ended, notes
     not owned". The `origin` is threaded from `handle` (`:5843`); `None` logs
     as `cli`. Inactivity chips and autoshot skips are logged in steps 3
     and 4.

   → verify: `cargo test` with new tests:
   - call-end priority when call end and inactivity apply together;
   - `call_window.class` absent from `state.get` for `mcp` and `agent` in
     normal mode, and from both the reply and the broadcast `state` event in
     Local-only;
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
     having moved on;
   - steps 1 and 2 share `on_call_reading`. Step 2 extends what step 1
     created.

3. **The sound clock, inactivity, and `inactivity_stop_minutes`.**
   - **Audio:** in `recording_task`'s audio branch (`session.rs:3467-3473`),
     next to `levels.feed`:
     - a per-channel sustained-sound detector: 300 ms above 0.008 within 1 s,
       `SOUND_LEVEL` from `transcription.rs:412`, made `pub(crate)`;
     - it sets `last_sound[ch]`, and records `last_frame`, which drives the 5 s
       gap reset;
     - **for testability,** the clock state lives on `Inner`
       (`SoundClock {last_sound: [Option<Instant>; 2], last_frame, window}`),
       with `fn feed_sound(&self, samples, now)` and
       `fn inactivity_tick(&self, now)`. `recording_task` calls both, and unit
       tests call them directly, because tests run with `capture: false`
       (`:164`, `:3284`) and never get `SessionEvent::Audio`.
   - **Rules, on a 1 s check in the same task:**
     - the inactivity countdown (reason `inactivity`) through step 2's
       countdown;
     - mic sound cancels it;
     - the chip state `inactivity_chip`, in `state`;
     - **during a call:** sound on *either* channel clears the chip and cancels
       an `inactivity_call` countdown;
     - F1: 3 × N during `call` leads to a countdown with reason
       `inactivity_call`;
     - resets at every point in the decisions above;
     - log each inactivity chip shown, and each countdown, with its reason.
   - *Implementation notes (step 3, coder):* the 1 s tick runs from the audio branch (no frames, no tick; a running countdown is then cancelled by `expire`); one lock covers the ring push and the sound feed; a change of `present` and `finish_recording` also reset the clock; no chip shows while a countdown runs; README gets the regenerated Nix options block (`docs-match`).
   - **The setting:**
     - `src/core/models/settings.rs`: a field, its default 10, `keys`, the
       reader and the writer next to `auto_record_on_call` (`:223`, `:273`,
       `:298`, `:401`, `:473`);
     - `src/daemon/nix_settings.rs`: the key is numeric, so it does not go in
       the boolean list at `:55-60`. It goes in the numeric validation path,
       with a range of 0 to 240;
     - `session.rs:149`: add it to `NUMERIC_SETTINGS`, growing the array to 7,
       which `settings.set` uses at `:4597`;
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
   - **Two kinds of checks.** `auto_timer` exits for good when `auto_allowed`
     fails (`:2863-2867`), so:
     - **terminating checks stay in `auto_allowed`:** a different meeting or
       generation, Local-only, no consent, and recording ended;
     - **the transient checks go into a new `auto_eligible`,** which returns
       the pause cause and never ends the timer: no call, not showing, a
       countdown, window closed, hidden and not updating, locked, ambiguous.
   - **`elig_gen`** (from step 1) is bumped by `on_call_reading` and
     `emit_state`. It is **also bumped explicitly** at these places, whether
     or not they emit state:
     - `set_candidate` (`:5222`);
     - `candidate.local_only` (`:6159`);
     - `record_start`, `stop_locked` and `record.keep`;
     - each countdown start and cancel;
     - the inactivity tick, when the chip changes;
     - the lock poll below.
     - **The lock** has no event, so while autoshots are on, step 1's 1 s loop
       also runs `hyprctl locked` (as `auto_lock_state`, `:2873`) and bumps
       `elig_gen` when the answer changes.
     - **Accepted by the owner with this plan (2026-11-16).**
     - **Remaining limit:** a lock and unlock that both happen within one tick
       *and* inside one capture (≤ 3 s) is not seen. The lock is also checked
       before and after `grim` (`:2946`, `:2967`).
   - **`auto_capture_once` (`:2928`):** read `elig_gen` before `grim`. Store
     only if it is unchanged and all of these pass under the lock, next to the
     existing check at `:2975`:
     1. `auto_allowed`: the #84 checks, the same meeting, not Local-only,
        consent;
     2. `g.call.present`, `g.call.call == "call"`, `g.call.window_state ==
        "bound"`, and `g.call.window` equal to the bound id;
     3. `g.call.showing`;
     4. `g.stopping` is none;
     5. the window was present in this capture's `hyprctl clients -j`.
   - **Window checks:**
     - the window must be in `hyprctl clients -j`;
     - visible means its workspace is a monitor's active or special workspace
       (`hyprctl monitors -j`);
     - a hidden window gets two `ppm` fingerprints 1 s apart, and identical
       ones mean a skip. "Identical" is byte equality (`a == b`), not
       `same_frame`'s tolerance.
   - **The pause cause:** kept on `AutoShots`, reported as `autoshots.paused`,
     and logged once per change of cause, so not every tick.
   - **Stop → Play:** keeps the binding only if the same id exists and the
     call is on (the retained state at `:3367-3373`). Run `hyprctl clients -j`
     **before** taking the lock at `:3296`: the `parking_lot` guard is held
     through `:3373`.

   → verify: `cargo test`, with the fake capture and fake `hyprctl` injected as
   in `auto_capture`:
   - each refusal and each pause cause;
   - an `elig_gen` flip that reverts mid-capture is discarded, for each source:
     a call reading, `callWindowSeq`, a countdown, consent, Local-only, and the
     lock poll;
   - hidden and identical skipped;
   - the timeout;
   - restart after a close;
   - Stop → Play.

   Traps:
   - #84's tests rely on `auto_capture` taking the area string: update the
     injected fakes to the target;
   - the 13 tests in `mod autoshots_tests` (`:11922`) set only `auto_area`.
     Their `daemon()` helper must seed a schema 2, bound, showing
     `CallReading` and a fake `hyprctl` hook (clients and monitors), or all of
     them fail;
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
     - the same two lines in the meeting view, plus the window identity line
       (the app and class, "no meeting window", or "several possible windows");
     - the auto screenshots label shows `autoshots.paused` (`:467-471`), and a
       start refusal shows its reason (`:157`);
     - the first-use confirmation at `:160` is rewritten. The old text was
       "whatever is in the area you choose"; the new text says it captures the
       meeting window only, even when hidden or covered, that the export
       folder may sync, and to check that you may capture what others share.
   - **`shell/Client.qml`:** add `origin: "shell"` to every request (`:86`).

   → verify: `bash shell/tests/lint.sh`. The harnesses run in step 6.

   - *Implementation notes (step 5, coder):*
     - `Header.qml`'s manual stop still sends `{}`; the daemon defaults a missing reason to `"user"`, so no change is needed there.
     - A countdown notification for a reason other than `call_end` reads "No sound for a while. Notes stop in N s."; the shell's notification Stop sends reason `notification`, the chip's Stop `inactivity_chip`.
     - The meeting view's countdown line shows `in_s` as last received; only the bar chip ticks.
     - *Deviation (step 4 fix):* with no meeting window (`callWindowState` `none`), `auto_eligible` now pauses with cause `no_window` instead of `ambiguous`, so the shell says "no meeting window" rather than "several possible windows".

   Traps:
   - **run Quickshell only through the test scripts,** which source
     `shell/tests/headless.sh`. Starting it directly crashed the portals before
     (memory);
   - no `/tmp/nm.*` wildcards;
   - titles stay shell-local, never sent;
   - **Overlay geometry:** a new line inside `convo` (`Overlay.qml:413-597`)
     must be counted in `convo.below` (`:550`), or `check-geometry.py` in
     `ci.sh` fails;
   - `call-state.sh` fails between steps 5 and 6. That is expected: step 6
     updates `harness-call.qml` and `fake-daemon.py`.

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
   - **`harness-autoshots.qml`:**
     - the paused label and the refusal reason;
     - the new first-use text, replacing the area text at `:45`.
   - **`shell/tests/smoke.sh:754`:** the "slurp is not installed" refusal
     assertion becomes the "no meeting window" refusal.
   - **Visual cases for the bar and the overlay.** `harness-call.qml` mounts
     only `Service`, so it cannot show the chips. Add scenarios `stopping`,
     `inactivity` and `window-states` to `live-shots.sh`. They cover:
     - the countdown chip;
     - the inactivity chip with its **Stop notes** action;
     - the three window-state texts in the tooltip and the meeting view.
   - **`shell/tests/ci.sh`:**
     - add `step call-state; bash "$here/call-state.sh"`, which it never ran
       before (`:15-24`);
     - add the new scenarios to the `live-shots` loop.
   - **`origin`:** a fake-daemon assertion that every request carries
     `origin: "shell"`.

   → verify:
   - `bash shell/tests/ci.sh`, which now includes `call-state` and the new
     shots;
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

   Its implementation is tracked there. This PR waits for it: the merge gate
   above.

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
       the hidden repaint skip, a screenshot taken while the window was hidden,
       and the countdown chip with Do Not Disturb on;
     - Discord and Chrome confirm F1's 3 × N behaviour.
   - The results go into the PR as text. The samples are deleted afterwards.

   → verify: the table in the PR, each row as expected.

### Review fixes (pre-merge review: Opus, Codex)

*Deviations, each with a test unless noted:*
- **Keep on an inactivity countdown** (`inactivity`, `inactivity_call`) only restarts the sound clock. Only Keep on `call_end` clears `for_call` and marks the recording kept, so a later call still adopts it, and the call's own end still counts down. (Approved decision "Keep clears `for_call`" is read as the call-end Keep, per spec B4: Keep "does not switch the backstop off".)
- **`hyprctl` calls** (`clients`, `monitors`, `locked`) run under the 3 s deadline with `kill_on_drop`, so a stalled compositor cannot stall the 1 s call-state loop. In `record.start`, a failed window lookup drops the retained autoshot binding instead of failing the start.
- **An unreadable lock state** pauses as `locked` *and* warns once ("none are being saved"), as #84 did and the README says.
- **A pause clears only when a capture is stored** (or matches the last frame), so a hidden, frozen window logs one cause, not "resumed" then `hidden_not_updating` every tick.
- **An expired countdown whose condition is gone** is cancelled for every reason, `call_end` included.
- **Logged `reason` and `origin` take known values only:** `record.stop` reasons other than `user`, `notification`, `inactivity_chip` log as `other`; origins other than `shell` (or none, `cli`) log as `other`. Agents can no longer put text in the logs.
- **Sound windows:** a burst across a 1 s window boundary counts: an adjacent window's count carries over, so in effect 300 ms within 2 s, erring on the side of not stopping.
- **A gap reset that clears the inactivity chip** now broadcasts state, so the shell does not keep showing it.
- **The call-reading log line** carries `event`, `reason` and `origin` like the others.
- README: only the bar chip is sure to show under Do Not Disturb; the notification may be hidden.
- *Not added:* tests for an `inactivity_call` countdown expiring into a stop, and frames stopping during a countdown. `expire` reads the real clock while the inactivity tests use a fake one; the expiry path itself is covered by the `call_end` expiry tests. Verified live in step 9 (F1).

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
