---
status: draft
issue: 60
intent: intent/2026-11-03-60-grounded-answers-transcript.md
phase: B (intent outcomes 6–7)
---

# Spec: Line actions and the post-meeting view (phase B)

Phase B of #60 covers intent outcomes 6 and 7:
- from a transcript line, the user can correct it, bookmark it or ask about
  it;
- after a meeting, one view shows the recap, items, bookmarks and
  transcript, each linked to the moment it came from.

Phase A (outcomes 1–5) shipped in #63. Every constraint in the approved
intent applies. The ones that shape this design:
- corrections keep the original;
- no silent rewrites;
- the line UUID is the stable identity;
- MCP has read parity only;
- local-only interviews stay local-only.

Revision 2 adds the confirmed findings of the Codex and agy reviews. They are
listed at the end.

## Design

### B1. Correct a line (outcome 6)

**Storage.** Two nullable columns on `meeting_lines`, added both in
`src/db/schema/schema.sql` (fresh databases) and as a guarded migration in
`src/db/mod.rs` (existing ones):
- `heard TEXT`: what speech-to-text produced. It is set only by the first
  correction (`heard = COALESCE(heard, text)`).
- `edited_at TEXT`: a millisecond timestamp
  (`strftime('%Y-%m-%d %H:%M:%f','now')`), set by every correction.

`text` holds the corrected words. Search, the prompt, notes, recap and export
read the correction with no change to their readers. If a correction
restores the heard text, `heard` is cleared but `edited_at` is kept, so any
notes written from the intermediate wording still show as out of date.

**Command.** `transcript.edit {id, text}`.
- The text is trimmed. It must not be empty or longer than 32 000
  characters, a new explicit limit; the live path only shortens the
  displayed copy. Deleting a line is out of scope.
- An unknown id returns "That line no longer exists."
- One `UPDATE` (`meetings::edit_line`) changes the row.
- Under the same lock the daemon patches:
  - the matching `g.transcript` entry by id;
  - the `restored-meeting:<id>` source in `g.sources`, rebuilt by the same
    helper `session_restore` uses, when the line belongs to the restored
    meeting.

  Answers, citations and follow-ups in the open session therefore see the
  correction.
- It emits `transcript.edited {id, text, edited}`. This follows the
  `relabel_room` / `transcript.relabel` pattern (`session.rs` ~3040,
  `shell/Client.qml` ~332).
- It works during a live call. Nothing is regenerated automatically.

**Out of date, without races.** Notes record when they *read* their inputs,
not when they were written:
- `meeting_note_chunks` and `meeting_recaps` gain a nullable millisecond
  column, `read_at`. It is set from the moment the chunk's lines, or the
  recap's chunks and lines, were read, before any model call.
- `meetings::renamed_at` becomes `changed_at`: the later of the speaker
  renames' `updated_at` and the lines' `edited_at`.
- `notes_get`: `stale = changed_at >= read_at`. A row written before this
  change has no `read_at`, so it falls back to `created_at` / `updated_at`,
  as today.

This closes two gaps:
- An edit made while a chunk or recap is being written is newer than its
  `read_at`, so the result shows as stale.
- Both sides are compared at millisecond precision. Today a millisecond
  rename compared against a second-precision `datetime('now')` can stay
  stale after a regeneration in the same second.

The caption becomes "Names or lines changed after these notes were written".
Archived answers are marked by an edit the same way as by a rename
(`renamedAfter`, renamed to `changedAfter`).

**Regenerate reaches the correction.** `recap::notes` reuses saved chunk
bodies, so a correction under an existing chunk never reaches the recap.
`notes.regenerate` therefore first **re-summarises in place** every chunk
that covers a line edited after that chunk's `read_at`. A chunk covers the
lines after the previous chunk's `last_line`, up to its own.
- Only the body is replaced. Team chunks use the team prompt, and the items
  it extracts are discarded. Items keep their text, status and user edits;
  they are never re-extracted or duplicated. Items are user-editable if
  their wording is now off.
- A chunk's body and `read_at` are written only when its model call
  succeeds. A failed call keeps the old chunk and reports the error. The
  notes then stay stale.
- After that, the existing recap path runs unchanged.
- Regenerate does **not** re-title the meeting. Today `notes.regenerate`
  re-titles any meeting not in the in-memory `named_meeting`, so it would
  overwrite a title. Titling stays at meeting end only.
- Regenerating a meeting that is still recording stays refused, as today.

The shell gains the **Regenerate** button that phase A lacked. It sits next
to the stale caption in the meeting view (B4).

**Shell.** Each transcript row gets a "⋯" toggle with three `IconButton`s:
Correct, Bookmark, Ask. This is the inline pattern from `Library.qml`
`menuFor`; the shell has no context menus.
- **Correct** swaps the bubble text for the inline `Input` the speaker rename
  uses. It is hidden on rows shown shortened (over 32k), because the shell
  does not hold their full text.
- An edited row shows a small "edited" mark. Its tooltip shows `heard`
  (`transcript.get` and `transcript.edited` return it when set).
- Answer cards, partial rows and rows without an id get no toggle.

### B2. Bookmark a line (outcomes 6, 7)

**Storage.** A new table, in `schema.sql` and created by the migration:

```sql
CREATE TABLE IF NOT EXISTS line_bookmarks (
  line_id TEXT PRIMARY KEY REFERENCES meeting_lines(id) ON DELETE CASCADE,
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

It is keyed by the line UUID, unlike `meeting_items.source_line` (a rowid).
A bookmark has no label; the line is the label. Deleting a line or meeting
removes its bookmarks. Candidate deletion (`candidates::delete_all`) cascades
through the meeting.

**Commands.**
- `bookmark.set {line, on}` is user-only.
- `bookmarks.list {meeting}` returns
  `[{line, who, speaker, speaker_key, text, t0}]` in time order, with names
  resolved as in `transcript.get`.

Rows from `transcript.get`, `transcript.final` and restore gain
`bookmarked: bool`.

**Shell.** Bookmark toggles a star on the row. It works on stored lines
during a recording too.

### B3. Ask about a line (outcome 6)

`chat.send` gains `line: Option<String>`, a stored line id.

**Which lines.** A line can be asked about if it belongs to:
- the open session's meeting (live, finishing or restored), or
- a past meeting that has **no candidate**.

A line from another interview is refused with "Open that interview to ask
about its lines." So the turn always runs under the open session's own
rules:
- its provider chain (local-only if the session is);
- its interviewer guard, with the guard's `said` evidence from the open
  meeting;
- its conversation history.

An interview's line therefore never enters an ordinary chat that a later
turn could send to a cloud model. It is also never checked against another
meeting's candidate speech.

**Daemon (`begin_send`).**
- It resolves the line; an unknown id gets the B1 error.
- In the `cite_window` line list, the quoted line always gets label `[1]`,
  even when it is older than the last 20 live lines or from a past meeting.
  It is never listed twice. The answer can cite it, and its chip jumps to it.
- The prompt adds "The user is asking about line [1]." above the question.

**Shell.** Ask fills the input bar with a quote chip (speaker, m:ss, the
first 80 characters) and focuses it.
- Enter sends `chat.send {text, line}`.
- The chip has an × to drop it.
- If the input is empty, Enter sends "What does this mean, and what should I
  say?".

### B4. The meeting view (outcome 7)

The Library notes pane for a selected past meeting (`Library.qml` ~563–627)
becomes the meeting view. It is not a new window or tab. Sections:

1. **Header.** Title, date, the stale caption, and **Regenerate** (B1).
2. **Recap.** The recap Markdown, as today.
3. **Timeline.** One entry per notes chunk: `m:ss–m:ss` and the chunk body.
   - Each time links to the first line at or after the chunk's `t0`, the
     mapping `cite_window` already uses. `notes.get` gains
     `chunks: [{t0, t1, body, line}]`.
   - A short meeting has no chunks, because `recap::notes` summarises it in
     one pass. Its timeline is then one entry, the whole meeting, linked to
     its first line.
   - The recap itself is one consolidated text with no line references, so
     the timeline is how it links to its moments.
4. **Items.** From `items.list {meeting}`.
   - Rows gain `line`: the UUID for `source_line`, looked up by rowid in the
     same query that already derives `t0`.
   - The m:ss becomes a link. Items without a source line show none.
5. **Bookmarks.** From `bookmarks.list`, each one as m:ss, speaker and text,
   linked.
6. **Transcript.** "Open transcript" restores the meeting, as the Restore
   button does today.

Every link goes through the same path as a search hit (`Library.qml`
`openHit`): it restores that meeting, then jumps to the line with
`pendingJump`. The transcript therefore never shows one meeting's lines under
another meeting's session.

### B5. Answer sources survive a restore

Phase A kept "Based on" sources in memory only. A restored chat replays
answers without `cite_max`, so their `[k]` markers show raw and there are no
chips. Spec A deferred this to phase B.
- `messages` gains a nullable `sources TEXT` column (schema and migration):
  JSON `{sources, cite_max, cite_notes}`.
- `finish_turn` writes it with the assistant message, when there are any
  sources.
- `restore_chat` adds those fields to each `history` row. `Client.qml`
  passes them to `addRow`, as live answers already do.
- Older messages have no sources and render as today.

### B6. Agents (MCP read parity)

- `bookmarks.list` joins:
  - the `--agent` allowlist (`session.rs` ~4671);
  - the MCP allowlist (~4690);
  - the local-only guard (~4730), which resolves its `meeting`.

  MCP gains a `meeting_bookmarks` tool (`src/mcp/mod.rs`).
- Chunks in `notes.get`, `line` in `items.list` and `sources` in `history`
  are covered by their commands' existing guards. `history` is not sent to
  agents.
- `transcript.edit` and `bookmark.set` stay off both allowlists.
- From MCP, `chat.send` with `line` is refused, like `attachments`.

### B7. Tests and fakes

`shell/tests/fake-daemon.py` answers the new commands and events. The smoke
test checks:
- correct a line and see the mark;
- bookmark a line and see the star survive a reload;
- Ask sends `line`;
- each meeting-view link restores the right meeting and lands on the line;
- Regenerate appears only when the notes are stale;
- a restored answer shows its chips.

## Alternatives rejected

- **A separate corrections table.** Every reader would need a join. Two
  columns keep the readers unchanged and still keep the original.
- **Rebuilding all chunks on Regenerate.** It re-extracts items, which
  duplicates them and drops user edits, and a failed model call loses the
  notes. In-place body re-summaries avoid both.
- **Regenerating automatically after a correction.** Ruled out by the intent
  (open question 3).
- **Asking about any line from any session, with the chain chosen per line.**
  The answer would stay in the current chat's history and could reach a
  cloud model on the next turn. The interviewer guard would also check
  against the wrong meeting. Opening the interview first is simpler and
  closes both.
- **Showing `heard` in exports.** The export is for reading; the original
  stays in the database for audit.
- **Bookmark labels.** Nobody asked for them. They can be added later as a
  nullable column.
- **Moving items to UUID line keys.** A separate cleanup. Their rowid join
  works, and B4 returns the UUID alongside.
- **A new post-meeting window, or a right-click menu.** The Library pane and
  the inline "⋯" toggle are the existing patterns.
- **Bookmarks in the export.** Not part of outcome 7.

## Risks

- **Regenerate cost.** One model call per chunk that covers an edit, and
  only when the user presses Regenerate.
- **Migration.** It adds:
  - nullable columns on `meeting_lines`, `meeting_note_chunks`,
    `meeting_recaps` and `messages`;
  - one new table.

  All of it is additive, and older builds ignore it. After a rollback,
  corrected text stays corrected, because `text` holds it.
- **Label shift.** The quoted line takes `[1]` for that turn only. Labels map
  through `g.cite` per turn.
- **Item wording.** After a correction, an item can still quote the old
  words until the user edits it. This is shown as-is, not silently changed.

## Verification

- `cargo test --workspace`, with new tests for:
  - **B1:**
    - a second edit keeps the first `heard`;
    - reverting clears `heard` but keeps the notes stale;
    - empty and over-limit text are refused;
    - unknown ids are refused;
    - the edit reaches `g.transcript` and the restored-meeting source;
    - an edit during an in-flight chunk leaves the notes stale (race test);
    - a regeneration in the same second as a rename is not stuck stale;
    - Regenerate re-summarises only the affected chunks, adds no items and
      keeps item edits;
    - a failed re-summary keeps the old chunk;
    - Regenerate keeps the title;
    - the migration runs on a pre-phase-B database, and a fresh database
      opens.
  - **B2:** set and unset; cascade on line, meeting and candidate delete;
    `bookmarked` on rows; `bookmarks.list` order and names.
  - **B3:**
    - `[1]` is citable for an old line and a past-meeting line, with no
      duplicate;
    - another interview's line is refused;
    - a line of the open interview runs under its guard and chain;
    - MCP `chat.send` with `line` is refused.
  - **B4:** `notes.get` chunks carry `line`; a short meeting gives one
    entry; `items.list` carries `line`.
  - **B5:** sources are saved, restored, and absent for old rows.
  - **B6:**
    - `bookmarks.list` from MCP is refused for a local-only interview;
    - `transcript.edit` and `bookmark.set` are refused from MCP and
      `--agent`.
- Then run:
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo fmt --all -- --check`
  - `bash shell/tests/ci.sh`
  - `nix flake check -L --no-build`
  - `nix build .#checks.x86_64-linux.shell -L`
- Manual run against the real daemon:
  - correct a misheard name mid-call; the next answer uses it, the notes
    show as out of date, and Regenerate fixes them;
  - bookmark two moments, then open last week's meeting from the Library;
    its bookmarks, items and timeline each land on their line;
  - Ask about a line from 20 minutes earlier; the answer cites it and the
    chip jumps;
  - restore that meeting; the answer keeps its chips.

## Review record (revision 2)

| Finding | From | Verdict |
|---|---|---|
| A local-only line asked from an ordinary chat stays in its history and can reach a cloud model on the next turn | Codex | confirmed → B3 limits lines to the open meeting or meetings with no candidate |
| Interviewer guard would use the open meeting's `said`, not the line's | Codex | confirmed → same B3 limit |
| Deleting chunks on Regenerate duplicates items, drops edits, and loses notes on failure | Codex, agy | confirmed → re-summarise bodies in place; items untouched; keep on failure |
| Edit during an in-flight chunk looks current; second vs millisecond stamps can stay stale | Codex, agy | confirmed → `read_at` (ms) snapshot on chunks and recaps |
| Library links would show past lines under the current session | Codex | confirmed → links use the `openHit` restore-then-jump path |
| Short meetings have no chunks, so no recap links | Codex | confirmed → one whole-meeting timeline entry |
| Reverting to heard text would clear the stale signal | Codex | confirmed → keep `edited_at` on revert |
| The "32k live limit" is display-only | Codex | confirmed → explicit 32 000-character correction limit |
| Columns and table need `schema.sql` as well as the migration | Codex | confirmed → both, with a fresh-database test |
| Edits must reach `g.transcript` and the `restored-meeting` source | agy | confirmed → both patched under the lock |
| Shortened rows would save the shortened text | agy | confirmed → Correct hidden on shortened rows |
| Regenerate re-titles meetings (`named_meeting` is in memory only) | agy | confirmed → Regenerate keeps the title |
| Restored answers lose sources and show raw markers | agy | confirmed → B5 persists sources |
| Items return a rowid, not a UUID | agy | already in revision 1 (B4 `line`) |

agy's run did not see the spec file: the branch was switched while it ran.
Its findings came from the code, and each was checked against the code above.
