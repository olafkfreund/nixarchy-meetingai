---
status: draft
issue: none (issues are disabled on olafkfreund/mixarchy-meetingai)
spec: spec/2026-10-27-0-meeting-modes.md
reviewers:   # the owner wants agy and Codex as team mates; both read-only, after the Opus review
  - codex
  - agy
---

# Plan B: Interviewer mode

Phase B of the approved spec. It starts after Plan A
(`plan/2026-10-27-0-meeting-modes-a-team.md`) and its review fixes are
committed: both touch `session.rs`, `recap.rs` and `Overlay.qml`.

## Approved decisions (copied from the spec)

- **Mode:** `MeetingMode` gains `Interviewer` (`interviewer`). It has its
  own prompt kinds (`answer`, `live_answer`, `follow_up`, `recap`). The
  header gets a fourth button, "Interviewer".
- **Schema** (idempotent, `src/db/schema/schema.sql`):
  ```sql
  CREATE TABLE IF NOT EXISTS candidates (id TEXT PRIMARY KEY, meeting_id TEXT,
    name TEXT NOT NULL DEFAULT '', local_only INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')));
  CREATE TABLE IF NOT EXISTS candidate_documents (id TEXT PRIMARY KEY,
    candidate_id TEXT NOT NULL REFERENCES candidates(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('cv','job')), name TEXT NOT NULL, text TEXT NOT NULL);
  CREATE TABLE IF NOT EXISTS interview_questions (id TEXT PRIMARY KEY,
    candidate_id TEXT NOT NULL REFERENCES candidates(id) ON DELETE CASCADE,
    text TEXT NOT NULL, source TEXT NOT NULL, competency TEXT,
    status TEXT NOT NULL DEFAULT 'planned' CHECK (status IN ('planned','asked','covered','skipped')),
    note TEXT, position INTEGER NOT NULL);
  ```
- **Candidate data is scoped to one interview.** It is never written to
  `profiles` or `profile_documents`, and never put into any other
  meeting's prompt.
- **Before the call:** the Candidate tab takes a name, a CV and a job
  description (through `core::context::import`), then "Generate question
  bank". `questions.generate` makes one typed-model call that returns JSON:
  8–12 questions, each with `source` (an exact quote from the CV or job
  description), `competency` and an order. **Questions whose `source`
  isn't found verbatim in the documents are dropped.** The interviewer can
  edit, reorder, add and delete questions.
- **During the call:**
  - **Coverage:** a final mic line (channel 0) with token similarity ≥ 0.6
    to a planned question marks it `asked`. The interviewer can tick any
    question as covered or skipped.
  - **Probes:** when the candidate's turn ends (channel 1 + the pacing
    pause), the live model returns 1–3 probes, each grounded in a quote
    from that answer, shown in the NOW panel.
  - **Evidence notes:** one text field per question, never generated.
- **After the call:** the recap follows question → what the candidate said
  (quotes) → gaps to verify. It is evidence only.
- **Hard limits:** no scores, ratings, rankings, hire/no-hire, or
  inferences about emotion, personality, confidence or honesty. Prompts
  forbid it, a test checks they do, and a post-filter rejects outputs that
  match verdict/score patterns and retries once with a reminder.
- **Consent:** the first recording in an interviewer meeting shows a
  banner: "Tell the candidate this interview is being recorded and
  transcribed."
- **Local-only:** per interview, off by default, recommended in the
  banner. With `local_only=1`, STT must be `whisper-local` and every model
  call must be `ollama`. Otherwise recording and generation are refused
  with a clear message, and the fallback chain never leaves the machine.
- **Retention:** deletion on request only. `candidates.delete{id}` removes
  the candidate, their documents, the question bank, and the linked
  meeting's lines, chunks, recap, items and FTS row in one transaction. A
  test checks nothing is left.

## Decisions made in this plan

- **`candidates.meeting_id` is nullable.** Preparation happens before the
  meeting row exists (it's created at `record_start`). The candidate is
  chosen with `session.new{candidate}` (like `series`) and linked at
  `record_start`. One candidate links to one meeting; a second
  `record_start` on the same session resumes that meeting.
- **Consent is enforced by the daemon.** In interviewer mode,
  `record.start` without `{consent:true}` is refused with the error code
  `consent_required` until it has been given once for that meeting. The UI
  shows the banner and resends with `consent:true`. MCP's `record_start`
  can't pass consent, so agents can't start an interview recording.
- **Local-only is enforced in one place:** the chain builder at
  `session.rs:602` (`plan_chain` with `usable`) only allows `ollama`, and
  `speech_config` (`session.rs:613`) only allows `whisper-local`, while
  the session's candidate has `local_only=1`. Titles, chat, probes,
  notes and `questions.generate` all go through these two.
- **MCP and local-only:** for a local-only interview, MCP reads of that
  meeting's transcript, notes and NOW answer are refused ("This interview
  is local-only"). An agent could otherwise forward them to a cloud model.
- **The verdict filter** is a case-insensitive word-boundary list in
  `src/ai/guard.rs`: `\d+\s*/\s*10`, `score`, `rating`, `rank`,
  `hire`/`no hire`, `recommend (hiring|against)`, `strong|weak
  candidate`, `red flag`, `culture fit`, `nervous`, `confident`,
  `honest`, `dishonest`, `personality`, `seemed (anxious|unsure)`. A
  second hit after the retry returns the error "The answer contained an
  assessment and was withheld."
- **Probes reuse the live-answer path:** in interviewer mode,
  `completed_turn` (`session.rs:1078`) treats a meaningful candidate turn
  as eligible whatever `auto_response` says, and uses the interviewer
  `live_answer` prompt. No new timer.

## Repo traps

- cargo only via `devenv shell -- <cmd>`. rustfmt only your own files
  (`rustfmt --edition 2024 <files>`), never `cargo fmt`; not
  `src/db/mod.rs` or `src/db/repos/mod.rs` (they pull in modules).
- Coders cannot commit; the lead commits each step.
- Shell tests: `devenv shell -- env -u SIZE bash shell/tests/<x>.sh`
  (devenv exports `SIZE`, which breaks the fake daemon).
- Tests that pick STT set `runtime_settings.speech_provider =
  "deepgram-stt"`, except the local-only tests, which assert the refusal.
- Socket paths over 108 bytes fail: tests use `mktemp -d /tmp/nm.XXXX`.
- Add commands to the existing `handle` match in `session.rs`.

## Steps

1. **Mode and prompts.** `src/core/models/settings.rs`: `Interviewer`
   variant, `RECAP_PROMPT_INTERVIEWER` and its key. `src/db/schema/seed.rs`:
   the interviewer answer prompt (help the interviewer ask; never assess)
   and `live_answer` (1–3 probes, each quoting the candidate). `prompts` /
   `prompts_reset` accept `interviewer`; `shell/Library.qml` lists it.
   → verify by unit tests: settings round-trip; every interviewer prompt
   contains the no-assessment rule.
2. **Schema and repo.** The three tables; new `src/db/repos/candidates.rs`
   (create, get, list unlinked + recent, link_meeting, set_local_only,
   documents add/list, questions add/list/update/reorder/delete,
   `delete_all(id)` in one transaction covering the linked meeting's rows).
   → verify by unit tests: CRUD; `delete_all` leaves zero rows in every
   table that held the candidate's data (`meetings`, `meeting_lines`,
   `meeting_note_chunks`, `recaps`, `meeting_items`, `notes_fts`,
   `candidate_documents`, `interview_questions`, `candidates`).
3. **Guard.** `src/ai/guard.rs`: `assessment(text) -> Option<&str>` and
   `with_retry` around a model call. Wired into interviewer probes,
   follow-ups, chat answers and the recap.
   → verify by unit tests: hits ("7/10", "I'd hire her", "seemed
   nervous") and a miss on plain probes ("What was your part in the
   migration?"). False positives ("hire date") only cost one retry, so
   the list stays strict.
4. **Daemon commands.** `candidate.create{name}`, `candidate.get{id}`,
   `candidates.list`, `candidate.attach{id, kind, path}` (via
   `context::import`, like `profiles.attach` at `session.rs:2217`),
   `candidate.local_only{id, on}`, `questions.generate{candidate}`,
   `questions.add/update/reorder/delete`, `candidates.delete{id}`,
   `session.new{candidate?}`, the consent gate in `record.start`, local-only
   in the chain builder and `speech_config`, and the MCP refusal. Events:
   `questions.updated{candidate}`.
   → verify by unit tests: generate drops unquoted sources (fake provider
   returns one good and one invented source); consent refused then
   accepted; local-only refuses a cloud chat and deepgram STT; MCP-origin
   `transcript.get` refused for a local-only meeting.
5. **Live.** In interviewer mode: final channel-0 lines run coverage and
   emit `questions.updated`; channel-1 turns trigger probes through the
   live path with the guard. The recap uses the interviewer prompt plus
   the question bank and notes.
   → verify by a `tests/daemon_flow.rs` case: a full interviewer session
   with the fake provider (consent, a question marked asked, a probe, a
   guarded recap), then `candidates.delete` leaves nothing.
6. **Shell.** `Overlay.qml`: the "Interviewer" header button; in that mode
   a **Candidate** tab in the library (name, CV and job file paths with
   Attach, Generate, the editable question list with source quotes,
   local-only toggle, a two-click "Delete candidate data"); during the
   call the **Question bank** checklist replaces the Items panel, probes
   show in NOW, one evidence field per question; the consent banner before
   the first record. `fake-daemon.py --interviewer` and
   `live-shots.sh SCENARIO=interviewer`.
   → verify by lint, smoke, smoke-real, contrast and SCENARIO=interviewer
   crops at 100 % and 150 %.
7. **Docs.** README: an "Interviews you run" section with the limits, the
   consent duty, local-only, deletion, and that the interviewer is
   responsible for lawful use.
8. **Review** (session model): a fresh Opus review with this path and
   `git diff`, then Codex and agy per `reviewers:`.

## Tests

- `devenv shell -- cargo test --workspace --no-fail-fast` is fully green.
- clippy `-D warnings` is clean; `rustfmt --check` on the touched files.
- `lint.sh`, `smoke.sh`, `smoke-real.sh`, `contrast.py` pass.
- `git add -A && nix flake check` passes.

## Rollback

Each step is its own commit; `git revert` any of them. The tables are
additive; reverting leaves candidate rows unused, so delete them first
with `candidates.delete` if the owner wants them gone.

## Implementation split

Steps 1–5 go to one `coder` in order (all touch `session.rs`). Step 6 runs
in parallel in its own worktree against the protocol above with the fake
daemon; step 7 follows step 5. The session model merges, commits and runs
step 8.

## Deviations log (recorded with the implementation commit)

None yet.
