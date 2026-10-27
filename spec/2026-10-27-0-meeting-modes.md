---
status: approved
issue: none (issues are disabled on olafkfreund/mixarchy-meetingai)
intent: intent/2026-10-27-0-meeting-modes.md
---

# Spec: Team meeting and Interviewer modes, with continuity across meetings

## Decisions on the intent's open questions (the owner approved the proposals)

1. **Series** are chosen by hand when a meeting starts. There is no
   automatic linking.
2. **"Earlier task?" callouts** appear live, at most one every 3 minutes,
   and are also listed in the post-meeting notes.
3. **Local-only** is a switch per interview (off by default; recommended in
   the consent banner).
4. **Retention** (no proposal was made, so the safe default): candidate
   data is deleted only on request, with a one-click "Delete candidate
   data". A retention timer can come later.
5. **Order:** Phase A (Team meeting + series) and then Phase B
   (Interviewer), each with its own plan.

## Design

### Modes

`meeting_mode` accepts `general | interview | team_meeting | interviewer`
(today `src/core/models/settings.rs:321` accepts only general/interview,
and the code checks `== "interview"` at `session.rs:599`, `session.rs:1968`
and `recap.rs:205`).
- Replace those string checks with one `Mode` enum in
  `src/core/models/settings.rs`.
- Each mode owns its four prompt kinds (`answer`, `live_answer`,
  `follow_up`, `recap`), seeded in `src/db/schema/seed.rs`. The unused
  seeded "Meeting" prompt becomes the `team_meeting` answer prompt.
- The header's segmented control gets four options: General · Interview ·
  Team · Interviewer.

| Mode | Automatic trigger | What appears live |
|---|---|---|
| general / interview | the other side asks a question (`session.rs:1016`, unchanged) | NOW answer + ASK NEXT |
| team_meeting | **no automatic answers**; the rolling-chunk timer (`recap.rs`, 8k characters / 5 min) | an **Items** panel (actions, decisions, questions) + an "Earlier task?" callout |
| interviewer | the **candidate finishes an answer** (`channel == 1`, end of utterance + pacing pause) | the **Question bank** checklist + 1–3 **probes** grounded in that answer |

### Phase A: Team meeting, items and series

**Schema** (idempotent, in `src/db/schema/schema.sql`):
```sql
CREATE TABLE IF NOT EXISTS series (id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE,
  created_at TEXT NOT NULL DEFAULT (datetime('now')));
-- meetings gains series_id (guarded ALTER, as for meeting_note_chunks.last_line)
CREATE TABLE IF NOT EXISTS meeting_items (
  id TEXT PRIMARY KEY, meeting_id TEXT NOT NULL, series_id TEXT,
  kind TEXT NOT NULL CHECK (kind IN ('action','decision','question')),
  text TEXT NOT NULL, owner TEXT, due TEXT,
  status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open','done','dropped')),
  source_line INTEGER, origin_item TEXT,   -- origin_item: an earlier item this one continues
  draft INTEGER NOT NULL DEFAULT 1, created_at TEXT NOT NULL DEFAULT (datetime('now')));
CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(meeting_id UNINDEXED, body);
```

**Extraction:**
- `recap::maybe_chunk` (unchanged timing) asks for the chunk's markdown
  **plus** a fenced JSON list of items, in one call, so there's no extra
  model call:
  `[{kind, text, owner|null, due|null, source_line, continues_item|null}]`.
- Parsing is strict (serde). Invalid JSON keeps the markdown and drops the
  items (logged).
- Items are written as `draft=1`. At stop, `generate` reconciles the items
  across chunks (merging duplicates) and sets `draft=0`.
- Owner and due date stay null unless spoken: the prompt says so, and a
  test checks the parser keeps null.

**Series and "Last time":**
- `session.new{series?}` and `series.list` / `series.create{name}`. The
  composer has a small series picker shown only in team_meeting mode.
- When a series is set, `last_time.get{series}` returns the previous
  meeting's recap (≤ 3k characters, reusing the restore path at
  `session.rs:1720`) plus the series' **open** items.
- The overlay shows a collapsible "Last time" drawer.

**"Earlier task?":**
- The open items of the series (id, kind, text, date; ≤ 4k characters)
  are added to the chunk prompt. When the model returns
  `continues_item: <id>`, the daemon emits
  `earlier_item{item, quote, line}`, at most once every 180 s.
- The overlay shows a callout with **Discussed** / **Done** / **Not
  related**, which updates `status` or links `origin_item`.
- The notes list every earlier item touched.

**Search:**
- The final recap goes into `notes_fts` at stop.
- `meetings.search` also matches `notes_fts MATCH ?`, so notes become
  searchable.
- MCP gains `meeting_items{meeting|series, status?}` and `series_list`, as
  read tools in the existing server.

### Phase B: Interviewer

**Schema:**
```sql
CREATE TABLE IF NOT EXISTS candidates (id TEXT PRIMARY KEY, meeting_id TEXT NOT NULL,
  name TEXT NOT NULL DEFAULT '', local_only INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT (datetime('now')));
CREATE TABLE IF NOT EXISTS candidate_documents (id TEXT PRIMARY KEY, candidate_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('cv','job')), name TEXT NOT NULL, text TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS interview_questions (id TEXT PRIMARY KEY, candidate_id TEXT NOT NULL,
  text TEXT NOT NULL, source TEXT NOT NULL,   -- quoted CV passage or job requirement
  competency TEXT, status TEXT NOT NULL DEFAULT 'planned'
  CHECK (status IN ('planned','asked','covered','skipped')), note TEXT, position INTEGER NOT NULL);
```

Candidate data is **scoped to one interview** (`meeting_id`). It is never
written to `profiles` or `profile_documents`, and never included in any
other meeting's prompt.

**Before the call:**
- The interviewer opens Interviewer mode and goes to the "Candidate" tab:
  name, attach a CV and a job description (through
  `core::context::import`), then "Generate question bank".
- `questions.generate{candidate}` runs one typed-model call that returns
  JSON: 8–12 questions, each with `source` (an exact quote from the CV or
  job description), `competency`, and an order.
- Questions whose `source` isn't found verbatim in the documents are
  dropped, to guard against invented CV claims. The interviewer can edit,
  reorder, add or delete questions.

**During the call:**
- **Coverage:** when a mic (channel 0) line is similar to a planned
  question (token similarity ≥ 0.6, the same helper as the echo dedupe),
  it is marked `asked`. The interviewer can tick any question as covered
  or skipped.
- **Probes:** after the candidate's turn ends (channel 1 + pacing pause),
  the live model returns 1–3 probes grounded in a quote from that answer
  ("You said you 'led the migration' — what was your part?"), shown in the
  NOW panel.
- **Evidence notes:** a small text field per question. Nothing is
  generated into it.

**After the call:** the recap follows `question → what the candidate said
(quotes) → gaps to verify`. It is evidence only.

**Hard limits:**
- The interviewer prompts and recap must never produce scores, ratings,
  rankings, hire/no-hire, or inferences about emotion, personality,
  confidence or honesty.
- A test feeds a transcript that invites a verdict and asserts the
  prompts forbid it. A post-filter rejects outputs containing
  verdict/score patterns, and retries once with a reminder.

**Consent and locality:**
- Starting a recording in interviewer mode shows a one-time-per-meeting
  banner: "Tell the candidate this interview is being recorded and
  transcribed."
- With `local_only=1`: STT must be `whisper-local` and chat must be
  `ollama`. Otherwise recording and generation are refused with a clear
  message; the provider fallback chain never leaves the machine.

**Deletion:** `candidates.delete{id}` removes the candidate, their
documents, the question bank, the meeting's lines, chunks, recap and FTS
row in one transaction. A test checks nothing is left.

## Alternatives rejected

- **Interviewer as a profile:** the candidate's CV would leak into
  unrelated prompts (Codex).
- **Extracting items per utterance:** too many tokens, and half-finished
  thoughts would hallucinate tasks (agy).
- **Extracting only at stop:** no live "Earlier task?"; chunk-time
  extraction costs no extra call.
- **Linking a series automatically by title or profile:** titles are
  generated after the meeting (`session.rs:1328`), so they're unreliable.
- **Local embeddings for earlier-task detection:** they'd compete with
  whisper for GPU memory to search about 20 items; prompt injection
  handles paraphrases.
- **AI scoring or rubric grades for candidates:** high-risk under the EU AI
  Act Annex III, and against the owner's constraints.

## Risks

- **Item extraction quality:** wrong or duplicate items. Mitigation: drafts
  plus reconciliation at stop, an edit UI, and source lines shown.
- **No speaker separation:** owners are often missing in team meetings.
  Documented; null is preferred to a guess.
- **Prompt size:** the chunk prompt grows by about 4k characters with the
  open items, within budget. Measure chunk latency in tests with the fake
  provider; real latency is checked in step 12.
- **Probe timing:** firing too early cuts into the candidate's answer;
  uses the same pacing pause as live answers.
- **Legal:** the app gives notice and minimizes data but can't guarantee
  compliance. The README states the interviewer is responsible for lawful
  use.

## Verification

- **Phase A:**
  - Unit tests: the chunk JSON parser (valid / invalid / null owner);
    reconciliation dedupe; the guarded `ALTER`; FTS5 creation inside
    SQLCipher (proves the bundled build has FTS5); search hits on recap
    text; the callout rate limit.
  - `daemon_flow`: a two-meeting series. Meeting 1 creates an action;
    meeting 2's fake transcript mentions it, so the `earlier_item` event
    fires and "Done" updates the status.
  - MCP: `meeting_items` and `series_list` in `mcp_flow`.
  - Screenshots: the Items panel, the "Last time" drawer and the callout,
    in the live-shots harness.
- **Phase B:**
  - Unit tests: question generation drops unquoted sources; coverage
    similarity; the probe trigger on channel 1; the verdict post-filter;
    `local_only` refusing cloud providers; candidate deletion leaving no
    rows.
  - `daemon_flow`: a full interviewer session with the fake provider.
  - Screenshots: the Candidate tab with the question bank, and a live
    probe.
- **Both:** the full suite, clippy, rustfmt, `qmllint`, both smoke tests,
  the contrast check and `nix flake check` all pass.
