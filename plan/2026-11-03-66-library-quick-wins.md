---
status: approved
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
| `selectedId` (string) | The selected session id, always `String(s.id)`: the fake daemon's ids are numbers, so every compare uses `String()`. |
| `notesPhase` (string) | `""`, `loading`, `ready`, `empty` or `error`. |
| `menuFor` (string) | The row whose "⋯" line is open. |
| `linesDoneFor` (string) | The trimmed query whose `transcript.search` reply has arrived. |
| `rowH` (readonly) | `theme.ctl + theme.caption + Style.spacing.xs`, the height of a two-line meeting row (the approved formula). |

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
- `noResults` sits *above* `restoreList` in the `Column`. `restoreList.y`
  therefore already includes it, and the height binding (`:306`) does not
  subtract it again.

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
   - The default `"session.list"` meeting row (~121) gains
     `"duration_seconds": 2520, "line_count": 412`. It keeps exactly one
     meeting and one chat, because `smoke.sh:663` asserts two dates.
   - **The default `transcript.search` answer is `[]`**, not `{}`. Outside
     `--grounded` it fell through to `DATA.get(cmd, {})`.
   - **New flag `--library`**, used only by the library harness:
     - `session.list` adds a second meeting, `{"id": 3, "kind":
       "meeting", "title": "Quick sync", "started_at": "2026-10-29
       08:00:00", "duration_seconds": 20, "line_count": 3}`;
     - `notes.get` for meeting 1 answers `{"text": "# Meeting
       recap\n## Summary\nfake", "stale": false}`, and for meeting 3
       `{"text": "", "stale": false}`;
     - **`notes.get` replies after 0.8 s.** The reply data is chosen
       first, by the same branches as every mode (grounded included),
       then sent through `threading.Timer(0.8, self.send, …)` and
       `continue`. The delay never skips a mode's data;
     - a `notes.bump` command casts `{"event": "notes.updated",
       "meeting": 1}` and switches meeting 1's text to `"# Meeting
       recap\nupdated"`, for the reload check.
2. **`smoke.sh` export flow** (~581-625):
   - In phase 13 and in phase 17, before `find(card, "exportNow")`, click
     the `rowMore` **inside the meeting row**: the `openNotes` item's own
     descendant, not the first `rowMore`. The list is sorted by time and
     the fake chat is newer, so the first "⋯" is the chat's.
   - Phase 15's size check uses `lib.rowH` in place of
     `lib.theme.ctl + Style.spacing.xs`.
   - After phase 13's `openNotes.clicked()`, record `notesTitle` text and
     `selectedBar` visibility, and assert them in its Python check.
3. **A new `harness-library.qml` and its `smoke.sh` block**, modelled on
   the `harness-grounded` block (~129). It runs the fake with `--library`.
   Phases are 500 ms apart, and a wait of N ms spans `ceil(N/500)`
   phases. Give quickshell `timeout 20`. Find each row by its title (the
   `openNotes` or `openChat` item whose accessible name contains it),
   never by position.
   1. Open the Library on the Restore tab, then switch to settings and
      back to restore, so the list loads.
   2. **Geometry first:** `Geo.dump` of the card with no search active.
   3. Click "Fake standup". `notesStateText` reads "Loading notes…".
   4. Two phases later (1 s): `notesTitle` holds "Fake standup", the
      recap has no `#`, and `selectedBar` is visible.
   5. Send `notes.bump` through the client (`client.send`). Two phases
      later the recap shows "updated", so the selected meeting reloaded.
   6. Click the chat row. The pane is hidden.
   7. **Late reply:** click "Fake standup", then the chat row in the same
      phase. Three phases later (1.5 s, longer than the 0.8 s reply) the
      pane is still hidden.
   8. Click `rowMore` in the chat row: `deleteSession` is present and
      `exportNow` is absent. Record the chat row's `sessionWhen` x
      position before and after; they must be equal.
   9. **Metadata:** "Fake standup" line 2 reads `42 min · 412 lines`, and
      "Quick sync" reads `< 1 min · 3 lines`.
   10. Click "Quick sync". In the same phase `notesStateText` reads
       "Loading notes…"; two phases later it reads "No notes yet for this
       meeting."
   11. **`capHeadings` fixture**, called directly on the Library. The
       input has four parts: `# Title`, `  ## Sub`, a ```` ``` ```` fence
       holding `# kept`, and a `~~~` fence holding `# kept2`, followed by
       `# A **b**`.

       Assert:
       - the heading lines read `**Title**`, `**Sub**` and `**A b**`,
         each with a blank line before and after;
       - both fenced lines are unchanged.
   12. **No results:** type "zzzz" in `search`. In the same phase
       `noResults` is not visible, because the transcript search hasn't
       answered. Two phases later it is visible.
   13. Print `LIBRARY {json}` and quit. The `smoke.sh` Python check
       asserts every key, then runs `check-geometry.py` on the dump.
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

## Review record (draft plan)

| Finding | From | Verdict |
|---|---|---|
| "The first `rowMore`" is the chat's (sorted by time, the chat is newer), so the export test would fail | Codex, agy | confirmed (`Library.qml:308`, `fake-daemon.py:121`) → use the meeting row's own `rowMore` |
| `rowH` drifted from the approved formula | Codex | confirmed → the spec's `ctl + caption + spacing` |
| The empty-notes and late-reply checks ignored the 0.8 s delay and the 500 ms phases | Codex, agy | confirmed → explicit phase waits |
| The spec's `notesUpdated` reload check was missing | Codex, agy | confirmed → `notes.bump` plus phase 5 |
| `--slow-notes` must delay after choosing the mode's data | Codex | confirmed → a Timer after the data is chosen |
| Numeric fake ids vs a string `selectedId` | agy | confirmed → `String()` on every compare |
| Outside grounded, `transcript.search` returns `{}`, so `noResults` never shows | agy | confirmed → the default is `[]` |
| A second default meeting breaks `smoke.sh:663` (two dates) | agy (could not verify), Claude | verified → extra data only behind `--library` |
| Subtracting `noResults` from the list height double-counts, because it is above the list | agy | confirmed → dropped |
| Geometry ran while "zzzz" was active | agy | confirmed → geometry first |

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
- Step 3 (Codex): its sandbox could not run `smoke.sh` (no Nix daemon or
  D-Bus), so Claude ran it. The geometry check found line 2 (`42 min ·
  412 lines`) clipped by an explicit `height: theme.caption`; a
  `UiText` line is taller than its pixel size. Claude removed the
  explicit height. `implicitHeight` makes a binding loop, and the
  `Column` already gives a hidden line no space. `rowH` keeps the
  approved formula, so the notes reservation is about 3 px short of a
  real two-line row. That is harmless, and the size check uses `rowH`
  itself.
- Step 4 review fixes (Codex, Opus, agy):
  - **`capHeadings`:** closing `#`s count only after whitespace, so
    `# C#` keeps its `#` (new fixture). A fence closes only on its own
    marker. An empty heading stays as written, because `****` would draw
    a rule. This corrects the plan's regex.
  - **Delete** copies the row's id before the dialog opens. A live
    `notes.updated` reloads the list and can rebuild the row while the
    dialog is open.
  - **`notes.updated` for the selected meeting** refreshes the notes in
    place (`fetchNotes`), with no "Loading…" flash and no menu close. This
    replaces the plan's "re-run `select`".
  - **Selecting a meeting sets `notesMeeting` again**, so an export
    finishing for the meeting being read still shows its status line.
  - **`rowH`** comes from the measured caption line (a `TextMetrics` in
    the drawing font), not `theme.caption`, so two full meeting rows fit.
    This replaces the approved formula, which was about 3 px short per
    row. The kind and date metrics also use `Style.font.family`.
  - **`contrast.py`** checks text and dim text on the selected-row accent
    tint (0.18).
  - **A failed transcript search** does not count as "No results".
  - **The notes header title wraps.** Deleting the selected meeting also
    clears `notesStale`.
  - **Accepted:** setext headings are not capped, which is rare in
    recaps. Clicking the date or line 2 does not select. "⋯" buttons share
    one name, as Restore buttons already did. The harness timing has
    about 200 ms of slack.
