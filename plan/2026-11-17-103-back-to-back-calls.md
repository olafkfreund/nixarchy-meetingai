---
status: draft
issue: 103
spec: spec/2026-11-17-103-back-to-back-calls.md
coders:
  1: codex   # small, tricky: ownership, handoff ordering against stop_locked and finishing_meeting
  2: coder   # QML and fake-daemon edits following existing patterns in shell/tests
  3: coder   # short README paragraph
reviewers:
  - codex   # an independent read of the daemon handoff ordering
  - agy     # an independent read of the shell retry (out of quota until ~2026-11-24; reported if it fails)
---

# Plan: a new call gets its own notes; automatic starts are logged as such

## Approved decisions (self-contained)

**Intent and spec approved 2026-11-17.** Intent answers: "different" means
a different, known window class; the old note ends when the new call is
detected.

**Which notes are affected**
- Only notes that are recording, not stopping, `for_call` and not `kept`.
- A call-end Keep (`record.keep` on a `call_end` countdown) clears
  `for_call` and sets `kept`. An inactivity Keep keeps ownership
  (`session.rs:4025`).

**Owner class**
- `Recording` (`src/daemon/session.rs:339`) gains `call_class: String`, the
  window class of the owning call. `""` means unknown.
- It is set where `for_call` becomes true:
  - `record_start` (`session.rs:3802`, `owns_call`), from
    `g.call.window_class`;
  - `adopt_call` (`session.rs:1732`), from the adopted reading.
- While `for_call` is true and `call_class` is empty, the first reading with
  `call == "call"` and a non-empty class fills it in. A known class is never
  changed.

**Different call → stop**
- Where: `on_call_reading` (`session.rs:1757`), after `g.call = reading` and
  before `adopt_call` / `update_call_end`.
- When all of these hold:
  - the affected-notes test above passes;
  - the reading has `call == "call"` and a non-empty `window_class`;
  - `call_class` is non-empty and differs from it.
- Then, in this order:
  1. `g.handoff = Some(Handoff { token, at: Instant::now() })`, where
     `token` comes from a `u64` counter;
  2. `stop_locked(g, "new_call", "system")`.

  This order matters: `stop_locked` emits state (`session.rs:4013`), and
  that state must already carry the handoff. The stop also cancels any
  countdown.
- What triggers it and what doesn't:
  - it covers `none`/`maybe` → `call` during the countdown, and a direct
    `call` → `call` switch;
  - a `maybe` reading never triggers it;
  - `adopt_call` skips stopped recordings (it filters `!r.stop`), so the
    old notes never adopt the new call.

**Handoff readiness**
- **Why the shell can't use `recording`.** `recording: false` shows as soon
  as the stop flag is set (`session.rs:1460`). `record.start` is refused
  until both `recording` and `finishing_meeting` are `None`
  (`session.rs:3720`, `4241`, `4264`).
- **What `state_value` shows.** `"handoff": <token>` only while:
  - `handoff` is set and younger than 120 s;
  - `recording` is `None`;
  - `finishing_meeting` is `None`.

  Otherwise it shows `null`.
- **The ready signal.** Clearing `finishing_meeting` already emits state
  (`session.rs:4267`), so the shell sees the token appear.
- **When `handoff` clears.** Only on a recording start (`g.handoff = None`
  in `record_start`). A drop of the new call does not clear it.

**Shell retry (`shell/Service.qml`)**
- **New property.** `property double handledHandoff: 0`, the last token
  acted on.
- **Where it checks.** In a `checkHandoff()` function, called from
  `onStateChanged` (the `client` Connections) and from `onCallChanged` (the
  `calls` Connections).
- **What it does.** If `client.state.handoff` is a number other than
  `handledHandoff`, and `calls.call === "call"`, then:
  1. `handledHandoff = token`;
  2. `offeredFor = -1`;
  3. `react()`.
- **Why reset `offeredFor`.** meeting-mode keeps `callSince` across a
  direct `call → call` switch and across `call → maybe → call`; it resets
  only through `none` (`nixarchy-meeting-mode/Service.qml:131`,
  `SettingsModel.js:324`). So the new call can carry a key that `react()`
  already marked.
- **Manual stops.** They set no handoff, so they never restart notes.

**Start reason**
- **The protocol.** `protocol::RecordStart` (`src/daemon/protocol.rs:81`)
  gains `reason: Option<String>`, with `#[serde(default)]` as on the other
  fields.
- **The daemon's whitelist.** `record_start` logs
  `reason = if a.reason.as_deref() == Some("call") { "call" } else { "user" }`
  at `session.rs:3974`, so it never logs text a client chose.
- **The shell.** `startNotes(quiet, title, reason)` adds `reason` to the
  `record.start` args only when it is given. `react()` passes `"call"`;
  every other caller passes nothing.

**Accepted limits**
- Same-app back-to-back meetings still share one note.
- The new notes start only once the old meeting is saved, so its first
  seconds can be missing.

## Repo traps

- Logs carry only fixed values; never titles, transcript text or a client's
  string (privacy rule, #101).
- `session.rs` is large: edit by function name, then run
  `cargo test --lib daemon::session` before the full suite.
- Shell probes run through `shell/tests/headless.sh`; never start
  quickshell directly, and never use `/tmp/nm.*` wildcards.
- No load loops on p620; agents never stage, commit, deploy or restart.

## Steps

1. **Daemon** (`src/daemon/session.rs`, `src/daemon/protocol.rs`)
   - **Changes:** add `call_class`, `Handoff` / `g.handoff` and the token
     counter; the different-call stop in `on_call_reading`; `handoff` in
     `state_value`; clear it in `record_start`; `RecordStart.reason` and the
     whitelisted log.
   - **Lib tests**, next to the #101 call tests (around `session.rs:12620`):
     - a `for_call` recording plus another class → `record_stop` with
       `new_call`, and the emitted state has `handoff` set before
       `recording` is false;
     - no stop for: the same class, an empty class on either side, a
       `maybe` reading, `kept`, or notes that aren't `for_call`;
     - an empty `call_class` is filled from the first classed reading and
       never changed after;
     - `handoff` shows in the state only after `recording` and
       `finishing_meeting` are gone, and never after 120 s (use an
       injectable or back-dated `at`);
     - a recording start clears it;
     - `record.start` with `reason: "call"` logs `call`; with anything else,
       or nothing, it logs `user`.
   - **Verify:** `cargo test --lib` passes; `cargo clippy --all-targets -- -D warnings`
     is clean.
   - **Traps:** the stop must come after setting `handoff`; never log
     `reason` unfiltered.
2. **Shell** (`shell/Service.qml`, `shell/tests/fake-daemon.py`,
   `shell/tests/call-state.sh` and its harness)
   - **`Service.qml`:** `handledHandoff`, `checkHandoff()`, and the
     `startNotes` reason as above.
   - **`fake-daemon.py`:**
     - a configurable finishing delay that refuses `record.start` while it
       lasts;
     - `handoff` in the state with the readiness rule;
     - `new_call` stops on a class change;
     - log `record.start` with its reason.
   - **New `call-state.sh` cases:**
     - Teams → Meet during the countdown: the old notes stop, saving ends,
       then new notes start with `reason=call`;
     - a direct `call → call` switch with the same `callSince`: new notes
       start once;
     - Meet briefly `maybe` while Teams saves: new notes still start;
     - same class: notes kept;
     - a manual stop during a call: no new start.
   - **Verify:** `shell/tests/ci.sh` passes, including `call-state`.
   - **Traps:** use `headless.sh`; keep `react()`'s existing once-per-call
     behaviour for all other paths.
3. **README** (`README.md`, the stop-countdown paragraph near line 360)
   - **Change:** one short paragraph. A call in a different app during the
     countdown starts new notes once the old ones are saved. Two meetings in
     a row in the same app share one note.
   - **Verify:** read it back.
   - **Traps:** none.

## Tests

- `cargo test --lib`: all pass, including the new tests from step 1.
- `cargo clippy --all-targets -- -D warnings`: clean.
- `shell/tests/ci.sh`: passes, including the new `call-state` cases.
- Live on p620, after the owner's OK:
  - Teams, then Meet within 30 s: two notes;
  - a short Teams drop: one note;
  - the daemon log shows `reason="call"` for automatic starts.

## Rollback

Revert the PR's merge commit. Then bump `nixarchy-meetingai` in the nixos
config and deploy p620 with the owner's OK. No data migration: `handoff` and
`call_class` live only in memory.
