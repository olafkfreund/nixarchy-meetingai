---
status: approved
issue: 84
spec: spec/2026-11-05-84-auto-screenshots.md
coders:
  1: codex   # tricky: capture task, lifecycle, cancellation, lock discipline
  2: coder   # the shell control and review section: ordinary QML, after #81 merges
reviewers:
  - codex    # read-only: lifecycle and the model boundary
  - agy      # read-only: privacy paths and tests
---

# Plan: Automatic screenshots during a meeting

**Running order:** step 1, then step 2 once #81 is merged, since step 2 builds on its notes column. Claude runs checks and commits.

## Decisions carried from the approved spec

- **On:** one per-meeting control, available **only while recording**, since a meeting and its consent exist only from then. Turning it on runs `slurp` once to choose the area. Cancelling leaves it off. No global setting or Nix default; auto-record never starts it.
- **Triggers:** one capture every 5 minutes; on a finished line from either channel containing a listed phrase, captures 4 s and 10 s later. Phrases, lowercase: "share my screen", "share my desktop", "sharing my screen", "can you see my screen", "here is the document", "here's the document", "here is the presentation", "here's the presentation", "next slide".
- **Limits:** 60 captures per meeting, at most one per 3 s. At the limit it stops and says so.
- **Before each capture, and again before storing:** this same meeting is still recording; not Local-only; consent still held; `hyprctl locked` says unlocked. An error or unknown counts as locked.
- **Refused** to start in Local-only. In interview mode it needs recording consent.
- **Duplicates:** fingerprint with `grim -g "<area>" -t ppm -s 0.05 -`, compared with the last stored fingerprint by pixel difference. Near-identical frames are not stored.
- **Stop cancels at once,** including queued captures. New, restore and delete clear the area. Play on the same meeting keeps it. It is in memory only.
- **Storage:** `meeting_screenshots` with `question = "[auto]"`. No schema change.
- **Never to a model:** never `pending_capture` or `sources`.
- **Export:** file names start `auto-`. Deleting a row removes the exporter's own file on the next export. Anything else in the folder is left alone.
- **Commands:** `autoshots.start`, `autoshots.stop`, `screenshots.list {meeting}`, `screenshots.delete {id}`; none on the agent (MCP) allow-lists.

## Traps

- **Lock discipline.** `handle_live_event` (:2403) runs under `inner.lock()`, which the audio loop also uses. It only schedules. `grim`, comparison and the write run in their own task, taking the lock only to check state and to store.
- **`record_stop` (:2886) only sets a flag;** the recording task drains for up to 15 s before `finish_recording` (:3090). Cancel in `record_stop`, with a generation counter that queued tasks compare before capturing and before storing.
- **`slurp` waits on the user.** Never hold the lock across it, as at :1796.
- **Codex's sandbox cannot run nix, cargo or the shell suite.** Claude runs them. Nothing is committed during a handoff.
- **Tests must never run real `grim` or `hyprctl`:** the capture and lock check are injected.
- **No artificial CPU load on p620.**

## Steps

### 1. Daemon (codex) — `src/daemon/session.rs`, `src/daemon/protocol.rs`, `src/db/repos/screenshots.rs`, `src/core/export.rs`

1. `Inner.autoshots: Option<AutoShots>` with the meeting id, area, generation, count, last fingerprint and last capture time. Reset in `reset_session` (:3691), `session_restore` (:3760) and `session_delete` (:3981); kept on Play.
2. `grab_screen` (:1065) splits out a function taking an area. Capture and `hyprctl locked` go behind a small test seam, in the style of existing test hooks.
3. Commands: `autoshots.start` (`slurp` off the lock; refused when not recording, in Local-only, or without consent in interview mode), `autoshots.stop`, and state fields `autoshots: {on, count, at_limit}`. A candidate turning Local-only (:5566) stops it. The interview consent message (:2680) gains that screenshots may be captured, with a test on its wording.
4. Timer: a task spawned with the recording, ticking every 5 minutes. Phrases: in `handle_live_event`, on final lines, a phrase match schedules the two delayed captures.
5. `screenshots::delete(id)`; `screenshots.list` returning id, time, mime, whether automatic, and base64 data on request; `screenshots.delete`. Confirm by search that `screenshots::list` is read only by export (:3206), and nothing in `recap.rs` or `mcp/` reaches it.
6. `src/core/export.rs`: `Shot` knows if it is automatic, and is named `auto-NN` (:65). **`write_dir` (:281) keeps other files on purpose, and a test requires it (:655),** so removal needs ownership: the exporter records the `auto-` names it wrote in `screenshots/.auto-files`, and removes only names listed there that are no longer current. A user's file, even one named `auto-99.png`, is never removed. Test both.
7. **Tests:** off by default; refused in Local-only; consent; agents refused; both schedules and delays; limit and rate cap; duplicates; locked and unknown lock; a queued capture dropped by stop and never landing in a new meeting; a meeting turning Local-only stops it; New, restore and delete clear the area; Play keeps it; no automatic capture reaches a prompt or `pending_capture`; delete removes the row; export naming, and removal of only the exporter's files; consent wording.

Check, by Claude: `cargo test --workspace`, clippy `-D warnings`, fmt.

### 2. Shell (coder, after #81 merges) — `shell/Overlay.qml`, `shell/Library.qml`, `shell/tests/`

1. In the meeting view beside `meetingLanguage`: a toggle, `objectName: "autoShots"`, showing "Auto screenshots" and, when on, "on · 12". The first time, a confirmation explains what is captured: whatever is in the chosen area, including other windows, the export folder may sync, and check that you may capture shared material. The refusal shows as a notice.
2. In the notes column, a "Screenshots" section for the selected meeting, from `screenshots.list`: time, whether automatic, preview on click, and Delete with a confirmation.
3. The fake daemon and smoke test: the toggle, the count, the limit message, the refusal, the section and delete. `check-geometry` at 0 problems.

Check, by Claude: `bash shell/tests/ci.sh`.

### 3. Ship (Claude)

Full checks, `nix flake check`; reviews by a fresh Opus agent, Codex and agy, read-only; measure one 60-capture meeting's database growth, and limit resolution if tens of MB is too much; README section, including the privacy limits; a PR closing #84; merge after CI; a meetingai-only nixos bump, and deploy to p620 when nothing is recording. The owner tries one real meeting with a shared screen.

## Rollback

`git revert -m 1 <merge>`, and bump nixos. Automatic screenshots already stored stay with their meetings, and can be deleted.

## Deviations

(Recorded here during implementation, in the same commit as the code.)
