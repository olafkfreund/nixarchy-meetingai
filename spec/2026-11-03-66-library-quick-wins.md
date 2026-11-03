---
status: approved
issue: 66
intent: intent/2026-11-03-66-library-quick-wins.md
---

# Spec: A Library session list you can scan, with notes that say whose they are

## Design

### Daemon: `session.list` gains two fields

In `src/daemon/session.rs` `session_list` (~3515), meeting rows add
`"duration_seconds": r.duration_seconds` and `"line_count": r.line_count`.
Both are already on `MeetingRecord` (`meetings.rs:58-67`), filled by
`meetings::list`.
- Chat rows are unchanged.
- The agent filter for local-only interviews runs after the list is built
  (`:3518-3521`), so it is unaffected.
- A meeting still being recorded has `duration_seconds` 0. The row then
  shows only its line count.

### Shell: `Library.qml`, Restore tab

**Selection state:**

| Property | Meaning |
|---|---|
| `selectedId` (string, `""`) | The selected session id. Empty means nothing is selected. |
| `notesPhase` (string) | `""`, `"loading"`, `"ready"`, `"empty"` or `"error"`, for the selected meeting. |
| `linesDoneFor` (string) | The trimmed query whose `transcript.search` reply has arrived. |
| `rowH` (readonly real) | The height of a two-line meeting row: `ctl + caption + spacing`. Used for the notes reservation. |
| `menuFor` (string, `""`) | The row whose "⋯" actions are open. One at a time. |

`select(s)`:
- Set `selectedId = s.id` and `menuFor = ""`.
- Clear `notes` and `notesStale`.
- For a meeting, set `notesPhase = "loading"` and send `notes.get`. Its
  reply is applied only if `root.selectedId === id` still holds, and then
  **every** field it sets is assigned inside that guard: `notes`,
  `notesStale` and `notesPhase`.
  - An empty text gives `"empty"`.
  - A failed reply gives `"error"`, with the message.
- For a chat, set `notesPhase = ""`.
- **`Connections { target: client; onNotesUpdated(meeting) }`:** when
  `meeting === selectedId`, re-run `select` for that row. Notes written
  while the pane is open then appear.

`notesMeeting` stays as it is: Export uses it for its folder flow. The
notes pane no longer reads it.

**The row delegate** (replaces the `RowLayout` at `:309-352`). It is a
`FocusScope`-like `Item` containing a `Column`.
- **The row item:**
  - `activeFocusOnTab: true`;
  - `Accessible.role: Accessible.Button`, and `Accessible.name` =
    `"Meeting: " + title + ", " + date` (or `"Chat: …"`);
  - `readonly property string accName: Accessible.name`, like
    `IconButton.qml:20`, so `geometry.js` finds a named control;
  - `signal clicked()`. `Keys.onReturnPressed`, `Keys.onEnterPressed`,
    `Keys.onSpacePressed` and `Accessible.onPressAction` emit it, and so
    does a `MouseArea` over line 1's kind and title cells (not the
    buttons). `onClicked: root.select(modelData)`;
  - meeting rows have `objectName: "openNotes"`. Chat rows have
    `"openChat"`, so `openNotes` only ever matches a meeting.
  - **Selected** (`selectedId === id`): a `Rectangle` behind the row in
    the accent colour at low opacity, with an accent left bar 3 px wide.
    The keyboard focus ring is the same accent outline the Transcript
    flash uses.
- **Line 1** is its own `RowLayout`, and lines 2 and 3 are separate
  children of the row's `Column`, so they never take part in line 1's
  layout:
  - a kind marker: "Meeting" or "Chat", dim caption text, with
    `Layout.preferredWidth` from a `TextMetrics` of "Meeting";
  - the title, filling the width and elided;
  - `sessionWhen`, the date, with `Layout.preferredWidth` from a
    `TextMetrics` of the widest formatted sample date, and
    `Layout.minimumWidth` the same;
  - `restore`, the **Restore** `IconButton`;
  - `rowMore`, an `IconButton` "⋯" with the accessible name "More
    actions". It toggles `menuFor`.
- **Line 2 (meetings only):** a dim caption such as `42 min · 412 lines`.
  - A duration under a minute reads `< 1 min`.
  - The minutes are left out when the duration is 0 (still recording).
  - The line is hidden when both values are 0.
- **Line 3** (`menuFor === id`): right-aligned buttons:
  - `exportNow`, "Export now", meetings only;
  - `deleteSession`, "Delete", with the existing confirmation.

  On confirm it also clears `selectedId`, `notes` and `notesPhase` when
  the deleted row is the selected one.

Line 3 opens *below* the row, so opening it never changes line 1's
columns. Only that row grows taller.

The row height is now variable (one to three lines). `restoreList` uses a
`ListView` with delegate-sized items, which already works.
- `notesMax` (`:43`) reserves `2 * root.rowH` instead of
  `2 * (theme.ctl + Style.spacing.xs)`, so the list keeps two full meeting
  rows when the notes pane is at its largest.
- The `restoreList` height binding (`:306`) also subtracts `noResults`
  when it is visible.

**The notes pane** (`notesBox`, `:449`). It is visible when
`root.tab === "restore" && notesPhase !== ""`, so loading, empty and error
show too.
- **A header** (`notesTitle`): the selected meeting's title (body size,
  bold) and its date (caption, dim), followed by the existing
  `notesStale` caption.
- **The body, by `notesPhase`**, shown in a `UiText` with
  `objectName: "notesStateText"` for the non-ready states:
  - `"loading"`: "Loading notes…";
  - `"empty"`: "No notes yet for this meeting.";
  - `"error"`: "Notes failed: " + the message;
  - `"ready"`: the recap.
- **Heading cap:** before rendering, the recap text goes through
  `capHeadings(md)`, a `Library` function. It caps recap headings at
  body-bold, so "Meeting recap" and "Summary" no longer outsize the
  overlay. It walks the text line by line:
  - **Fences:** a line matching `^ {0,3}(```|~~~)` toggles "in a fence",
    and lines inside a fence are left alone.
  - **Headings:** outside fences, a line matching
    `^ {0,3}#{1,6}[ \t]+(.*?)[ \t]*#*[ \t]*$` becomes `**` + the
    heading text with any `**` inside removed + `**`.
  - **Blocks:** a blank line is put before and after the rewritten line,
    so `Text.MarkdownText` renders it as its own block.
- The grip (`notesGrip`), `notesHeight`, `notesMax` and the export status
  line are unchanged.

**Search.**
- The placeholder depends on the tab: `"Search meetings, chats and
  transcripts"` on Restore, `"Search settings, chats, profiles"` elsewhere.
- **No results** (`objectName: "noResults"`, dim caption under the
  search field) shows on Restore only when all of these hold:
  - `query.trim().length >= 2`;
  - the filtered session list is empty;
  - `linesDoneFor === query.trim()`, so the transcript search for this
    query has answered;
  - `lineHits` is empty.

  `searchLines` sets `linesDoneFor` in its reply, after `lineHits`, and
  clears `lineHits` when the query drops under 2 characters.

**Objects kept for the tests:**

| objectName | Now on |
|---|---|
| `restoreList`, `sessionWhen`, `restore`, `notesBox`, `notesGrip`, `notesDrag`, `notesStale`, `linesBox`, `lineHits`, `exportStatus` | unchanged |
| `openNotes` | the meeting row item, with `signal clicked()` |
| `exportNow`, `deleteSession` | line 3 buttons, visible after `rowMore` is clicked |
| new: `rowMore`, `openChat`, `notesTitle`, `notesStateText`, `noResults`, `selectedBar` | |

### Tests

- **`smoke.sh` export flow (~581-625):**
  - In phase 13 **and again in phase 17**, before `find(card,
    "exportNow")`, click the meeting row's `rowMore`. Selecting the row
    closes the menu.
  - The notes-size check (`:602-605`) uses `lib.rowH` instead of
    `lib.theme.ctl + Style.spacing.xs`.
  - `openNotes.clicked()` keeps working, because the row item has
    `clicked()`.
  - Assert that after that click, `notesTitle` holds the fake meeting's
    title and `selectedBar` is visible.
- **`harness-grounded.qml` notes check:** unchanged. It clicks
  `openNotes` and expects `notesStale`.
- **A new `harness-library.qml` phase** in `smoke.sh` (the fake daemon's
  default session list):
  - select a meeting; the title shows, and the state is loading, then
    ready;
  - select a chat; the pane is hidden;
  - open "⋯" on a chat; Export is absent and Delete is present;
  - a search that matches nothing shows `noResults`;
  - `lib.capHeadings` on a fixture gives the expected text: a `# Title`,
    an indented `  ## Sub`, a heading inside a ```` ``` ```` fence and one
    inside a `~~~` fence (both unchanged), and `# A **b**` (inner `**`
    removed). The rendered recap header has no `#`;
  - `notesUpdated` for the selected meeting reloads its notes;
  - the fake's meeting row gets `"duration_seconds": 2520, "line_count":
    412`, and the second line reads `42 min · 412 lines`;
  - geometry snapshots pass `check-geometry.py`.
- **The late-reply guard:** the fake daemon delays `notes.get` with
  `--slow-notes` (0.8 s). The harness selects meeting A, then at once
  selects chat B, then waits 1.2 s; the pane stays hidden.
- **Rust:** in `session.rs` `command_tests`, `session.list` meeting rows
  carry `duration_seconds` and `line_count`. The agent filter test still
  passes.

## Review record (draft spec)

| Finding | From | Verdict |
|---|---|---|
| The export smoke flow clicks `exportNow` twice; phase 17 must reopen "⋯" too | Codex, agy | confirmed (`smoke.sh:583`, `:625`) → tests |
| `notesMax` reserves two old fixed rows; with two-line rows the reservation and its smoke assertion are wrong | Codex | confirmed (`Library.qml:43`, `smoke.sh:602-605`) → `rowH` |
| "No results" could show while the 300 ms transcript search is in flight, and stale hits stay up | Codex, agy | confirmed → `linesDoneFor` |
| `capHeadings` needs exact rules: indent, backtick and tilde fences, blank lines, inner `**` | Codex, agy | confirmed → exact rules, with a fixture test |
| The row needs `Accessible.onPressAction`, `Keys.onEnterPressed` and `accName` for `geometry.js` | Codex, agy | confirmed (`IconButton.qml:16-20`) |
| `notesState` was both a property and an `objectName` | Codex, agy | confirmed → `notesPhase` plus `notesStateText` |
| Fixed widths in a `RowLayout` need `Layout.preferredWidth` | agy | confirmed |
| Durations under a minute; `notesUpdated` should refresh the selected meeting | agy | confirmed |

## Alternatives rejected

- **A popup menu for "⋯".** The overlay is a layer-shell window. Popups
  there need extra windowing work, and no menu component exists in the
  shell. An inline line 3 is simpler, stays within the card, and keeps
  the columns fixed.
- **Export on the selected row only.** The column would move when the
  selection changes (Codex). Rejected in the intent.
- **Keeping a Notes button.** Selection does its job; the intent removed
  it.
- **Styling Markdown headings through Qt.** `Text.MarkdownText` has no
  per-heading style hook, so a pre-pass on the text is the smallest
  reliable cap.
- **A new list or detail component.** That is #60 phase B.

## Risks

- **The heading rewrite** touches only lines starting with `#` outside
  code fences. A recap line that legitimately starts with `#` (rare)
  turns bold, which is harmless.
- **Variable row heights** change the geometry snapshots. `live-shots`
  and `check-geometry.py` re-baseline in the same change if their
  thresholds are positional.
- **Contrast:** the selected background uses the accent at low opacity
  behind popup text. `contrast.py` checks it, and the opacity is raised
  if needed.
- **Agents:** `session.list` gains two integers, with no text and nothing
  from local-only interviews.

## Verification

- `cargo test --workspace` (the new `session.list` test) and
  `fmt`/`clippy`.
- `bash shell/tests/lint.sh` and `bash shell/tests/ci.sh` (smoke, the
  library phase, grounded, geometry, contrast).
- `nix build .#checks.x86_64-linux.shell`.
- **Manual after deploy:** open the Library and click a meeting. It is
  highlighted with its name above the notes, the headings are normal
  size, and the date column does not move when you click "⋯" or another
  row.
