---
status: draft
issue: 60
intent: intent/2026-11-03-60-grounded-answers-transcript.md
phase: B (intent outcomes 6–7)
---

# Spec: Line actions and the post-meeting view (phase B)

Phase B of #60 covers intent outcomes 6 and 7: from a transcript line the
user can correct it, bookmark it or ask about it, and after a meeting one
view shows the recap, items, bookmarks and transcript, each linked to the
moment it came from. Phase A (outcomes 1–5) shipped in #63.

The approved intent's constraints all apply. These are the ones that shape
this design:
- corrections keep the original;
- no silent rewrites;
- stable line identity (the line UUID);
- MCP read parity only;
- local-only interviews stay local-only.

## Design

### B1. Correct a line (outcome 6)

**Storage.** An additive migration in `src/db/mod.rs` adds two nullable
columns to `meeting_lines`:
- `heard TEXT`: what speech-to-text produced, set only on the first
  correction;
- `edited_at TEXT`: a millisecond timestamp, in the same format as
  `meeting_speakers.updated_at`.

`text` holds the corrected words. So search, the answer prompt, notes,
recap and export use the correction with no change to their readers, and
`heard` keeps the original for audits. If a correction sets `text` back to
`heard`, both columns are cleared.

**Command.** `transcript.edit {id, text}`, with these rules:
- The text is trimmed and must not be empty; deleting a line is out of scope.
- It keeps the 32k-character limit the live path applies.
- An unknown id returns "That line no longer exists."
- It runs one `UPDATE` (`meetings::edit_line`). It then patches the
  matching `g.transcript` entry by id and emits
  `transcript.edited {id, text, edited: bool}`. This follows the
  `relabel_room` pattern (`session.rs` ~3040) and `transcript.relabel` in
  `shell/Client.qml`.
- It is refused while a line is still partial, because only stored lines
  have an id.
- Editing during a live call is allowed. Nothing is regenerated
  automatically.

**Out of date.** The rename rule from phase A gets one more input:
- `meetings::renamed_at` becomes `meetings::changed_at`, which returns the
  later of `MAX(meeting_speakers.updated_at)` and
  `MAX(meeting_lines.edited_at)`.
- `notes_get` keeps its `>=` comparison and its `stale` field.
- The shell caption becomes "Names or lines changed after these notes were
  written".
- An edit marks archived answers out of date the same way a rename does
  (`renamedAfter`, renamed to `changedAfter`).

**Regenerate reaches the correction.** `recap::notes` reuses saved chunks,
so today a correction to a line the chunks already cover would never reach
a regenerated recap. When `changed_at` is newer than the oldest chunk,
`notes.regenerate` deletes that meeting's chunks first and rebuilds them
from the lines. This uses the existing `finish_meeting_artifacts` path,
with no new prompt. Regenerating a meeting that is still recording stays
refused, as it is today.

The shell gains the **Regenerate** button that phase A lacked. It sits next
to the stale caption in the meeting view (B4), and nowhere else.

**Shell.** A transcript row gets a "⋯" toggle (the existing inline pattern
from `Library.qml` `menuFor`; the codebase has no context menus). The toggle
shows three `IconButton`s: Correct, Bookmark, Ask.
- **Correct** swaps the bubble text for the inline `Input`, the same one the
  speaker rename uses.
- An edited row shows a small "edited" mark. Its tooltip shows `heard`,
  which `transcript.get` returns as `heard` when it is set.
- Answer cards, partial rows and rows without an id get no toggle.

### B2. Bookmark a line (outcomes 6, 7)

**Storage.** A new table:

```sql
CREATE TABLE IF NOT EXISTS line_bookmarks (
  line_id TEXT PRIMARY KEY REFERENCES meeting_lines(id) ON DELETE CASCADE,
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

It is keyed by the line UUID, not the rowid that `meeting_items.source_line`
uses: the intent makes the UUID the stable identity. Deleting a meeting or a
line removes its bookmarks. A bookmark has no label; the line is the label.

**Commands.**
- `bookmark.set {line, on}` is user-only.
- `bookmarks.list {meeting}` returns
  `[{line, who, speaker, speaker_key, text, t0}]` in time order, with speaker
  names resolved as in `transcript.get`.

`transcript.get` and `transcript.final` rows gain `bookmarked: bool`, so a
row shows its star after reload and restore.

**Shell.** The Bookmark button toggles a star on the row. While a meeting
records, bookmarking a stored line works like any other.

### B3. Ask about a line (outcome 6)

`chat.send` gains `line: Option<String>`, a stored line id.

**Daemon (`begin_send`).**
- It resolves the line and its meeting. An unknown id returns the B1 error.
- In the `cite_window` line list the quoted line always gets label `[1]`,
  even when it is older than the last 20 live lines or belongs to a past
  meeting. If it is also in the last 20, it is not listed twice. So the
  answer can cite it, and its "Based on" chip jumps to it.
- The prompt adds one sentence above the user's question: "The user is
  asking about line [1]."
- **Local-only.** The turn picks its provider chain with
  `local_for(c, Some(line_meeting))`. A question about a local-only
  interview's line therefore runs on a local model even from an ordinary
  session. Without a local model it fails with the existing `local_only:`
  error.
- **Interviewer guard.** The no-assessment guard applies when the session is
  guarded or the line's meeting has a candidate. It is the same check
  `guarded` uses for the session.

**Shell.** Ask fills the input bar with a quote chip (speaker, m:ss, the
first 80 characters) and focuses it. The user types the question; Enter
sends `chat.send {text, line}`. The chip has an × to drop it. If the input
is empty, Enter sends "What does this mean, and what should I say?".

### B4. The meeting view (outcome 7)

The Library notes pane for a selected past meeting (`Library.qml` ~563–627)
becomes the meeting view. It has no new window and no new tab. Sections,
top to bottom:

1. **Header.** Title, date, the stale caption and **Regenerate** (B1).
2. **Recap.** The recap Markdown, as today.
3. **Timeline.** One entry per rolling notes chunk: `m:ss–m:ss` and the
   chunk body. The time is a link to the first line at or after the chunk's
   `t0`, the same mapping `cite_window` uses for notes sources. To support
   it, `notes.get` gains `chunks: [{t0, t1, body, line}]`. The recap is one
   consolidated text with no line references, so the timeline is how the
   recap links to its moments.
4. **Items.** Action items, decisions and questions, from
   `items.list {meeting}`.
   - `items.list` rows gain `line`: the UUID of `source_line`, looked up by
     rowid in the same query that already derives `t0`.
   - The m:ss on an item (shown today in `ItemsPanel.qml` but not clickable)
     becomes a link.
   - Items without a source line show no link.
5. **Bookmarks.** From `bookmarks.list`, each one as m:ss, speaker and text,
   linked.
6. **Transcript.** An "Open transcript" button restores the meeting, as the
   Restore button does today.

Every link calls the existing `Client.openLine(id)`, which uses
`transcript.get around` and `viewingPast`. A link therefore lands on the
line, flashed, exactly as a search hit or a citation chip does.

### B5. Agents (MCP read parity)

- `bookmarks.list` joins both the `--agent` and MCP allowlists
  (`session.rs` ~4671, ~4690). It also joins the local-only `matches!` guard
  (~4730), which resolves its `meeting` argument. MCP gains a
  `meeting_bookmarks` tool in `src/mcp/mod.rs`.
- `notes.get` chunks and the new `line` field on `items.list` are covered by
  those commands' existing guards.
- `transcript.edit` and `bookmark.set` stay off both allowlists.
- From MCP, `chat.send` with `line` is refused like `attachments`. Agents can
  quote text in `text`, but not reach a line in another meeting by id.

### B6. Tests and fakes

`shell/tests/fake-daemon.py` answers the new commands and events. The smoke
test checks these flows:
- correct a line and see the mark;
- bookmark a line and see the star survive a reload;
- Ask sends `line`;
- in the meeting view, each section's link opens the transcript at the line;
- Regenerate appears only when the notes are stale.

## Alternatives rejected

- **A separate corrections table.** Every reader (search, prompt, notes,
  export) would need a join to show the corrected text. Two nullable
  columns keep the readers unchanged and still keep the original.
- **Showing `heard` in exports.** The export is for reading. The original
  stays in the database for audit; adding it to `notes.md` doubles every
  corrected line.
- **Regenerating notes automatically after a correction.** The intent rules
  it out (open question 3).
- **Bookmark labels or notes.** Nobody asked for them. The line is the label.
  They can be added later as a nullable column.
- **Keying bookmarks by rowid, like items.** Phase A chose the UUID as line
  identity. Moving items to the UUID too is a separate cleanup, because
  their join already works.
- **Persisting answer sources in `messages`** (deferred to phase B by spec
  A). Outcome 7 lists the recap, items, bookmarks and transcript, not
  answers. Persisting sources would add a column and a restore path for
  "Based on" chips under answers that are older than a restore. Left for a
  follow-up issue if wanted.
- **A new post-meeting window.** The Library pane already shows one meeting's
  notes. Extending it reuses navigation, `openLine` and the theme.
- **A right-click context menu.** There is none in the shell. The inline "⋯"
  toggle is the existing pattern and works with the keyboard.
- **Bookmarks and corrections in the export.** Not part of outcome 7; the
  export already carries the corrected text. Left out.

## Risks

- **Regenerate cost.** Deleting chunks means a long meeting is re-summarised
  from scratch, with one model call per chunk. That happens only when the
  user presses Regenerate, and only if something changed since the chunks
  were written.
- **Migration.** Two nullable columns and one new table, all additive. Older
  builds ignore them. A rolled-back daemon still opens the database. Corrected
  text stays corrected, because `text` holds it.
- **Ask about an old line from a cloud session.** The B3 local-only rule is
  the only thing keeping an interview line off a cloud model. A daemon test
  covers a local-only line asked about from an ordinary session.
- **Label shift.** Putting the quoted line at `[1]` renumbers the live
  lines for that turn only. Labels are mapped per turn through `g.cite`, so
  chips still resolve correctly.
- **Stale caption wording.** "Names or lines" is slightly longer. The
  caption already wraps.

## Verification

- `cargo test --workspace`, with new tests for:
  - **B1:**
    - an edit keeps `heard`, and a second edit keeps the first `heard`;
    - an edit back to the original clears both columns;
    - empty text and unknown ids are refused;
    - `changed_at` makes the notes stale;
    - `notes.regenerate` after an edit rebuilds the chunks, and the edited
      words reach the recap;
    - the migration on a database without the columns.
  - **B2:**
    - set and unset;
    - cascade on line and meeting delete;
    - `bookmarked` in `transcript.get`;
    - `bookmarks.list` order and names.
  - **B3:**
    - the quoted line is labelled `[1]` and is citable, including an old
      line and a past-meeting line, with no duplicate;
    - a line from a local-only interview uses the local chain from an
      ordinary session;
    - the interviewer guard applies;
    - MCP `chat.send` with `line` is refused.
  - **B4:** `notes.get` chunks carry `line`; `items.list` carries `line`.
  - **B5:** `bookmarks.list` from MCP is refused for a local-only interview,
    and `transcript.edit` and `bookmark.set` are refused from MCP and
    `--agent`.
- `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --all -- --check`, `bash shell/tests/ci.sh`,
  `nix flake check -L --no-build` and
  `nix build .#checks.x86_64-linux.shell -L`.
- Manual run against the real daemon:
  - correct a misheard name mid-call; the next answer uses the correction;
    the notes are marked out of date, and Regenerate fixes them;
  - bookmark two moments, then open last week's meeting: the bookmarks,
    items and timeline each jump to their line;
  - Ask about a line from 20 minutes earlier; the answer cites it and its
    chip jumps to it.
