---
status: draft
issue: 60
intent: intent/2026-11-03-60-grounded-answers-transcript.md
phase: A (intent outcomes 1–5)
---

# Spec: Grounded answers and a searchable, nameable transcript (phase A)

Phase A of #60 covers intent outcomes 1–5. Phase B (line actions,
post-meeting view) gets its own spec. Speaker separation moved to #61.

## Design

### A1. Suggestions stay with their answer (outcome 1)

`src/daemon/session.rs` `spawn_follow_ups` (~1693) stores a suggestion only
when `g.turn == turn` but emits it regardless. Move the `emit("suggestion")`
inside that check and add `"turn": turn` to the event. `shell/Client.qml`
(`case "suggestion"`) ignores a suggestion whose `turn` is not `now.turn`.
This makes the fix hold on both sides, including on reconnect.

### A2. Rolling notes in the answer prompt (outcome 2)

In `system_prompt` (`session.rs` ~1185), when there is a live meeting
(`g.meeting_id`) and it has rolling-note chunks (`meetings::chunks`), add a
block **before** the transcript:

> Earlier in this meeting (generated notes, not quotes; may be incomplete):
> `<meeting_notes>…</meeting_notes>`

Chunks go in oldest first, but if the total is over `NOTES_BUDGET = 6_000`
characters the **newest** chunks are kept. The block is marked as untrusted
data, in the same style as the transcript block. The transcript instruction
"Only recent speech is supplied; do not assume earlier details" becomes "The
notes summarise earlier parts; the transcript below is the most recent
speech." Restored meetings are unchanged: they already get the recap and an
excerpt through the restored-meeting source.

### A3. Line identity and timestamps (outcome 3)

The identity of a line is `meeting_lines.id`, the UUID returned by
`meetings::add_line`. The rowid (`line`) stays the paging cursor of
`transcript.get` only.

- The `transcript.final` emits in `push_transcript` and `session_restore`
  add `"id"`. A line that could not be saved has no id, and nothing can link
  to it.
- `transcript.get` rows add `"id"`.
- `Client.qml` `addRow` stores a `lineId` role.
- `Transcript.qml`: the label of each speech bubble shows the time as `m:ss`,
  or `h:mm:ss` from one hour on, taken from `t0` when `t0 >= 0`, for example
  `Them · 12:04`. Nothing is shown for partial lines.

### A4. Transcript search and jump to line (outcome 3)

**Daemon.** New command `transcript.search {query, meeting?, limit?}`, backed
by a new repo function `meetings::search_lines`. It matches with
`instr(lower(text), lower(?))` (the same matching `meetings::search` uses),
orders by meeting `started_at` DESC then `start_time`, and returns at most 50
results (default 20). Each result is `{meeting, title, started_at, id, who,
speaker, text, t0}`, with `speaker` resolved through the names in A5. An
empty query, or one shorter than 2 characters, is an error.

**Jumping to a line.** `transcript.get` gains an optional `around: <line
id>` argument. It returns up to `limit / 2` lines before the target and up
to `limit / 2` after it, by rowid, in the same row format.

**Shell.**
- Library, Restore tab: the existing search field also calls
  `transcript.search` once the query is 2 or more characters, debounced
  (300 ms). It lists results under a **Lines** heading: time, speaker, the
  text with the match in bold, and the meeting title.
- Clicking a result:
  - If it belongs to the current meeting: close the Library and call
    `jumpTo(id)`.
  - Otherwise: `session.restore`, then `jumpTo(id)`. While recording,
    restoring is refused today (`require_idle`), so the row shows "Stop
    recording to open" and is disabled.
- `Client.jumpTo(id)`: if a row with that `lineId` is loaded, scroll to it
  and highlight it for 2 s. Otherwise call `transcript.get {around: id,
  limit: 200}`, replace the list with the result, and show a "Back to
  latest" button that runs `rebuild()`. Automatic follow-to-end is off until
  then.

### A5. Naming speakers (outcome 4)

**Command.** `speakers.rename {meeting?, key, name}`, which uses the existing
`meetings::name_speaker`. The meeting defaults to the current one, as in
`transcript.get`. `key` is either:
- the stored speaker number as a string, which is what export already uses;
  or
- `"them"`, which names every remote line that has no speaker (Parakeet).

An empty `name` deletes the row. `speakers.rename` is user-only: it is not
added to the MCP allowlist.

**One resolver.** A single helper, `speaker_label(names, who, speaker)`,
replaces the ad-hoc labels in four places: the `transcript.final` emit,
`transcript.get`, `session_restore` and `export_meeting`. It is also used in
the `system_prompt` transcript block, so new answers use the names. On a
rename the daemon emits `speakers.renamed {meeting, key, name}`, and
`Client.qml` updates the `speaker` of the loaded rows that match.

**Group name.** A name given to `"them"` shows as typed. Its tooltip says
"All remote speakers (this engine does not tell them apart)", so the UI does
not suggest it is one person.

**Shell.** In `Transcript.qml`, clicking the label of a remote bubble opens
an inline `Input` that is pre-filled; Enter saves and Escape cancels. The
key is the row's speaker number, or `"them"` when it has none. The row
therefore needs a `speakerKey` role next to the display name.

**Out-of-date notes.** A guarded migration in `src/db/mod.rs`, in the style
of the existing `ALTER`s, adds `meeting_speakers.updated_at`. `notes.get`
returns `"stale": true` when a speaker was renamed after the recap
(`meeting_recaps.updated_at`), or after the newest chunk if there is no
recap yet. The Library notes view then shows "Names changed after these
notes were written" with the existing **Regenerate** (`notes.regenerate`).
Nothing is regenerated automatically.

**Earlier answers keep their text.** An answer records what was said at the
time it was written; it is not wrong after a rename. Its "Based on" list
(A6) shows the current names, because they are resolved when it is drawn.
The intent's "no silent rewrites" rule is met this way: nothing is edited,
and the citations update. **This is a decision for the approver.**

### A6. Answers cite transcript lines (outcome 5)

**Prompt.** In `system_prompt`, every line in the transcript window is
prefixed with a local number: `[3] Speaker 2: …`. The numbers run 1 to n,
where n is at most 20. `begin_send` stores the mapping for the turn in
`Inner` (`cite: Vec<String>`, holding the line ids by number). The
instruction added to the prompt:

> When a sentence relies on a transcript line, end it with that line's
> number in brackets, like [3]. Cite transcript lines only, never the notes
> or documents. Do not cite when the answer comes from general knowledge.

**Parsing.** On a successful `finish_turn`, the daemon:
1. parses `\[(\d{1,2})\]` from the answer;
2. keeps only numbers in `1..=n`, without duplicates, in order of first use;
3. maps them to line ids and looks up `t0`, the speaker label and a text
   snippet of up to 160 characters;
4. emits `answer.sources {turn, sources: [...]}`, and stores the list in
   `Inner` so that `now.get` returns it.

The guarded interviewer path ends in `finish_turn` too, so it is covered.
Sources are not persisted in phase A. A restored chat shows answers without
"Based on"; the phase B post-meeting view covers that.

**Shell.**
- `answer.started` carries `cite_max: n`. While the answer streams, and
  afterwards, the NOW card and the archived answer card remove ` [k]` with
  `1 ≤ k ≤ cite_max`, outside back-tick code. Any other bracket text is left
  alone.
- Below the answer, a **Based on** row has one chip per source: `12:04 ·
  Speaker 2`, with the snippet as its tooltip. A click calls `jumpTo(id)`.
- When an answer was made with transcript context (`cite_max > 0`) but
  cites nothing, a dim caption reads "No transcript lines cited".
- `archiveNow` copies the sources onto the archived row as a JSON string
  role.

### A7. Agents (MCP)

- The MCP allowlist (`session.rs` ~4318) gains `transcript.search`.
- `src/mcp/mod.rs` gains a `transcript_search` tool. Results from local-only
  interviews are removed, in the same way as `meetings_search`.
- `now.get` (already allowed) returns `sources`.
- `transcript.get` with `around` keeps the existing local-only refusal.
- `speakers.rename` stays user-only.

### A8. Tests and fakes

- `shell/tests/fake-daemon.py` gains: `transcript.search`, `transcript.get`
  with `around`, `speakers.rename` (with the `speakers.renamed` event),
  `answer.sources`, `cite_max`, a `turn` on `suggestion`, and `id` on
  transcript rows.
- `shell/tests/smoke.sh` gains: the timestamp label, search → jump, rename,
  the Based on chips, and the stale-suggestion drop.

## Alternatives rejected

- **Inline markers left visible in the answer.** Rejected in the approved
  intent (open question 4). The markers are only how sources are carried;
  the UI hides them.
- **Asking the model for a JSON list of sources at the end.** This breaks
  streaming (the JSON tail would show while streaming, or the answer would
  have to be buffered), and small local models get it wrong more often than
  bracket numbers.
- **Matching sources afterwards with embeddings** (an `embedder()` exists).
  It guesses what the model used rather than recording it, and it adds
  latency and a dependency on the embedder being configured.
- **The rowid as line identity.** It is stable too, but `Entry.id`, export
  and the database key already use the UUID. The rowid stays the paging
  cursor.
- **An FTS index on `meeting_lines`.** `instr` already scans lines for
  `meetings::search` and is fast enough for a personal archive. Upgrade when
  a search measurably takes more than 200 ms.
- **Rewriting earlier answers or regenerating notes on rename.** Ruled out
  by the intent's "no silent rewrites" and "never regenerate automatically"
  decisions.
- **Persisting sources in `messages`.** Phase A only needs them for the
  answers on screen. Persisting would mean a migration and changes to the
  restore path for a feature phase B will design properly.

## Risks

- **Weaker models do not cite, or cite the wrong lines.** Only numbers that
  were offered are accepted, so a wrong cite still points at a real line;
  "No transcript lines cited" is worded as a fact, not a judgement. The
  caption could still be wrong in the "general knowledge" sense; it is
  accepted for phase A.
- **Longer prompts.** Up to 6 000 characters of notes plus line numbers,
  which is about 1.5k tokens more per live answer. With very small local
  context windows (≤ 4k), Ollama answers could get slower or be cut. The
  budget is a constant and easy to lower.
- **Stripping markers.** An answer that legitimately writes `[2]` outside
  code (a reference list, say) loses it when 2 ≤ `cite_max`. This is rare
  and only cosmetic.
- **The `around` view.** While the user looks at an older window, new live
  lines go into the model but the view does not follow. The "Back to latest"
  button must stay visible. The smoke test checks this.
- **Migration.** One additive `ALTER TABLE meeting_speakers ADD COLUMN
  updated_at`, guarded like the existing ones. Old rows get NULL, which
  counts as "not after the recap".
- **Local-only interviews.** Two new read paths for agents
  (`transcript_search`, `now.get.sources`). Both are filtered, and both are
  covered by a test that uses a local-only meeting.

## Verification

- `cargo test --workspace`, with new unit tests for:
  - A1: the turn gate (a stale follow-up is not emitted);
  - A2: notes budget and order (the newest are kept over budget), and the
    block is absent with no chunks;
  - A6: citation parsing (out of range, duplicates and unknown numbers are
    dropped; order is kept);
  - A4: `search_lines` (ordering, limit, minimum length) and `around`
    windows at the start and end of a meeting;
  - A5: label resolution in all four places, `"them"`, the stale flag after
    a rename, an empty name deleting the row, and `speakers.rename` refused
    from MCP;
  - A7: `transcript.search` hides local-only meetings from MCP.
- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`,
  `nix flake check --no-build`, and
  `nix build .#checks.x86_64-linux.shell` (the shell smoke test with the
  fake daemon), as in `.github/workflows/ci.yml`.
- A manual run on the real daemon (`shell/tests/smoke-real.sh` or a live
  call):
  - ask about something said 10 minutes earlier; the answer uses it;
  - a meeting answer shows "Based on" chips that jump to the line;
  - search finds a word in last week's meeting and opens it at that line;
  - rename "Them" and see the label change and the notes marked out of
    date;
  - suggestions never appear under the wrong answer.
