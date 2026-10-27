---
status: approved
issue: none (issues are disabled on olafkfreund/mixarchy-meetingai)
spec: spec/2026-10-27-0-meeting-modes.md
reviewers:   # the owner wants agy and Codex as team mates; both read-only, after the Opus review
  - codex
  - agy
---

# Plan A: Team meeting mode, items and series

Phase A of the approved spec. Phase B (Interviewer) gets its own plan after
this lands.

## Approved decisions (copied from the spec)

- **Modes:** `meeting_mode` accepts `general | interview | team_meeting`
  (`interviewer` is added in Phase B). One enum replaces the string checks.
  Each mode owns its prompt kinds (`answer`, `live_answer`, `follow_up`,
  `recap`). The unused seeded "Meeting" prompt (`seed.rs:65`) becomes the
  `team_meeting` answer prompt. The header gets General · Interview · Team.
- **Team meeting gives no automatic answers.** The live output is the
  rolling-chunk timer (`recap.rs`, 8k characters / 5 min, unchanged): an
  **Items** panel and an "Earlier task?" callout. Typed questions still work.
- **Schema** (idempotent, `src/db/schema/schema.sql`):
  ```sql
  CREATE TABLE IF NOT EXISTS series (id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')));
  -- meetings gains series_id TEXT (guarded ALTER, like meeting_note_chunks.last_line)
  CREATE TABLE IF NOT EXISTS meeting_items (
    id TEXT PRIMARY KEY, meeting_id TEXT NOT NULL, series_id TEXT,
    kind TEXT NOT NULL CHECK (kind IN ('action','decision','question')),
    text TEXT NOT NULL, owner TEXT, due TEXT,
    status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open','done','dropped')),
    source_line INTEGER, origin_item TEXT,
    draft INTEGER NOT NULL DEFAULT 1, created_at TEXT NOT NULL DEFAULT (datetime('now')));
  CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(meeting_id UNINDEXED, body);
  ```
- **Extraction:** in team mode the chunk call returns the chunk's markdown
  **plus** one fenced ```json list
  `[{kind, text, owner|null, due|null, source_line, continues_item|null}]`,
  in the same model call. Parsing is strict (serde). Invalid JSON keeps the
  markdown, drops the items, and logs a warning. Items are stored
  `draft=1`. At stop, `generate` merges duplicates across chunks and sets
  `draft=0`. Owner and due date stay null unless spoken.
- **Series:** chosen by hand. `session.new{series?}`, `series.list`,
  `series.create{name}`. A series picker shows in the composer only in team
  mode. `last_time.get{series}` returns the previous meeting's recap
  (≤ 3k characters) plus the series' open items.
- **"Earlier task?":** the series' open items (id, kind, text, date;
  ≤ 4k characters) go into the chunk prompt. When an item comes back with
  `continues_item: <id>` naming a real open item, the daemon emits
  `earlier_item{item, quote, line}`, at most once every 180 s. The callout
  offers **Discussed** (links `origin_item`), **Done** (status done) and
  **Not related** (nothing stored). The recap lists every earlier item
  touched.
- **Search:** the final recap goes into `notes_fts` at stop (replacing that
  meeting's row). `meetings.search` also matches `notes_fts MATCH ?`.
- **MCP** gains two read tools: `meeting_items{meeting?|series?, status?}`
  and `series_list` (12 → 14 tools).

## Decisions made in this plan

- **The enum is `MeetingMode`,** not `Mode`: `Mode` already exists
  (`src/core/models/domain.rs:12`, Listen/Ask). It serializes to the same
  snake_case strings, so stored settings and the socket protocol don't
  change.
- **Dedupe** reuses the echo filter's `tokens`/`similarity`
  (`src/speech/stt/filter.rs:25,33`) made `pub(crate)`; same kind and
  similarity ≥ 0.8 merge into the earlier item.
- **The FTS query is escaped:** the user's text is wrapped as one FTS5
  string (`"` doubled), so `50%_` or `-` can't raise a syntax error; the
  existing `instr` search stays for titles and lines.
- **Items edits** go through `items.update{id, status?, text?, owner?, due?,
  origin_item?}` and `items.delete{id}`. The UI edit is inline text plus
  the status buttons; nothing more.

## Repo traps

- cargo only via `devenv shell -- <cmd>`. rustfmt only your own files
  (`rustfmt --edition 2024 <files>`), never `cargo fmt` on the crate.
- Coders cannot commit (guard hook); the lead commits each step.
- Worktrees: create them from HEAD by hand
  (`git worktree add -q -b <br> .claude/worktrees/<name> HEAD`); workflow
  isolation bases on `main`, which is far behind.
- Socket paths over 108 bytes fail: tests use `mktemp -d /tmp/nm.XXXX`.
- `src/daemon/session.rs` is ~2.7k lines: add commands to the existing
  `handle` match (`session.rs:2235`), no new dispatch.
- Tests that pick STT must set `runtime_settings.speech_provider =
  "deepgram-stt"` (the whisper-local default needs a server).
- `notes_fts` lives in the SQLCipher database; prove FTS5 there in a test,
  not only in plain SQLite.

## Steps

1. **Mode enum.** `src/core/models/settings.rs:195,233,335,423`:
   `meeting_mode: MeetingMode` (`General | Interview | TeamMeeting`,
   serde snake_case, `as_str`, unknown → General). Replace the checks at
   `src/ai/recap.rs:205`, `src/daemon/session.rs:599`, `:1968`, `:2023`
   with `match`. Add `RECAP_PROMPT_TEAM` (decisions, actions with
   owner/due only if spoken, open questions, earlier items touched; no
   invented content) and the `recap_prompt_team` key; `prompts` and
   `prompts_reset` accept mode `team_meeting` (answer → seeded "Meeting").
   `completed_turn` (`session.rs:1048`) is never eligible in team mode.
   → verify by unit tests: settings round-trip for all three values and an
   unknown one; prompts get/reset for team_meeting; no pending question in
   team mode; `devenv shell -- cargo test --workspace`.
2. **Schema and repos.** `schema.sql`: the tables above. `src/db/mod.rs:68`:
   a guarded `ALTER TABLE meetings ADD COLUMN series_id TEXT`. New
   `src/db/repos/items.rs`: series create/list/get, items
   insert/list(meeting|series, status)/update/delete, `open_for_series`,
   `previous_meeting(series, before)`. `meetings::start`
   (`meetings.rs:78`) takes `series_id: Option<&str>`; `meetings::search`
   (`meetings.rs:184`) adds `OR m.id IN (SELECT meeting_id FROM notes_fts
   WHERE notes_fts MATCH ?3)` with the escaped query; add
   `meetings::index_notes(id, body)`.
   → verify by unit tests: the ALTER on an old database; FTS5 works on an
   encrypted `Database`; search hits recap text and survives `"`, `%_`,
   `-`; items CRUD and the status CHECK.
3. **Extraction.** `src/ai/recap.rs`: in team mode `summarise_slice`
   (`:62`) uses `SLICE_PROMPT_TEAM` with the open items appended (≤ 4k
   characters, as untrusted data), and `parse_team_chunk(text) ->
   (markdown, Vec<ExtractedItem>)` splits off the last fenced json block.
   `chunk_pending` (`:142`) stores items (`draft=1`, source_line mapped to
   the slice's rowids, unknown `continues_item` ids dropped) and returns
   the continuations. `generate` (`:193`): team mode merges duplicates,
   sets `draft=0`, and passes the earlier items touched into the recap
   prompt; every mode writes the recap into `notes_fts`.
   → verify by unit tests: valid / invalid / missing JSON; null owner kept
   null; a bogus `continues_item` dropped; reconcile merges two similar
   actions; `tests/recap_rolling.rs` still green.
4. **Daemon.** `session.rs`: `session.new{series?}` stores the series for
   the next `record_start` (`:1361` passes it to `meetings::start`);
   `series.list`, `series.create`, `items.list`, `items.update`,
   `items.delete`, `last_time.get`. After `maybe_chunk` (`:1259`), emit
   `items.updated{meeting}` and, for continuations, `earlier_item{item,
   quote, line}` rate-limited to one per 180 s (an `Instant` in `Inner`).
   `state.get` reports `series`.
   → verify by unit tests (rate limit, unknown series refused, last_time
   with no prior meeting returns empty) and a `tests/daemon_flow.rs` case:
   meeting 1 in a series creates an action; meeting 2's fake transcript
   continues it → `earlier_item` fires → `items.update{status:"done"}`
   sticks.
5. **MCP.** `src/mcp/mod.rs`: `meeting_items` and `series_list` (read-only
   annotations, text summary + JSON). Update the tool count in
   `tests/mcp_flow.rs` and the README to 14 and add one call of each.
   → verify by `devenv shell -- cargo test --test mcp_flow`.
6. **Shell.** `Overlay.qml:262`: a third header button "Team".
   `Client.qml`: `items`, `series`, `earlierItem` properties fed by
   `items.updated` / `earlier_item` / `state`. `Composer.qml`: a series
   dropdown (with "New series…") shown only in team mode. `Overlay.qml`:
   in team mode the NOW panel is replaced by an **Items** panel (grouped
   action / decision / question, monochrome kind glyph, source time, status
   buttons, inline edit), a collapsible **Last time** drawer, and the
   **Earlier task?** callout (item, date, quote, three buttons). Same
   spacing, radius, `ts` scaling and theme colors as the existing cards.
   `shell/tests/fake-daemon.py --team` and `live-shots.sh SCENARIO=team`.
   → verify by `bash shell/tests/lint.sh`, `smoke.sh`, `smoke-real.sh`,
   `python3 shell/tests/contrast.py`, and `SCENARIO=team` crops at 100 %
   and 150 % scale.
7. **Docs.** README: a "Team meetings" section (series, items, Last time,
   Earlier task?, search) and the two new MCP tools.
   → verify by reading it back.
8. **Review** (session model): a fresh Opus review with this path and
   `git diff`, then Codex and agy per `reviewers:`; fix confirmed findings.

## Tests

- `devenv shell -- cargo test --workspace --no-fail-fast` is fully green.
- `devenv shell -- cargo clippy --workspace --all-targets -- -D warnings`
  is clean; `rustfmt --check` on the touched files.
- `bash shell/tests/lint.sh`, `smoke.sh`, `smoke-real.sh`, `contrast.py`
  pass.
- `git add -A && nix flake check` passes.

## Rollback

Each step is its own commit on `refactor/omarchy-quickshell-rewrite`;
`git revert` any of them. The new tables and the `series_id` column are
additive and unused by older code, so a revert needs no data migration.

## Implementation split

Steps 1–4 touch `session.rs`/`recap.rs` in sequence and go to one `coder`
(Sonnet) step by step. Step 6 (QML) runs in parallel in its own worktree
against the protocol above, with the fake daemon; step 5 and 7 follow
step 4. The session model merges, commits each step, and runs step 8.

## Deviations log (recorded with the implementation commit)

None yet.
