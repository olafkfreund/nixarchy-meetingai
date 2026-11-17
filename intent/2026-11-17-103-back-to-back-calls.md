---
status: draft
issue: 103
author: olafkfreund
---

# Intent: a new call gets its own notes; automatic starts are logged as such

## Problem

1. **Back-to-back calls share one note.** When a call ends, meetingai starts
   a 30 s countdown and then stops the notes. If any call is on again before
   the countdown ends, the countdown is cancelled. That is right for a short
   drop inside one call. It is wrong when a *different* meeting starts. Seen
   live on 2026-11-17: Teams ended at 12:50:12, a Meet call started 16 s
   later, and the Meet call was written into the Teams note.
2. **Automatic starts are logged as `reason="user"`.** The shell starts notes
   on its own when a call begins (`auto_record_on_call`). `record.start`
   carries no reason, so the daemon logs `reason="user"`. The log can't tell a
   click from an automatic start.

## Proposed outcome

- A different call starting during the call-end countdown stops the old
  notes at once, and the new call is handled as any new call is: notes
  start by themselves when `auto_record_on_call` is on, or the shell offers
  to start them.
- A short drop inside the same call (seen live: 2 s, 6 s and 20 s, same app
  and window class) still cancels the countdown and keeps the notes.
- The daemon log says `reason="call"` for an automatic start on a call, and
  `reason="user"` only for a click or a CLI start.

## Affected users and systems

- meetingai daemon (`src/daemon/session.rs`, call-end countdown and
  `record.start`), the shell (`shell/Service.qml`, the automatic start) and
  their tests (`shell/tests/`, lib tests).
- p620, where it is used daily. No change to meeting-mode or its state file.

## Constraints

- Never stop notes the user started for something other than this call:
  only notes owned by the ending call (`for_call`) are affected, as today.
- Keep (`record.keep`) still wins: kept notes are never stopped by this.
- Logs keep their privacy rule: a fixed set of reasons, never window titles
  or text a client chose.
- Deploying to p620 needs the owner's OK; p510 is not affected.

## Open questions

1. **What makes a call "different"?** Recommendation: a different window
   class or app from the call that ended (Teams → Meet). Two meetings in a
   row in the same app (two Meet tabs, or one Teams meeting after another)
   would then still share one note. A stricter test, a different window id,
   breaks on the Teams window that is replaced mid-call, as seen live at
   12:41 and 12:45.
2. **Old note's end time:** stop it at the moment the new call starts, or
   back-date it to when the old call ended? Recommendation: at the moment the
   new call starts; it is what "stop now" does today.
