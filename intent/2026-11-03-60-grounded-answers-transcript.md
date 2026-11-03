---
status: approved
issue: 60
author: olafkfreund
---

# Intent: Answers that remember the meeting, and a transcript you can search, name and trust

## Problem

The live loop works (record, transcribe, answer, stream), but the user
cannot rely on it past the first few minutes of a call:

- **Answers forget the meeting.** The answer prompt carries only the last 20
  transcript lines (`src/daemon/session.rs` `system_prompt`) and tells the
  model not to assume earlier details. The rolling notes the daemon already
  writes every 8k characters or 5 minutes never reach the prompt.
- **Answers cannot be checked.** An answer carries no reference to the lines
  it was based on. A wrong answer about an earlier point looks as
  authoritative as a right one. Older answers fold to two plain lines with no
  link to the question that triggered them.
- **Suggestions can belong to the wrong answer.** Follow-up suggestions from
  an earlier turn are emitted after a newer turn has started
  (`session.rs` `spawn_follow_ups` stores only when `g.turn == turn` but
  emits regardless; `shell/Client.qml` appends without a turn check).
- **The transcript is a feed, not a record.** Rows have start/end times, but
  the view shows none. The daemon's `meetings.search` is not called by any
  part of the shell, and it returns whole meetings, not the matching lines
  (`src/db/repos/meetings.rs` `search`). The shell drops the stored line id
  when it loads a transcript (`shell/Client.qml` `addRow`), so nothing can
  point at or jump to one specific line. A misheard name cannot be fixed, a
  moment cannot be marked, and a line cannot be asked about.
- **Remote speakers are all "Them".** Speaker names are stored and exported
  (`meetings::name_speaker`, `meeting_speakers`) but no command or UI sets
  them, and `transcript.get` and the live prompt ignore stored names. With
  the default Parakeet engine every remote line has no speaker at all; only
  Deepgram separates speakers.
- **Nothing ties a finished meeting together.** Recap, items and transcript
  exist but there is no single view that links them by time.

## Proposed outcome

Observable by the user:

1. A suggestion only ever appears under the answer it was generated for.
2. A live answer late in a long call can use something said 20 minutes
   earlier, because the meeting's rolling notes are part of its context.
3. Every transcript row shows its time. A search field finds words in the
   current and past meetings and jumps to the line.
4. The user can name a separated speaker ("Speaker 2") once and the name
   shows in the live view, new answers, new notes and exports. Where the
   engine gives no speaker (Parakeet today), the remote side can be named as
   a whole ("Them" → "Acme team"), and the UI does not pretend it is one
   person.
5. An answer that relies on the meeting shows which transcript lines it
   used, in a "Based on" list under the answer; clicking one scrolls the
   transcript to that line, including in a past meeting. Citations point at
   transcript lines, never at generated notes. An answer from general
   knowledge is visibly different from one grounded in the meeting.
6. From a transcript line the user can correct its text, bookmark it, or ask
   the copilot about it.
7. After a meeting, one view shows the recap, items, bookmarks and transcript,
   each linked to the moment it came from.
8. (Separate issue, see open question 1.) With local Parakeet, remote
   speech is split into separate speakers without sending audio anywhere.

## Affected users and systems

- Anyone using the overlay during and after calls; all four modes.
- Daemon: `src/daemon/session.rs`, `src/daemon/protocol.rs`,
  `src/db/repos/meetings.rs`, `src/db/schema/`, `src/ai/recap.rs`,
  `src/speech/stt/` (speaker separation), `src/core/export.rs`,
  `src/mcp/mod.rs` (agents read the same data).
- Shell: `shell/Transcript.qml`, `shell/Overlay.qml`, `shell/Client.qml`,
  `shell/Library.qml`, test fakes in `shell/tests/`.
- Nix packaging if speaker separation needs a new model in the closure.

## Constraints

- **Local-first stays true.** Nothing new leaves the machine; local-only
  interviews stay local-only, and agents still cannot read them.
- **Interviewer guard holds.** Citations, corrections and "ask about this
  line" go through the same no-assessment guard as answers today.
- **Audio is still not stored.** "Jump to moment" means jump to the line,
  not play audio.
- **Encrypted database:** schema changes are additive migrations; existing
  meetings keep working and gain the new features where data allows.
- **Prompt budget:** adding notes and line references must not push live
  answers past the providers' context or slow them noticeably.
- **Corrections keep the original.** An edited line keeps what was heard, so
  notes and audits can tell the two apart.
- **No silent rewrites.** Notes, recaps and earlier answers made before a
  correction or a rename are shown as out of date, not quietly changed.
- **Stable line identity.** Search results, citations, bookmarks and
  corrections all refer to the stored line id, in live and past meetings.
- **MCP: read parity only.** Agents may read search results, bookmarks and
  citations through MCP (new commands added to the allowlist explicitly);
  editing lines and naming speakers stay user-only. Local-only interviews
  stay refused.
- Follows the Omarchy theme and the existing shell components; every new
  daemon command is covered in `shell/tests/fake-daemon.py` and the smoke test.

## Open questions

Each has a recommendation (Claude, agreed by Codex); approving the intent
accepts them unless you say otherwise.

1. **Delivery in phases.** Recommended: phase A = outcomes 1–5, phase B =
   6–7, each its own spec, plan and PR under #60. Outcome 8 (local speaker
   separation) moves to its own issue and intent, because its RAM, closure
   and accuracy cost needs measuring before anyone commits to it.
2. **Speaker separation.** Recommended: out of scope for #60; Parakeet stays
   "Them", nameable as a group (outcome 4).
3. **Corrections and notes.** Recommended: mark recap and notes out of date
   and offer "Regenerate"; never regenerate automatically during a call.
4. **Citation format.** Recommended: a separate "Based on" list tied to line
   ids, not inline markers, so it survives folding and Markdown changes.
