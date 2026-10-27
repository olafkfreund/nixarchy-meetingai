---
status: approved
issue: none (issues are disabled on olafkfreund/mixarchy-meetingai)
author: olafkfreund
reviews: Codex (gpt-6-sol) and agy, read-only, 2026-10-27; claims checked against the code by the session model
---

# Intent: Team meeting and Interviewer modes, with continuity across meetings

## Problem

The app helps one kind of user, the person *answering* (General, or the
Interview candidate). The owner also runs meetings and interviews:

- **As an interviewer**, there's no way to load a candidate's CV, prepare
  questions from it, track which ones were asked, or get follow-up probes
  based on the candidate's actual answers.
- **In team meetings**, notes are free-form markdown (`src/ai/recap.rs`).
  Action items, decisions and questions aren't stored in a structured,
  queryable form, so they can't be followed up.
- **Across meetings,** there's no series: nothing links this week's sync
  to last week's. Search covers titles and transcript lines but not notes
  (`src/db/repos/meetings.rs:186-193`). Nothing tells you that today's
  discussion touches a task opened last time.

Code facts (verified):
- A "Meeting" listen prompt is seeded (`src/db/schema/seed.rs:44`) but
  can't be reached: `meeting_mode` only accepts general/interview
  (`src/core/models/settings.rs:321`).
- Automatic answers trigger on the other side's questions
  (`src/daemon/session.rs:1016`).
- Restore already loads a meeting's recap as context, up to 3,000
  characters (`session.rs:1720`).

## Proposed outcome

1. **Team meeting mode** (`team_meeting`):
   - While recording, the existing rolling note chunks also produce draft
     **items**, each linked to the transcript line it came from:
     - action (owner and due date only if spoken)
     - decision
     - open question
   - At stop they're consolidated; the owner can edit, close or delete
     them.
   - The meeting view lists the items next to the notes.
2. **Meeting series and "Last time":**
   - When starting, pick an existing series or create one (for example
     "Weekly platform sync"). A "Last time" drawer shows the previous
     meeting's notes and open items.
   - During the meeting, the series' open items are in the model's
     context. When the discussion seems to touch one, a quiet
     "**Earlier task?**" callout appears with the item and its date, and
     you confirm it (mark it discussed, done, or not related).
   - Notes become searchable too.
3. **Interviewer mode** (`interviewer`), for when the owner interviews
   someone:
   - Before the call:
     - upload the candidate's CV and the job description to **that
       interview** (not to a reusable profile)
     - "Generate question bank" produces editable questions, each linked
       to the CV passage or job requirement it came from
   - During the call:
     - a question checklist (planned / asked / covered)
     - after the candidate finishes an answer, short **follow-up probes**
       grounded in what they actually said ("ask for their specific
       contribution", "no metric given")
     - the owner's own evidence notes
   - After the call: a factual recap of question → what the candidate
     said → gaps to verify.
4. **Safeguards for candidate data:**
   - a consent reminder before recording in interviewer mode
   - an optional "keep this interview local" switch (whisper + Ollama
     only)
   - deletion per candidate (CV, transcript, notes, question bank)

## Affected users and systems

- The owner, as a team-meeting participant and as an interviewer.
- Candidates, whose data is personal data.
- `src/daemon/**`, `src/ai/recap.rs`, `src/db/**` (new items, series and
  question-bank tables), `src/core/models/settings.rs`, `shell/**`
  (mode switch, items panel, "Last time" drawer, question checklist).
- MCP tools (they'll want items and series).

## Constraints

- **Interviewer mode must not:**
  - score, rank or grade candidates
  - give hiring recommendations
  - infer emotion, personality, confidence or truthfulness

  The existing rule in the interview recap prompt ("Give no verdict, score
  or hiring recommendation") extends to every interviewer feature. AI that
  evaluates candidates is high-risk under the EU AI Act (Annex III) and
  under UK and EU data-protection guidance.
- **Candidate data** is minimized and stays in the encrypted database. It
  is never added to a reusable profile, and can be deleted completely.
  Sending it to a cloud provider is the owner's explicit choice, and the
  local-only switch is offered.
- **Extracted items are drafts:** shown as suggestions with their source
  line and editable, never presented as fact. Owners and due dates are
  left blank unless spoken; local whisper has no speaker separation.
- **Bounded context:** prior-meeting context is limited to the structured
  open items plus the recap (≤ 4k characters), never raw old transcripts,
  so it doesn't crowd the 64k budget or slow answers.
- **No new heavy infrastructure:** no embedding model. Search over notes
  uses SQLite (FTS5 is available in the bundled SQLCipher; confirm in the
  spec).

## Open questions

1. **Series:** chosen by hand (proposed), or defaulted to the last meeting
   recorded with the same profile?
2. **"Earlier task?" callouts:** shown live during the meeting (proposed,
   at most one per few minutes), or only in the post-meeting notes?
3. **Interviewer local-only:** a per-interview switch (proposed), or
   forced on for every interviewer-mode session?
4. **Retention:** should candidate interviews auto-delete after N days
   (for example 90), or only on request?
5. **Order:** Team meeting + series first, then Interviewer, as both
   reviewers recommend? (Proposed: yes; ship as two plans.)
