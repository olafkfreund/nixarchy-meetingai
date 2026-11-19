---
status: approved
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
- A call-end Keep (`record.keep` on a `call_end` countdown) clears
  `for_call` and sets `kept`; an inactivity Keep keeps ownership
  (`session.rs:4025`). `call_class` is read only while `for_call` is true and
  `kept` is false.

### 2. A different call stops the old notes

In `on_call_reading` (`session.rs:1757`), after `g.call = reading` and
before `adopt_call` and `update_call_end`:

- All of these must hold:
  - notes are recording and not stopping;
  - they are `for_call` and not `kept`;
  - the reading has `call == "call"` and a non-empty `window_class`;
  - `call_class` is non-empty and differs from it.
- Then:
  - set `g.handoff = Some(Handoff { token, at })` *first* (`token` from a
    counter, `at` now), then `stop_locked(g, "new_call", "system")`, so the
    state that reports the stop already carries the handoff; the stop also
    cancels any countdown.
- This covers both a new call during the countdown (`none`/`maybe` →
  `call`) and a direct `call` → `call` switch. A `maybe` reading never
  triggers it.
- Reading order matters. `adopt_call` skips stopped recordings (it filters
  `!r.stop`), so the new call is never adopted into the old notes.

### 3. The new call is handled once the old notes are saved

`recording: false` comes as soon as the stop flag is set, and `record.start`
is refused until both the recording and `finishing_meeting` are gone
(`session.rs:1460`, `3720`, `4241`, `4264`). So the shell must wait for an
explicit "ready", not for `recording` to turn false.

- **Daemon.** `Inner` gains `handoff: Option<Handoff { token: u64, at: Instant }>`.
  `state_value` shows `"handoff": <token>` only while all of these hold:
  - `handoff` is set and younger than 120 s;
  - `recording` is `None`;
  - `finishing_meeting` is `None`.

  Otherwise it shows `null`.
- **Telling the shell it's ready.** When `finishing_meeting` clears, the
  daemon already emits state, so the shell sees the token appear. A
  recording start clears `handoff`. A short drop of the new call does not
  clear it.
- **Shell.** `shell/Service.qml` keeps `handledHandoff`, the last token it
  acted on.
  - It checks from `onStateChanged` and from `onCallChanged`.
  - If `client.state.handoff` is a token other than `handledHandoff` and
    `calls.call === "call"`, it sets `handledHandoff` to the token, sets
    `offeredFor = -1`, and calls `react()`.
- **Why reset `offeredFor`.** meeting-mode keeps `callSince` across a direct
  `call → call` switch and across `call → maybe → call`; it resets only
  through `none` (`nixarchy-meeting-mode/Service.qml:131`,
  `SettingsModel.js:324`). So the new call can carry the old call's key,
  which `react()` already marked as offered. The token makes the handoff
  once-only on its own terms.
- **What `react()` does with it.** It starts notes (`auto_record_on_call`,
  outside interviewer mode) or offers them, exactly as for any new call.
- **Manual stops are unchanged.** A manual stop sets no handoff, so it
  never restarts notes.
- **A handoff that nobody uses expires.** If the new call is gone by the
  time saving finishes, nothing happens until it is on again; after 120 s
  the handoff is no longer shown.

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
  call window mid-call; seen live at 12:41 and 12:45. `since` resets on a
  drop through `none` and is kept across a direct switch, so it neither
  splits nor joins meetings reliably.
- **The daemon starts the new notes itself.** The decision belongs to the
  shell: `auto_record_on_call`, interviewer mode, consent, and the offer.
  Moving it would duplicate that logic.
- **Retry `react()` on every stop, or when `recording` turns false.** A
  manual stop during a call would then start new notes at once. It would
  also fire before saving ends, when `record.start` is still refused.
- **Back-date the old note's end.** Its transcript already holds the gap;
  changing only the end time would make it inconsistent.

## Risks

- **Same-app meetings still share one note:** two Meet calls, or two Teams
  meetings, in a row. This is accepted in the intent.
- **A window class that changes inside one call would split the note.**
  None was seen live (Teams kept `omateams`, Meet kept `google-chrome`). A
  browser tab dragged into its own window keeps its class.
- **A slow save delays the new notes.** The start waits until the old
  meeting has finished saving, so the first seconds of the new call can be
  missing from its notes. If saving takes over 120 s, the handoff expires
  and the shell's ordinary path for the next call takes over.
- **Host:** p620 only. A deploy needs the owner's OK.

## Verification

- **Lib tests (`session.rs`):**
  - a `for_call` recording plus a call with another class stops with
    `new_call` and sets `handoff`;
  - the same class, an empty class, a `maybe` reading, kept notes, or
    notes that aren't `for_call` do not stop;
  - an empty `call_class` is filled from the first classed reading;
  - the handoff token shows only after both `recording` and
    `finishing_meeting` are gone, and never after 120 s;
  - the state emitted by the stop already carries the handoff;
  - a recording start clears it;
  - `record.start` logs `call` only for `reason: "call"`.
- **Shell tests (`shell/tests/call-state.sh`, `fake-daemon.py`):**
  - the fake daemon gains a finishing delay, refusing `record.start` while
    it lasts;
  - Teams → Meet during the countdown: the old notes stop, saving ends,
    then new notes start, with the fake daemon logging `reason=call`;
  - a direct `call → call` switch with the same `callSince`: new notes
    start once;
  - Meet briefly `maybe` while Teams saves: new notes still start;
  - same class: notes kept;
  - manual stop during a call: no new start.
- **CI:** `shell/tests/ci.sh` and `cargo test`.
- **Live on p620, after the owner's OK:**
  - a Teams call, then a Meet call within 30 s, gives two notes;
  - a short Teams drop keeps one note;
  - the daemon log shows `reason="call"` for automatic starts.
