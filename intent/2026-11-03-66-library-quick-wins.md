---
status: draft
issue: 66
author: olafkfreund
---

# Intent: A Library session list you can scan, with notes that say whose they are

## Problem

The Library's Restore tab (`shell/Library.qml`) is where the user finds a
past meeting or chat and reads its notes. A screenshot review on
2026-11-03, by Claude and Codex (`gpt-6-sol`), found that it reads as a
raw list.

- **The notes have no owner.**
  - Clicking **Notes** fills a pane below the list, but no row is marked
    as selected and the pane does not name the meeting.
  - Two rows can look highlighted at once (hover and focus).
  - A slow `notes.get` reply can replace the notes after the user has
    clicked another row (`Library.qml:335-340`).
- **The columns shift.** Chat rows hide **Notes** and **Export now**, so
  the date sits in a different place on chat rows than on meeting rows.
- **Every row has four equal text buttons**, and the destructive
  **Delete** is as prominent as **Restore** (`:334-351`).
- **Rows say nothing but a title and a date.** The daemon already reads
  each meeting's duration and line count (`meetings::list`), but
  `session.list` drops them (`session.rs` ~3515). Chat titles are their
  first utterance, so duplicates such as two "Can we move the capacity
  review…" rows are told apart only by the time.
- **The recap renders at raw Markdown sizes.** "Meeting recap" and
  "Summary" are larger than anything else in the overlay.
- **Missing states.** There is no "No notes yet", "Loading…" or "No
  results".
- **The search placeholder is wrong** ("Search settings, chats,
  profiles"). On this tab it searches session titles and transcript
  lines.

## Proposed outcome

1. **A selected row.** Clicking a row selects it with the theme's accent,
   and the notes pane shows that meeting's title and date above its
   recap. Only the selected meeting's notes can appear: a late reply for
   another meeting is dropped.
2. **Stable, quieter rows.** Every row has the same layout: a kind
   marker, the title, a second line of metadata, and the date in a fixed
   column. **Restore** stays visible on every row. Selecting a meeting shows
   its notes, so there is no separate **Notes** button. **Export** appears
   on the selected meeting only. **Delete** moves into a small "⋯" menu,
   still with its confirmation.
3. **Useful metadata.** Meeting rows show their duration and line count,
   for example `42 min · 412 lines`. Chat rows say they are chats and
   show their last activity.
4. **A readable recap.** Recap headings are capped at the overlay's own
   heading sizes.
5. **Honest states.** The pane says "No notes yet for this meeting",
   "Loading notes…" or "Notes failed: …". A search with no hits says so.
6. **The right placeholder.** On the Restore tab the search field reads
   "Search meetings, chats and transcripts".

## Affected users and systems

- Anyone opening the Library.
- Shell: `shell/Library.qml`, possibly a small component for the row and
  the More menu.
- Daemon: `session.list` in `src/daemon/session.rs` adds
  `duration_seconds` and `line_count` to meeting rows. These are fields
  `meetings::list` already returns, so it is an additive JSON change.
- MCP and agents: `session.list` is allowed for agents, and the new
  fields are harmless counts. Local-only interviews stay hidden as today.
- Tests: `shell/tests/` (the smoke harness and fake daemon) and a daemon
  test for the new fields.

## Constraints

- **The Omarchy look:** theme colours and fonts only; the contrast check
  (`shell/tests/contrast.py`) passes.
- **Existing `objectName`s stay**, because the smoke tests use them
  (`restore`, `openNotes`, `exportNow`, `deleteSession`, `sessionWhen`,
  `notesBox`, `notesGrip`, `notesStale`, `linesBox`, `lineHits`).
- **Delete keeps its confirmation.**
- **Keyboard access is not reduced.** Every action stays reachable by Tab
  and has an accessible name.
- **No change to what is stored.** A read-only daemon field is the only
  backend change.
- **Out of scope:** the list-plus-detail post-meeting view (#60 phase B,
  outcome 7), editing titles, and grouping by day.

## Open questions

Each has a recommendation; approving the intent accepts them unless you
say otherwise.

1. **Where do the actions live?** Recommended: **Restore** on every row,
   **Export** on the selected meeting only, and **Delete** in a "⋯" menu
   on every row. Alternative: Export and Delete both in the "⋯" menu.
2. **Does selecting a row load its notes, or only clicking Notes?**
   Recommended: selecting a meeting row loads its notes. That one click
   replaces the **Notes** button, which then goes away; its `objectName`
   moves to the row.
3. **Grouping by day** (Today, Yesterday, Earlier). Recommended: leave it
   for #60 phase B, to keep this change small.
