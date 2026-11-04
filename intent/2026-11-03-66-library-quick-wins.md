---
status: approved
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

1. **A selected row.**
   - Clicking a row selects it and marks it with the theme's accent. So
     does Tab to the row and then Enter or Space: the row is focusable and
     has an accessible name, for example "Meeting: European Cyber Attacks,
     3 Nov 17:10".
   - Selecting a meeting loads its notes. The notes pane shows that
     meeting's title and date above the recap.
   - Selecting a chat clears the pane, because chats have no notes.
   - Only the selected meeting's notes can appear: a late reply for an
     earlier selection is dropped, and so is its stale flag.
   - Deleting the selected meeting clears the selection and the pane.
2. **Stable, quieter rows.**
   - Every row has the same layout: a kind marker, the title, an optional
     second line of metadata, and the date in a fixed column.
   - **Restore** and a "⋯" menu sit in the same place on every row.
     **Export** (meetings only) and **Delete** move into the "⋯" menu.
     Delete still asks for confirmation.
   - Selecting a row, or opening its menu, never moves the date or resizes
     the title.
   - The **Notes** button goes away, because selecting the row does its
     job.
3. **Useful metadata.** Meeting rows gain a second line with their
   duration and line count, for example `42 min · 412 lines`. Chat rows
   already show "Chat" and their last activity, and stay one line. This
   does not solve duplicate chat titles; title editing is out of scope.
4. **A readable recap.** Recap headings are capped at the overlay's own
   heading sizes.
5. **Honest states.**
   - For the selected meeting, the notes pane says "Loading notes…", "No
     notes yet for this meeting" or "Notes failed: …".
   - "No results" appears only when both the title search and the
     transcript-line search found nothing.
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
- **Existing `objectName`s stay** on the control that now does that job,
  because the smoke tests use them. `restore`, `sessionWhen`, `notesBox`,
  `notesGrip`, `notesStale`, `linesBox` and `lineHits` are unchanged.
  `exportNow` and `deleteSession` move to the menu items, so tests open
  the menu first. `openNotes` moves to the meeting row's focusable area,
  and only meeting rows carry it. The tests that use these are updated in
  the same change.
- **The surrounding panes keep working:** the notes grip and its
  keyboard resize, the export status line, and the transcript-hits box
  keep their behaviour and fit within the existing height calculations
  (`Library.qml:39-44`, `:306`).
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

1. **Where do the actions live?** Recommended, after review: **Restore**
   on every row, with **Export** and **Delete** in a "⋯" menu on every
   row. This keeps the columns stable.
2. **Does selecting a row load its notes, or only clicking Notes?**
   Recommended: selecting a meeting row loads its notes. That one click
   replaces the **Notes** button, which then goes away; its `objectName`
   moves to the row.
3. **Grouping by day** (Today, Yesterday, Earlier). Recommended: leave it
   for #60 phase B, to keep this change small.

## Review record (draft intent)

| Finding | From | Verdict |
|---|---|---|
| Dropping Notes needs a defined keyboard path: a focusable row, Enter/Space, an accessible name | Codex, agy | confirmed → outcome 1 |
| Selecting a chat must clear the previous meeting's notes and stale flag; states need a place | Codex, agy | confirmed → outcomes 1 and 5 |
| A selection-only Export makes the columns move; prefer Export and Delete in "⋯" | Codex | confirmed → outcome 2, open question 1 |
| Chats already show "Chat" and a date; the metadata line does not fix duplicate titles | Codex, agy | confirmed → outcome 3 reworded |
| The grip, export status and transcript-hits boxes must keep working | Codex, agy | confirmed → constraint |
| Menu items and the row must keep the smoke tests' `objectName`s; tests open the menu first | agy | confirmed → constraint |
| Deleting the selected meeting must clear the pane | agy | confirmed → outcome 1 |
| "No results" must consider both title and transcript hits | agy | confirmed → outcome 5 |
| `meetings::list` already returns duration and line count | Codex (could not verify) | verified: `meetings.rs` `list` selects `duration_seconds` and a line count |

