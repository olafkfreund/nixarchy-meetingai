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
2. **A way to keep recording.** During the grace period a notification
   says "The call ended. Notes stop in 30 s." with a **Keep recording**
   button. Choosing it leaves the recording running, with no further
   prompt for that call.
3. **No stop if the call returns.** If the call comes back within the
   grace period, for example a short `maybe` while the app reconnects,
   nothing stops.
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

- **Never stop a recording the user did not tie to this call** (outcome
  4). An old notification must not stop a newer call's notes; today's
  `act()` key check already handles that.
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
3. **Which notes count as "for this call"?** Recommended: notes started
   while this call was at `call`, whether started automatically, from the
   "Take notes" offer, or by hand from the bar or overlay. Notes already
   running when the call started are not stopped.
