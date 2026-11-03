---
status: draft
issue: 66
spec: spec/2026-11-03-66-library-quick-wins.md
coders:
  1: coder   # test plumbing: fake-daemon flags, a new harness and smoke phase, following existing helpers
  2: coder   # two JSON fields and one Rust test; same coder session, warm cache
  3: codex   # Library.qml: focus/selection state, late-reply guard, capHeadings parser, layout math — tricky logic
reviewers:
  - codex    # independent read of the diff against this plan
  - agy      # second read; it caught the fence and accName gaps in the spec
---

# Plan: A Library session list you can scan, with notes that say whose they are

## Decisions carried over (approved spec)

**Daemon.** In `src/daemon/session.rs` `session_list` (~3515), meeting
rows add `"duration_seconds": r.duration_seconds` and
`"line_count": r.line_count`.
- Both fields are already on `MeetingRecord` (`meetings.rs:58-67`).
- Chat rows are unchanged.
- The agent's local-only filter (`:3518-3521`) is unchanged.

**Library state:**

| Property | Meaning |
|---|---|
| `selectedId` (string) | The selected session id. |
| `notesPhase` (string) | `""`, `loading`, `ready`, `empty` or `error`. |
| `menuFor` (string) | The row whose "⋯" line is open. |
| `linesDoneFor` (string) | The trimmed query whose `transcript.search` reply has arrived. |
| `rowH` (readonly) | `theme.ctl + theme.caption * 1.4 + Style.spacing.xs`, the height of a two-line meeting row. |

**`select(s)`:**
1. Set `selectedId = s.id` and `menuFor = ""`, and clear `notes` and
   `notesStale`.
2. For a meeting, set `notesPhase = "loading"` and send `notes.get`. The
   reply is applied only if `selectedId` is still that id, and then all
   of `notes`, `notesStale` and `notesPhase` are assigned inside the
   guard: `empty` for empty text, `error` with the message, otherwise
   `ready`.
3. For a chat, set `notesPhase = ""`.

`Connections { target: client; onNotesUpdated }` re-runs `select` when
the updated meeting is the selected one. `notesMeeting` stays as it is
for Export, and the notes pane no longer reads it.

**The row delegate** (replaces `RowLayout` `:309-352`). An `Item`
containing a `Column`.
- **The row item:**
  - `activeFocusOnTab`;
  - `Accessible.role: Accessible.Button`, with the name
    `"Meeting: <title>, <date>"` or `"Chat: …"`;
  - `readonly property string accName: Accessible.name`;
  - `signal clicked()`. Return, Enter, Space, `Accessible.onPressAction`
    and a `MouseArea` over the kind and title cells emit it, and its
    handler calls `select`;
  - `objectName` is `openNotes` on meeting rows and `openChat` on chat
    rows.
  - **Selected:** an accent `Rectangle` at low opacity behind the row,
    and an accent left bar 3 px wide (`selectedBar`). The focus ring is
    an accent outline.
- **Line 1** is its own `RowLayout`:
  - the kind caption, with `Layout.preferredWidth` from a `TextMetrics`
    of "Meeting";
  - the title, filling the width and elided;
  - `sessionWhen`, with `Layout.preferredWidth` and
    `Layout.minimumWidth` from a `TextMetrics` of the widest sample date;
  - `restore`;
  - `rowMore` ("⋯", named "More actions"), which toggles `menuFor`.
- **Line 2, meetings only:** a dim caption, `N min · M lines`.
  - Under a minute it reads `< 1 min`.
  - With duration 0 the minutes are left out.
  - With both 0 the line is hidden.
- **Line 3, when `menuFor === id`:** right-aligned `exportNow` (meetings
  only) and `deleteSession`. The confirmation is kept. On confirm, if the
  deleted row is the selected one, `selectedId`, `notes` and `notesPhase`
  are cleared too.

**Layout:**
- `notesMax` (`:43`) reserves `2 * root.rowH`.
- The `restoreList` height (`:306`) also subtracts `noResults` when it is
  visible.

**The notes pane** (`notesBox`, `:449`). It is visible when
`tab === "restore" && notesPhase !== ""`.
- **Header** (`notesTitle`): the selected meeting's title (bold, body
  size) and its date (dim caption), then `notesStale`.
- **Body:**
  - when the phase is not `ready`: `notesStateText`, reading "Loading
    notes…", "No notes yet for this meeting." or "Notes failed: …";
  - when it is `ready`: the recap through `capHeadings`.

**`capHeadings(md)`**, a `Library` function that walks the text line by
line:
- a line matching `^ {0,3}(```|~~~)` toggles "in a fence", and fence
  lines are left alone;
- outside a fence, a line matching
  `^ {0,3}#{1,6}[ \t]+(.*?)[ \t]*#*[ \t]*$` becomes `""`, then
  `"**" + text.replace(/\*\*/g, "") + "**"`, then `""`. The blank lines
  around it make it a separate block.

**Search:**
- The placeholder on Restore is "Search meetings, chats and
  transcripts"; on the other tabs it stays "Search settings, chats,
  profiles".
- `searchLines` sets `lineHits` and then `linesDoneFor = q` in its reply,
  and clears `lineHits` and `linesDoneFor` when the query is under 2
  characters.
- `noResults` (a dim caption under search) shows only on Restore when the
  trimmed query is at least 2 characters, the filtered sessions are
  empty, `linesDoneFor === q`, and `lineHits` is empty.

## Steps

Test first: step 1 adds the checks, which must fail on the current code.

### 1. Tests and fake support (coder)

Files: `shell/tests/fake-daemon.py`, `shell/tests/smoke.sh`, a new
`shell/tests/harness-library.qml`.

1. **`fake-daemon.py`:**
   - The `"session.list"` meeting row (~121) gains `"duration_seconds":
     2520, "line_count": 412`. Add a second meeting row with
     `duration_seconds: 20, line_count: 3`, for the "< 1 min" case.
   - `notes.get` answers `{"text": "# Meeting recap\n## Summary\nfake",
     "stale": false}` for meeting 1, and `{"text": ""}` for the
     second meeting, to give the empty state.
   - New flag `--slow-notes`: `notes.get` answers after 0.8 s. Follow
     `--slow-settings` (`:54`), which uses a `threading.Timer`.
   - Keep the existing modes' data, including the `grounded` and
     `notes.get` branches #63 added.
2. **`smoke.sh` export flow** (~581-625):
   - In phase 13 and in phase 17, click the first `rowMore` before
     `find(card, "exportNow")`.
   - Phase 15's size check uses `lib.rowH` in place of
     `lib.theme.ctl + Style.spacing.xs`.
   - After phase 13's `openNotes.clicked()`, record `notesTitle` text and
     `selectedBar` visibility, and assert them in its Python check.
3. **A new `harness-library.qml` and its `smoke.sh` block**, modelled on
   the `harness-grounded` block (~129). Run it with `--slow-notes`, so the
   loading state is visible. Phases, 500 ms apart; extend the timeout as
   needed:
   1. Open the Library on the Restore tab, then switch to settings and
      back to restore, so the list loads.
   2. Click the first meeting row (`openNotes`). Assert
      `notesStateText` reads "Loading notes…", then after 1 s that
      `notesTitle` holds "Fake standup", that the recap has no `#`, and
      that `selectedBar` is visible.
   3. Click a chat row (`openChat`). The pane is hidden.
   4. Late reply: click meeting 1, then the chat at once, and wait 1.2 s.
      The pane stays hidden.
   5. Click `rowMore` on the chat row: `deleteSession` is present and
      `exportNow` is absent. Record line 1's `sessionWhen` x position
      before and after; they must be equal.
   6. Line 2 of meeting 1 reads `42 min · 412 lines`; the second meeting
      reads `< 1 min · 3 lines`.
   7. Select the second meeting: `notesStateText` reads "No notes yet for
      this meeting."
   8. `capHeadings` fixture, called directly. The input has four parts:
      `# Title`, `  ## Sub`, a ```` ``` ```` fence holding `# kept`, and a
      `~~~` fence holding `# kept2`, followed by `# A **b**`.

      Assert:
      - the heading lines read `**Title**`, `**Sub**` and `**A b**`, each
        with a blank line before and after;
      - both fenced lines are unchanged.
   9. Type a search such as "zzzz" with no hits. After 1 s `noResults`
      is visible, and right after typing (before 300 ms) it is not.
   10. Geometry: `Geo.dump`, as in the grounded harness, and run
       `check-geometry.py`.
4. **Check:** `TMPDIR=/tmp devenv shell -- bash shell/tests/smoke.sh`
   **fails** at the new library block or at the export flow (no
   `rowMore`, no `notesTitle`). Record which.

Traps:
- `find()` needs `visible`.
- `MouseArea.clicked` needs an argument (`clicked(null)`). The row item's
  own `signal clicked()` takes none.
- Never use bare `git stash`.

### 2. Daemon fields (coder)

File: `src/daemon/session.rs`.
- `session_list` (~3515): add the two fields.
- **Test**, in the existing local-only `session.list` test (~7640) or a
  new small test in `command_tests`: a meeting with lines and a duration
  lists `line_count` and `duration_seconds`, and the agent list still
  hides local-only rows.

**Check:** `TMPDIR=/tmp devenv shell -- bash -c 'cargo fmt --check; cargo clippy --workspace --all-targets -q -- -D warnings; cargo test --workspace -q'`.

### 3. Library.qml (codex)

File: `shell/Library.qml` only.

Implement the "Decisions carried over" exactly: the state, `select`, the
`notesUpdated` hook, the row delegate, the `notesMax` and `restoreList`
height changes, the notes pane header and body, `capHeadings`, the search
placeholder, `linesDoneFor` and `noResults`. Keep every other tab, the
grip's mouse and keyboard resize, the export status line and the
transcript-hits box as they are.

**Check:** `TMPDIR=/tmp devenv shell -- bash -c 'bash shell/tests/lint.sh && bash shell/tests/smoke.sh'`.
All checks green, the step 1 checks included.

Traps:
- Escape any user or recap text that goes into `StyledText`. Recaps stay
  `MarkdownText`.
- Theme colours only. `contrast.py` must pass, so raise the selected
  background's opacity if it fails.
- Keep `objectName`s exactly as the spec's table lists them.

### 4. Integrate, review, ship (Claude)

1. Run `bash shell/tests/ci.sh` (contrast, geometry, live shots),
   `nix flake check -L --no-build` and
   `nix build .#checks.x86_64-linux.shell -L`.
2. **Read-only reviews in parallel:**
   - a fresh Opus agent (this plan plus `git diff origin/main`);
   - `codex review --base origin/main -c sandbox_mode="read-only"`;
   - agy `--mode plan`.

   Verify each finding, then fix it or refute it in the PR.
3. Open a PR linking the intent, the spec and this plan, and comment
   `@codex review`. Merge with a merge commit after CI.
4. **Deploy to p620**, after #64 if it is still pending:
   - read the bus and post to it;
   - check that nothing is recording;
   - run `AGENT_BUS_ANNOUNCED=1 bash scripts/update-commit-deploy.sh p620
     nixarchy-meetingai` from `~/.config/nixos`.

   The new QML shows after an Omarchy shell reload. That is the user's
   choice, because it is their session.
5. **Manual:** open the Library and click a meeting. It is highlighted,
   its name is above the notes, the headings are normal size, the date
   does not move when "⋯" opens, and keyboard Tab then Enter selects.

## Tests

- After step 1, `smoke.sh` fails at the new checks.
- After steps 2 and 3, `cargo test`, `lint.sh`, `smoke.sh` and `ci.sh` are
  green, and so is the nix shell check.

## Rollback

- **Before merge:** drop the branch.
- **After merge:** `git revert -m 1 <merge>`, bump the lock again, and
  switch.
- `session.list` only gains fields, so older clients ignore them.

## Deviations

(Recorded here during implementation, in the same commit as the code.)
