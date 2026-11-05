---
status: draft
issue: 84
intent: intent/2026-11-05-84-auto-screenshots.md
---

# Spec: Automatic screenshots during a meeting

**From the approved intent:** a slow timelapse plus phrases with a delay, not screen-change detection; the user picks the area; off by default and per meeting; never to a model unless attached by hand.

## Design

### S1. Turning it on

- **One control per meeting,** in the meeting view next to the language picker: "Auto screenshots". Turning it on runs `slurp` once, and the user draws the area to capture. Cancelling leaves it off. No global setting, no Nix default, and auto-record never starts it.
- **Commands:** `autoshots.start`, which picks the area, and `autoshots.stop`. Neither is on the agent (MCP) allow-lists.
- **It lives with the recording.** It captures only while recording, and stopping a recording stops it. The area is kept in memory for that meeting only, so Play on the same meeting keeps it, and New clears it.
- **Refused** in a Local-only meeting, with a reason. In interview mode it needs the same consent as recording, and the consent text mentions screenshots.
- **Visible while on:** the control shows "Auto screenshots on · 12" with the count, while recording.

### S2. When it captures

- **Timelapse:** one capture every 5 minutes.
- **Phrases:** a finished transcript line from either channel, lowercased, containing a phrase from a fixed list: "share my screen", "share my desktop", "sharing my screen", "can you see my screen", "here is the document", "here's the document", "here is the presentation", "here's the presentation", "next slide". Each match schedules captures 4 s and 10 s later, because a share appears after the words. English only, as with #71.
- **Limits:** at most 60 captures per meeting, and no more than one every 3 s. At the limit it stops, and says so on the control.
- **Transcript latency:** a line can reach the daemon about 0.4 s after speech on the remote channel and about 3 s on your own (`docs/eval/2026-11-call-pipeline.md`), so the delays run from when the line arrives, not when it was spoken.

### S3. What is captured

- `grim -g "<area>" -` for the chosen area, in the existing PNG path (`grab_screen`, `src/daemon/session.rs:1065`).
- **Screen locked or no session:** skipped, using the session's lock state. The plan confirms how to read it on Hyprland; if it cannot be read reliably, this feature does not ship.
- **The area is screen coordinates.** Whatever is there gets captured, including other windows over the call, and the overlay. The first-use text says so plainly. This is the accepted limit of choosing an area.
- **Duplicates:** each capture first takes a tiny fingerprint, `grim -g "<area>" -t ppm -s 0.05 -`, compared with the last stored one with a simple pixel difference. A near-identical frame is not stored. It needs no new image library.

### S4. Where they go

- **Storage:** `meeting_screenshots` (`src/db/repos/screenshots.rs`), with `question = "[auto]"` marking automatic ones. No schema change.
- **Never to a model:** they are written directly there and never touch `pending_capture` or `sources` (`src/daemon/session.rs:1828`, `:1992`). The plan checks every reader of `screenshots::list` so none sends them on. A test proves it.
- **Export:** they are exported with the meeting's screenshots. The exported file names mark them `auto-`. First-use text mentions that export folders may sync.
- **Review and delete:** a new `screenshots.delete {id}`. In the Library's notes column (#81), a "Screenshots" section lists a meeting's captures with their time, a preview on click, and Delete. Deleting removes the row, and the next export of that meeting removes the file. An already synced copy elsewhere cannot be reached, and the text says so.

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

- **Rust, with a fake capture:** off by default; refused in Local-only; needs consent in interview mode; agents cannot start it; timelapse and phrase schedules with delays; the limit and rate cap; duplicates dropped; skipped when locked; stops with recording; New clears the area; no automatic capture reaches a prompt or `pending_capture`; delete removes the row.
- **Shell:** the control, the count, the refusal, and the Screenshots section with delete, in the fake-daemon smoke.
- **Export:** `auto-` files appear, and go after a delete and re-export.
- **Manual, by the owner:** one real meeting with a shared screen, then look at what was kept.
