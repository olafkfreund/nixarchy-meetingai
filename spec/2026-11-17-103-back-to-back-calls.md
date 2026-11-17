---
status: draft
issue: 103
intent: intent/2026-11-17-103-back-to-back-calls.md
---

# Spec: a new call gets its own notes; automatic starts are logged as such

## Design

### 1. The daemon remembers which window class owns the notes

- `Recording` (`src/daemon/session.rs:339`) gains `call_class: String`, the
  window class of the call that owns the notes. Empty means unknown.
- It is set wherever `for_call` becomes true:
  - in `record_start` (`session.rs:3802`, `owns_call`), from
    `g.call.window_class`;
  - in `adopt_call` (`session.rs:1732`), from the adopted reading.
- While `for_call` is true and `call_class` is still empty, the first
  reading with `call == "call"` and a non-empty class fills it in. A
  reading that only names a class never changes a known one.
- `record.keep` (which clears `for_call`) leaves it as is; it is only read
  while `for_call` is true.

### 2. A different call stops the old notes

In `on_call_reading` (`session.rs:1757`), after `g.call = reading` and
before `adopt_call` and `update_call_end`:

- All of these must hold:
  - notes are recording and not stopping;
  - they are `for_call` and not `kept`;
  - the reading has `call == "call"` and a non-empty `window_class`;
  - `call_class` is non-empty and differs from it.
- Then:
  - `stop_locked(g, "new_call", "system")`, which also cancels any
    countdown;
  - `g.handoff = Some(reading.since)`.
- This covers both a new call during the countdown (`none`/`maybe` →
  `call`) and a direct `call` → `call` switch. A `maybe` reading never
  triggers it.
- Reading order matters. `adopt_call` skips stopped recordings (it filters
  `!r.stop`), so the new call is never adopted into the old notes.

### 3. The new call is handled once the old notes are saved

- **New state field.** `Inner` gains `handoff: Option<f64>`, the `since` of
  the call that stopped the old notes. `state_value` exposes it as
  `"handoff": <since>` or `null`.
- **When it is cleared:**
  - a recording starts;
  - the call reading's `since` changes to another call;
  - the call ends (`call != "call"`).
- **The shell retries.** `shell/Service.qml` adds a step to
  `onRecordingChanged`. When recording turns false and
  `client.state.handoff` equals `calls.since`, it calls `react()` through
  `Qt.callLater`.
- **Why the retry is safe.** `react()` returned `idle` for this call while
  the old notes were recording, before it set `offeredFor`. So it now
  starts notes (`auto_record_on_call`, outside interviewer mode) or offers
  them, exactly once.
- **Manual stops are unchanged.** A manual stop has no `handoff`, so it
  never restarts notes, as today.
- **Waiting for the save.** `client.recording` is false only once the
  daemon has finished the old notes (`recording: None`). If `record.start`
  is refused while the old meeting is still finishing, the shell's existing
  error notification shows, as for any failed start. The plan's tests must
  show that the ordinary path is not refused.

### 4. The reason for a start is sent, not guessed

- **`protocol::RecordStart`** gains `reason: Option<String>`.
- **The daemon's whitelist.** `record_start` logs `reason` from a fixed
  set: `"call"`, or `"user"` for anything else or a missing value.
- **The shell sends the reason.** `startNotes(quiet, title, reason)` passes
  it. `react()` sends `"call"`; every other caller (click, notification,
  IPC) sends nothing.
- **Unchanged:** CLI and MCP starts stay `user`, and the log never carries
  text a client chose.

## Alternatives rejected

- **Judge "different" by window id, or by `since`.** Teams replaces its
  call window mid-call; seen live at 12:41 and 12:45. `since` changes on
  every short drop, too. Both would split one meeting into several notes.
- **The daemon starts the new notes itself.** The decision belongs to the
  shell: `auto_record_on_call`, interviewer mode, consent, and the offer.
  Moving it would duplicate that logic.
- **Retry `react()` on every stop.** A manual stop during a call would then
  start new notes at once.
- **Back-date the old note's end.** Its transcript already holds the gap;
  changing only the end time would make it inconsistent.

## Risks

- **Same-app meetings still share one note:** two Meet calls, or two Teams
  meetings, in a row. This is accepted in the intent.
- **A window class that changes inside one call would split the note.**
  None was seen live (Teams kept `omateams`, Meet kept `google-chrome`). A
  browser tab dragged into its own window keeps its class.
- **A slow save could leave the new call without notes.** If saving the old
  meeting takes long, `react()` runs only when `recording` turns false. If
  that happens after the new call has ended, `handoff` is already cleared,
  so nothing starts. That is the right outcome.
- **Host:** p620 only. A deploy needs the owner's OK.

## Verification

- **Lib tests (`session.rs`):**
  - a `for_call` recording plus a call with another class stops with
    `new_call` and sets `handoff`;
  - the same class, an empty class, a `maybe` reading, kept notes, or
    notes that aren't `for_call` do not stop;
  - an empty `call_class` is filled from the first classed reading;
  - `handoff` clears on a new start, on another `since`, and on the call
    ending;
  - `record.start` logs `call` only for `reason: "call"`.
- **Shell tests (`shell/tests/call-state.sh`, `fake-daemon.py`):**
  - Teams → Meet during the countdown: the old notes stop, then new notes
    start, with the fake daemon logging `reason=call`;
  - same class: notes kept;
  - manual stop during a call: no new start.
- **CI:** `shell/tests/ci.sh` and `cargo test`.
- **Live on p620, after the owner's OK:**
  - a Teams call, then a Meet call within 30 s, gives two notes;
  - a short Teams drop keeps one note;
  - the daemon log shows `reason="call"` for automatic starts.
