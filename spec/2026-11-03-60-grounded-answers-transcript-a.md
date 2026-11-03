---
status: draft
issue: 60
intent: intent/2026-11-03-60-grounded-answers-transcript.md
phase: A (intent outcomes 1–5)
---

# Spec: Grounded answers and a searchable, nameable transcript (phase A)

Phase A of #60 covers intent outcomes 1–5. Phase B (line actions and the
post-meeting view) gets its own spec. Speaker separation has moved to #61.

Revision 2 adds the confirmed findings of the Codex and agy reviews. They are
listed at the end.

## Design

### A1. Suggestions stay with their answer (outcome 1)

**Daemon.** In `src/daemon/session.rs` `spawn_follow_ups` (~1693), move
`emit("suggestion")` inside the existing `if g.turn == turn` check, and add
`"turn": turn` to the event.

**Reconnect.** `now_get` (~4262) returns `turn` (it is `g.turn`).
`shell/Client.qml` `rebuild()` sets `now.turn` from it instead of `-1`.
While an answer is streaming the value is still `-2`; the next delta
replaces it.

**Shell.** `Client.qml` `case "suggestion"` drops events whose `turn` is not
`now.turn`.

**MCP.** `src/mcp/mod.rs` `ask_daemon` (~190) accepts a `suggestion` only
when `e["turn"] == turn`.

### A2. Rolling notes in the answer prompt (outcome 2)

In `system_prompt` (`session.rs` ~1185), add a block before the transcript
when there is a live meeting (`g.meeting_id`) that has rolling-note chunks
(`meetings::chunks`):

> Earlier in this meeting (generated notes, not quotes; may be incomplete):
> `<meeting_notes>` `[N1] (0:00–5:12) …` `[N2] …` `</meeting_notes>`

- Each chunk is labelled `[Nk]` with its time span, oldest first.
- When the total is over `NOTES_BUDGET = 6_000` characters, the newest
  chunks are kept and k still counts from the oldest kept chunk.
- The block is untrusted data, worded like the transcript block.
- "Only recent speech is supplied; do not assume earlier details" becomes
  "The notes summarise earlier parts; the transcript below is the most
  recent speech."
- A restored meeting keeps its current restored-meeting source and gets no
  notes block.

### A3. Line identity and timestamps (outcome 3)

The identity of a line is `meeting_lines.id`, the UUID from
`meetings::add_line`. The rowid (`line`) remains the paging cursor of
`transcript.get` and nothing else.

- `Entry` (`session.rs` ~180) gains `t0: f64`. It is filled in
  `push_transcript` and `session_restore`.
- The `transcript.final` emits in `push_transcript` and `session_restore`
  add `"id"`. A line that failed to save has no id and cannot be linked.
- `transcript.get` rows add `"id"`. `Client.qml` `addRow` stores it as the
  `lineId` role.
- `shell/Transcript.qml`: the label of a speech bubble shows the time from
  `t0` when `t0 >= 0`, as `m:ss` or, from one hour, `h:mm:ss`. For example:
  `Them · 12:04`. Partial lines show no time.

### A4. Transcript search and jump to line (outcome 3)

**Daemon: search.** New command `transcript.search {query, meeting?,
limit?}`, backed by a new function `meetings::search_lines`.
- It matches with `instr(lower(text), lower(?))`, as `meetings::search`
  does.
- Results are ordered by meeting `started_at` descending, then by
  `start_time`.
- The default limit is 20 and the maximum 50. A query shorter than 2
  characters is an error.
- Each result is `{meeting, title, started_at, id, who, speaker, text, t0}`,
  with `speaker` resolved through A5.

**Daemon: window around a line.** `transcript.get` gains an optional
`around: <line id>`.
- The daemon looks up the line's meeting. If `meeting` is also passed and
  differs, the call is an error. The window is always taken from the line's
  own meeting.
- It runs two rowid queries in that meeting: up to `limit/2` lines with
  `rowid <= target` (newest first, then reversed), and up to `limit/2` with
  `rowid > target`. The rows have the same format as today.

**Shell: search UI.** In the Library Restore tab, the existing search field
also calls `transcript.search` once the query is at least 2 characters,
debounced by 300 ms.
- Results appear under a **Lines** heading: time, speaker, text with the
  match in bold, and the meeting title.
- Clicking a result in the current meeting closes the Library and jumps to
  the line.
- Clicking a result in another meeting runs `session.restore`, then jumps.
  While recording, restore is refused (`require_idle`), so those rows read
  "Stop recording to open" and are disabled.

**Shell: who owns the jump.**
- `Transcript.qml` owns the `ListView`, so it gets `jumpTo(id)`, which
  scrolls to the row and highlights it for 2 s. The highlight is a
  transient row role.
- `Client.qml` gets `openLine(id)`. If the row is loaded, it emits
  `lineRequested(id)` and `Transcript.qml` jumps.
- Otherwise it calls `transcript.get {around: id, limit: 200}` and, if the
  reply arrives and `currentMeeting` has not changed since the request, it
  replaces the rows, sets `viewingPast: true` and emits `lineRequested(id)`.

**Shell: while `viewingPast` is true.**
- `pushLine` does not add live rows to the window. It counts them instead.
- `Transcript.qml` shows a "Back to latest (N new)" button that calls
  `rebuild()` and clears `viewingPast`.
- Follow-to-end is off.
- `resetSession()` clears `viewingPast`.

### A5. Naming speakers (outcome 4)

**Command.** `speakers.rename {meeting?, key, name}`. The meeting defaults to
the current one, as in `transcript.get`. It uses `meetings::name_speaker`.
- `key` is the stored speaker number as a string, which is what export
  already uses, or `"them"` for remote lines with no speaker (Parakeet).
- An empty name deletes the row.
- It is user-only and is not added to the MCP allowlist.
- After the change it emits `speakers.renamed {meeting, key, name}`.

**Name cache.** `Inner` gains `speaker_names: BTreeMap<String, String>`. It
is loaded with the meeting at `record_start` and `session_restore`, updated
on rename, and cleared in `reset_session`. Live paths read the cache, not the
database.

**One resolver everywhere.** `meetings::label(who, speaker)` becomes
`label(names, who, speaker)`. The callers:
- `format_lines` and `transcript_excerpt` gain a `names` argument. Their
  callers pass `meetings::speaker_names(c, meeting)`: `src/ai/recap.rs`
  (rolling notes and recap, ~285/309/666/836), `meetings.rs` ~143/634,
  `session.rs` ~2663 and ~3187. This puts names into new notes, recaps and
  the restored-meeting context.
- `Self::who` takes the cache. That covers `transcript.partial`,
  `transcript.final`, `transcript.get`, `session_restore` and
  `transcript.search`.
- `export_meeting` (~2579) switches to the shared resolver and gains
  `"them"`.
- The transcript block in `system_prompt` uses the resolver, so new answers
  use the names.

**Group name.** A name set on `"them"` shows as typed. Its tooltip reads "All
remote speakers (this engine does not tell them apart)".

**Shell.**
- In `Transcript.qml`, clicking the label of a remote bubble opens an inline,
  pre-filled `Input`. Enter saves; Escape cancels.
- Rows get a `speakerKey` role: the number, or `"them"`.
- On `speakers.renamed`, `Client.qml` updates the matching rows, and also
  `lastSpeaker` and `now.asker` when they match.

**Notes marked out of date.**
- `meeting_speakers` gains a nullable `updated_at TEXT` in `schema.sql` and,
  for existing databases, through a guarded `ALTER TABLE … ADD COLUMN
  updated_at TEXT` in `src/db/mod.rs`. There is no non-constant default,
  since an `ALTER` cannot have one.
- `name_speaker` sets it to `strftime('%Y-%m-%d %H:%M:%f','now')` on both
  insert and conflict.
- `meetings::recap` selects `updated_at`. `meetings::chunks` selects
  `created_at`.
- `notes.get` returns `"stale": true` when the newest rename is at or after
  the recap's `updated_at`, or after the newest chunk's `created_at` when
  there is no recap. The comparison is a string compare. A rename within the
  same second sorts as later, which errs towards "stale"; the only cost is
  that a **Regenerate** button shows.
- The Library notes view shows "Names changed after these notes were
  written" with the existing **Regenerate** (`notes.regenerate`). Nothing is
  regenerated automatically.

**Earlier answers.** Their text stays as written, and the user is told.
- Each source chip keeps the speaker label from when the answer was made
  (see A6).
- After `speakers.renamed`, answer rows in the current view that were
  written before it show a dim caption: "Speaker names changed after this
  answer".

This follows the intent's "shown as out of date" rule. Codex asked for this;
agy preferred no caption.

### A6. Answers cite their sources (outcome 5)

**Sources are fixed at send time.** `begin_send` builds
`g.cite: Vec<Source>` from the same transcript window and chunks that
`system_prompt` numbers. A shared helper keeps the two in step.
- `Source` is `{kind: "line" | "notes", id, t0, t1, speaker, snippet}`.
- `snippet` is at most 160 characters.
- `speaker` is the resolved label at that moment.
- For a notes source, `id` is the first stored line with
  `start_time >= chunk.t0`, so a click still lands on the transcript.

**Prompt.**
- Each transcript line is prefixed `[k]`, with k from 1 to at most 20.
- Each notes chunk is prefixed `[Nk]`.
- The added instruction: *"When a sentence relies on the meeting, end it
  with the number in brackets: [3] for a transcript line, [N2] for the
  earlier notes. Do not cite general knowledge or documents."*

**On a successful `finish_turn`.**
- Parse `\[(N?)(\d{1,2})\]`.
- Keep only labels that were offered, drop duplicates, and keep the order of
  first use.
- Index into `g.cite`. Store the result as `g.answer_sources`.
- Emit `answer.sources {turn, sources}`.

The guarded interviewer path also ends in `finish_turn`. `g.cite` and
`g.answer_sources` are cleared in `begin_send`, `interrupt` and
`reset_session`, so a failed or replaced turn never shows old sources.
`now.get` returns `turn`, `cite_max` and `sources`. Sources are not
persisted in phase A; phase B's post-meeting view covers that.

**Shell.**
- `answer.started` carries `cite_max` (the number of lines) and
  `cite_notes` (the number of chunks).
- `NowCard.qml` and the archived answer card strip ` [k]` / ` [Nk]` markers
  whose number is within those counts, outside back-tick code, both while
  streaming and after.
- Below the answer, a **Based on** row has one chip per source:
  - a line: `12:04 · Speaker 2`;
  - notes: `Notes · 10:00–15:12`, labelled as notes and not as a quote.
  The tooltip is the snippet. A click calls `Client.openLine(id)`.
- When an answer was given meeting context but cites nothing, a dim caption
  reads "No meeting lines cited".
- `NowCard.qml` adds the height of the Based on row and caption to `extras`
  (~25), so the card neither clips nor overflows.
- `archiveNow` copies `sources` (as a JSON string), `cite_max` and
  `cite_notes` onto the archived row.
- `rebuild()` restores `cite_max`, `cite_notes` and `sources` from
  `now.get`.

### A7. Agents (MCP)

**`transcript.search`** goes on the MCP allowlist (~4318). The filtering
happens in the daemon's agent path, in the same way as `session.list`: in
`handle`, `transcript.search` from `origin == "mcp"` calls the handler with
`for_agent = true`. That drops every result whose meeting is in
`candidates::local_only_ids`, before the reply leaves the daemon. `meeting`
must not name a local-only interview; that is an error. `src/mcp/mod.rs`
gains a `transcript_search` tool.

**`transcript.get` with `around`.** The local-only guard in `handle`
(~4355) resolves the meeting from the line id when `around` is given, and
refuses local-only lines.

**`now.get`** (already allowed and already guarded) returns `turn`,
`cite_max` and `sources`. **`speakers.rename`** stays user-only.

**Not in scope.** agy noted that MCP `session.restore` is allowed today
without the local-only guard. That predates this work. I'll file it as its
own issue and leave it out of #60.

### A8. Tests and fakes

- `shell/tests/fake-daemon.py` gains:
  - `transcript.search`;
  - `transcript.get` with `around`;
  - `speakers.rename` and the `speakers.renamed` event;
  - `answer.sources`;
  - `cite_max` and `cite_notes` on `answer.started`;
  - `turn` on `suggestion` and in `now.get`;
  - `id` on transcript rows.
- `shell/tests/smoke.sh` gains checks for:
  - timestamp labels;
  - search, then jump, including the `around` window and "Back to latest";
  - renaming, including the earlier-answer caption;
  - Based on chips for a line and for notes;
  - a stale suggestion being dropped;
  - NowCard geometry with sources (the existing `check-geometry.py`).

## Alternatives rejected

- **Visible inline markers.** Rejected in the approved intent (open
  question 4).
- **A JSON list of sources at the end of the answer.** It breaks streaming,
  and small local models get it wrong more often than bracket numbers.
- **Matching sources afterwards with embeddings.** That guesses what the
  model used. It also adds latency and depends on the embedder being
  configured.
- **Citing only the last 20 lines.** An answer drawn from 20 minutes ago
  would show no source, which goes against outcome 5. Notes chunks are cited
  as time spans that open the transcript (Codex).
- **Retrieving older lines per question (search the archive and add the
  hits to the prompt).** It is a bigger change and adds a retrieval step to
  every live answer. Notes spans give a grounded pointer for much less.
  Revisit if notes turn out to be too coarse.
- **The rowid as line identity.** The UUID is already the key everywhere.
  The rowid stays the paging cursor.
- **An FTS index on lines.** `instr` is enough for a personal archive.
  Upgrade when search measurably takes more than 200 ms.
- **Rewriting earlier answers or regenerating notes automatically.** The
  intent rules both out.
- **Persisting sources in `messages`.** Phase B decides that, together with
  the post-meeting view.

## Risks

- **Weak models cite badly.** Only labels that were offered are accepted, so
  a wrong cite still points at a real line or span.
- **Longer prompts.** Notes add about 6 000 characters, plus the labels: roughly
  +1.5k tokens per live answer. Small local context windows (4k or less) may
  slow down or get truncated. The budget is a single constant.
- **Marker stripping.** A legitimate `[2]` outside code is hidden when it
  falls within range. It is rare and only cosmetic.
- **The `around` window while recording.** Live lines are counted, not
  shown, until "Back to latest". The smoke test covers this.
- **Migration.** One additive nullable column. NULL means "never renamed
  after the notes", so it is never stale.
- **Local-only interviews.** There are three agent read paths: search,
  `around`, and `now.get` sources. Each is guarded in the daemon and has a
  test that mixes local-only and ordinary meetings.
- **Name cache drift.** A rename via the daemon updates both the cache and
  the database in one locked step. Nothing else writes `meeting_speakers`.

## Verification

- `cargo test --workspace`, with new tests for:
  - **A1:** the turn gate, `now.get.turn`, and the MCP `ask_daemon` turn
    filter.
  - **A2:** budget, order, labels; no block when there are no chunks.
  - **A6:** parsing (out of range, `N`, duplicates, unknown labels), notes
    source resolving to the first line, sources cleared on interrupt and on
    a failed turn.
  - **A4:** `search_lines` (order, limit, minimum length); `around` at the
    start and end of a meeting; `around` with a mismatched `meeting`.
  - **A5:**
    - the resolver in partial and final lines, `transcript.get`, restore,
      export, `format_lines` and `transcript_excerpt`;
    - `"them"`;
    - an empty name deletes;
    - stale after a rename, including within the same second;
    - the migration on a database without the column;
    - `speakers.rename` refused from MCP.
  - **A7:** search and `around` from MCP never return local-only lines.
- `cargo clippy --workspace --all-targets -- -D warnings`;
  `cargo fmt --all -- --check`; `nix flake check -L --no-build`;
  `nix build .#checks.x86_64-linux.shell -L` (shell smoke with the fake
  daemon), as in `.github/workflows/ci.yml`.
- Manual run against the real daemon:
  - ask about something said 10 minutes earlier; the answer uses it and
    shows a notes chip that jumps;
  - search finds a word in last week's meeting and opens it at the line;
  - rename "Them": the label, the notes and the earlier answers are marked;
  - suggestions never appear under the wrong answer.

## Review record (revision 2)

| Finding | From | Verdict |
|---|---|---|
| Earlier facts uncitable (only 20 lines numbered) | Codex | confirmed → notes spans `[Nk]` |
| Names miss `format_lines` and `transcript_excerpt` (notes, recap, restore) | Codex, agy | confirmed → resolver with names everywhere |
| Names miss `transcript.partial`; `lastSpeaker`/`asker` not updated | agy | confirmed → name cache, Client update |
| Stale check: chunk `created_at` and recap `updated_at` not selected; `ALTER` default; second precision | Codex, agy | confirmed → nullable column, ms timestamp, `>=` |
| MCP search must be filtered in the daemon's agent path | Codex, agy | confirmed → `for_agent` in `handle` |
| `around` could read a local-only line via the current-meeting guard | Claude | confirmed → guard resolves the line's meeting |
| Reconnect loses turn, `cite_max` and sources | Codex, agy | confirmed → `now.get` returns them |
| MCP `ask_daemon` ignores suggestion turn | agy | confirmed (`src/mcp/mod.rs` ~190) |
| `Entry` lacks `t0`; resolve sources at send time | agy | confirmed → `Entry.t0`, `g.cite` |
| Jump belongs in `Transcript.qml`; stale `around` reply; live lines in a past window | Codex, agy | confirmed → `openLine`/`jumpTo`, `viewingPast` |
| NowCard `extras` ignores new rows | agy | confirmed (`NowCard.qml` ~25) |
| Earlier answers after a rename | Codex: mark; agy: keep as is | chose mark (intent rule) |
| MCP `session.restore` lacks the local-only guard | agy | real but predates #60 → separate issue |
