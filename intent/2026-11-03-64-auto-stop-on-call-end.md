---
status: draft
issue: 64
author: olafkfreund
---

# Intent: Notes stop by themselves when the call ends

## Problem

When a call ends, because the meeting app closed or the user hung up, the
notes keep recording until the user stops them by hand.

- The shell already knows the call ended. `shell/CallState.qml` reads
  meeting-mode's `meeting-mode.json` and goes `call` → `none`, or
  `call` → `maybe` → `none`. A file whose writer has gone also reads as
  `none`.
- `shell/Service.qml` `onCallChanged` then only shows a notification:
  "The call ended. Notes are still recording.", with a **Stop notes**
  button. Nothing stops by itself.
- The result is a recording that runs on after the meeting, picking up
  unrelated speech and room noise. Its notes, recap and items then include
  that speech, and the user has to remember to stop it.

## Proposed outcome

1. **Notes stop by themselves.** When a call that reached `call` ends and
   notes were being taken for it, they stop after a short grace period.
   The daemon then finishes the notes as it does for a manual stop.
   - The grace period starts when the call leaves `call`, whether to
     `maybe` or `none`. A `maybe` that lasts the whole grace period counts
     as an end.
   - A detector whose writer has gone (`writerGone`, which also reads as
     `none`) is **unknown, not ended**. The detector may only be
     restarting: a new `pid` briefly reads as `none` until `/proc` confirms
     it (`CallState.qml:68-72`). That case gets today's "Stop notes" offer,
     with no automatic stop.
2. **A way to keep recording.** During the grace period a notification
   says "The call ended. Notes stop in 30 s." with a **Keep recording**
   button. Choosing it leaves the recording running, with no further
   prompt for that call.
3. **No stop if the call returns.** If any call is back at `call` before
   the grace period ends, the pending stop is cancelled. That covers a
   short `maybe` while the app reconnects, or a second call starting
   straight away. The returning call **takes over** the notes, even with a
   new `callSince`, so they still stop when that call ends. A reconnect
   longer than the grace period does stop the notes.
4. **Other recordings are left alone.** Notes that were not taken for
   this call keep running, for example a room recording started before
   the call. They get today's "Stop notes" offer.

## Affected users and systems

- Anyone whose calls are detected by meeting-mode (the
  `meeting-mode.json` writer). Without the detector nothing changes.
- Shell: `shell/Service.qml`, and possibly `shell/CallState.qml`. Tests:
  `shell/tests/` (a harness and fake call-state file).
- Daemon: no change expected. `record.stop` already finishes the notes.

## Constraints

- **Never stop a recording that is not this call's** (outcome 4). Today's
  `act()` checks the call, not the recording: with no call on it sends
  `record.stop` whatever is recording (`Service.qml:58-63`). A pending or
  old stop must therefore name the recording it was made for, the daemon's
  `state.meeting`. It is dropped if a different meeting is recording by
  then, or if none is. This also applies to today's "Stop notes" button.
  A pending stop is also cancelled as soon as recording stops, for example
  when the user stops by hand during the grace period.
- **Ownership must survive a shell restart.** The shell may restart in the
  middle of a call, so it cannot rely on remembering who started the notes.
  The rule must work from state the daemon and the detector already keep
  (see open question 3).
- **Interviewer mode:** stopping is safe. Consent applies to starting,
  not stopping.
- **No audio loss for a reconnect:** the grace period must cover a
  `maybe` blip.
- **No deploy while recording:** this repo's existing rule.

## Open questions

Each has a recommendation; approving the intent accepts them unless you
say otherwise.

1. **A setting, or always on?** Recommended: always on, with no new
   setting. The **Keep recording** button covers the exceptions, and
   another setting adds a Nix option, a Settings row and a lock path for
   little gain. Alternative: an `auto_stop_on_call_end` setting, on by
   default, next to `auto_record_on_call`.
2. **Grace period.** Recommended: 30 s, a constant in `Service.qml`.
3. **Which notes count as "for this call"?** Recommended: notes whose
   recording started at or after the call started. That compares the
   daemon's `state.since` with the detector's `callSince`, allowing a few
   seconds of slack, so notes started during a short `maybe` just before
   the call also count. It covers notes started automatically, from "Take
   notes", or by hand, and it still works after a shell restart. Notes that
   were already running when the call started are not stopped.

## Review record (draft intent)

| Finding | From | Verdict |
|---|---|---|
| `act()` checks the call, not the recording, so an old or pending stop can end a newer recording (`Service.qml:58-63`) | Codex, agy | confirmed → constraint: a stop names `state.meeting` |
| "For this call" cannot be an in-memory flag: a shell restart mid-call loses it, and the start order is racy | Codex, agy | confirmed → open question 3 uses `state.since` and `callSince` (both epoch ms, checked) |
| End handling ran only on `none`, so a lasting `maybe` never ended; the grace period must start when the call leaves `call` | Codex, agy | confirmed → outcome 1 |
| A reconnect with a new `callSince` left the notes owned by no call | agy | confirmed → outcome 3: the returning call takes over |
| A detector restart briefly reads as `none` and would show a false "Notes stop in 30 s" | agy | confirmed (`CallState.qml:68-72`) → outcome 1: a gone writer is unknown, offer only |
| A manual stop plus a new recording within the grace period could be stopped | agy | confirmed → constraint: cancel on stop, and the meeting check |
| A timer cannot promise no audio loss for any reconnect | Codex | agreed → outcome 3 says a reconnect longer than the grace period stops |

