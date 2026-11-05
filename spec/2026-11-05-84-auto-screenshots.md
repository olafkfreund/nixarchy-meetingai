---
status: approved
issue: 84
intent: intent/2026-11-05-84-auto-screenshots.md
---

# Spec: Automatic screenshots during a meeting

**The owner approved, after review:** a slow timelapse plus phrases with a delay, not screen-change detection; the user picks the area; off by default and per meeting; never to a model unless attached by hand.

## Design

### S1. Turning it on

- **One control per meeting,** in the meeting view next to the language picker: "Auto screenshots". Turning it on runs `slurp` once, and the user draws the area to capture. Cancelling leaves it off. No global setting, no Nix default, and auto-record never starts it.
- **Commands:** `autoshots.start`, which picks the area, and `autoshots.stop`. Neither is on the agent (MCP) allow-lists.
- **It lives with the recording.** It captures only while recording. **Stop cancels it at once,** including queued phrase captures, not when the recording task finishes draining, up to 15 s later. New, restoring another session and deleting the meeting also clear it. The area is kept in memory for that meeting only, so Play on the same meeting keeps it, and the state survives neither a restart nor New.
- **Refused** in a Local-only meeting, with a reason. In interview mode it needs the same consent as recording, and the consent text mentions screenshots.
- **Checked again at every capture, not only at the start:** still recording this same meeting, not Local-only now, and consent still held. A meeting that turns Local-only stops it.
- **Visible while on:** the control shows "Auto screenshots on · 12" with the count, while recording.

### S2. When it captures

- **Timelapse:** one capture every 5 minutes.
- **Phrases:** a finished transcript line from either channel, lowercased, containing a phrase from a fixed list: "share my screen", "share my desktop", "sharing my screen", "can you see my screen", "here is the document", "here's the document", "here is the presentation", "here's the presentation", "next slide". Each match schedules captures 4 s and 10 s later, because a share appears after the words. English only, as with #71.
- **Limits:** at most 60 captures per meeting, and no more than one every 3 s. At the limit it stops, and says so on the control.
- **Transcript latency:** a line can reach the daemon about 0.4 s after speech on the remote channel and about 3 s on your own (`docs/eval/2026-11-call-pipeline.md`), so the delays run from when the line arrives, not when it was spoken.

### S3. What is captured

- `grim -g "<area>" -` for the chosen area, in the existing PNG path (`grab_screen`, `src/daemon/session.rs:1065`).
- **Screen locked:** skipped, using `hyprctl locked`. An error or unknown answer counts as locked, and it is checked again before a captured frame is accepted.
- **The area is screen coordinates.** Whatever is there gets captured, including other windows over the call, and the overlay. The first-use text says so plainly. This is the accepted limit of choosing an area.
- **Duplicates:** each capture first takes a tiny fingerprint, `grim -g "<area>" -t ppm -s 0.05 -`, compared with the last stored one with a simple pixel difference. A near-identical frame is not stored. It needs no new image library. Two `grim` runs can differ slightly, which does no harm. Codex thought this more than needed, given the 5-minute timer and 60-image limit, but it delivers the owner's approved "not hundreds of near-identical frames".
- **Off the shared lock.** Phrase handling only schedules a capture. `grim`, the comparison and the database write run in their own task, holding the daemon lock only to check state and to store, as final transcript handling and the audio loop use that lock.

### S4. Where they go

- **Storage:** `meeting_screenshots` (`src/db/repos/screenshots.rs`), with `question = "[auto]"` marking automatic ones. No schema change.
- **Never to a model:** they are written directly there and never touch `pending_capture` or `sources` (`src/daemon/session.rs:1828`, `:1992`). In `session.rs`, the only reader of `screenshots::list` is export (:3206); the plan checks the rest of `src/`, such as `recap.rs` and `mcp/`. A test proves it.
- **Export:** they are exported with the meeting's screenshots. The exported file names mark them `auto-`. First-use text mentions that export folders may sync.
- **Review and delete:** new `screenshots.list {meeting}` and `screenshots.delete {id}`, neither on the agent allow-lists. In the Library's notes column (#81), a "Screenshots" section lists a meeting's captures with their time, a preview on click, and Delete. Deleting removes the row, and the next export of that meeting removes the file. An already synced copy elsewhere cannot be reached, and the text says so.

## Alternatives rejected

- **Screen-change detection:** needs constant capturing, and a call's moving video looks like constant change. Dropped by the owner's approval.
- **Finding the call's window automatically:** `grim` works in screen coordinates and cannot isolate a covered window, so it would capture other things or skip often.
- **A global on switch:** could capture unattended through auto-record.
- **Local vision models to judge frames:** far beyond the ask.

## Risks

- **Private content in the chosen area.** This is the main risk, and limited by choosing an area, the visible indicator, the off default and delete, not eliminated.
- **Other people's shared material** may be confidential. The first-use text says to check. This is for the user to judge.
- **Depends on #81** for its review section. If #81 is not merged first, that part waits.
- **Database growth:** a 60-capture meeting at full-area PNG could reach tens of MB. The plan measures one, and limits resolution if needed.
- **Live call impact:** `grim` runs in a background task, and capturing must not touch the audio path.
- **Hyprland only verified here,** on p620.

## Verification

- **Rust, with a fake capture:** off by default; refused in Local-only; needs consent in interview mode; agents cannot start it; timelapse and phrase schedules with delays; the limit and rate cap; duplicates dropped; skipped when locked, or when the lock state is unknown; a queued phrase capture dropped by stop, and never landing in a new meeting; a meeting turning Local-only stops it; New, restore and delete clear the area; no automatic capture reaches a prompt or `pending_capture`; delete removes the row.
- **Shell:** the control, the count, the refusal, and the Screenshots section with delete, in the fake-daemon smoke.
- **Export:** `auto-` files appear, and go after a delete and re-export.
- **Manual, by the owner:** one real meeting with a shared screen, then look at what was kept.
