---
status: draft
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
| `notesState` (string) | `""`, `"loading"`, `"ready"`, `"empty"` or `"error"`, for the selected meeting. |
| `menuFor` (string, `""`) | The row whose "⋯" actions are open. One at a time. |

`select(s)`:
- Set `selectedId = s.id` and `menuFor = ""`.
- Clear `notes` and `notesStale`.
- For a meeting, set `notesState = "loading"` and send `notes.get`. Its
  reply is applied only if `root.selectedId === id` still holds.
  - An empty text gives `"empty"`.
  - A failed reply gives `"error"`, with the message.
- For a chat, set `notesState = ""`.

`notesMeeting` stays as it is: Export uses it for its folder flow. The
notes pane no longer reads it.

**The row delegate** (replaces the `RowLayout` at `:309-352`). It is a
`FocusScope`-like `Item` containing a `Column`.
- **The row item:**
  - `activeFocusOnTab: true`;
  - `Accessible.role: Button`, and `Accessible.name` = `"Meeting: " +
    title + ", " + date` (or `"Chat: …"`);
  - `signal clicked()`. `Keys.onReturnPressed` and `Keys.onSpacePressed`
    emit it, and so does a `MouseArea` that fills the text area (not the
    buttons). `onClicked: root.select(modelData)`;
  - meeting rows have `objectName: "openNotes"`. Chat rows have
    `"openChat"`, so `openNotes` only ever matches a meeting.
  - **Selected** (`selectedId === id`): a `Rectangle` behind the row in
    the accent colour at low opacity, with an accent left bar 3 px wide.
    The keyboard focus ring is the same accent outline the Transcript
    flash uses.
- **Line 1:**
  - a kind marker: "Meeting" or "Chat", dim caption text, with a fixed
    width from a `TextMetrics` of "Meeting";
  - the title, filling the width and elided;
  - `sessionWhen`, the date, in a fixed width from a `TextMetrics` of the
    widest formatted sample date;
  - `restore`, the **Restore** `IconButton`;
  - `rowMore`, an `IconButton` "⋯" with the accessible name "More
    actions". It toggles `menuFor`.
- **Line 2 (meetings only):** a dim caption such as `42 min · 412 lines`.
  The minutes are left out when the duration is 0, and the line is hidden
  when both values are 0.
- **Line 3** (`menuFor === id`): right-aligned buttons:
  - `exportNow`, "Export now", meetings only;
  - `deleteSession`, "Delete", with the existing confirmation.

  On confirm it also clears `selectedId`, `notes` and `notesState` when
  the deleted row is the selected one.

Line 3 opens *below* the row, so opening it never changes line 1's
columns. Only that row grows taller.

The row height is now variable (one to three lines). `restoreList` uses a
`ListView` with delegate-sized items, which already works. Its own height
binding (`:306`) is unchanged.

**The notes pane** (`notesBox`, `:449`). It is visible when
`root.tab === "restore" && notesState !== ""`, so loading, empty and error
show too.
- **A header:** the selected meeting's title (body size, bold) and its
  date (caption, dim), followed by the existing `notesStale` caption.
- **The body, by `notesState`:**
  - `"loading"`: "Loading notes…";
  - `"empty"`: "No notes yet for this meeting.";
  - `"error"`: "Notes failed: " + the message;
  - `"ready"`: the recap.
- **Heading cap:** before rendering, the recap text goes through
  `capHeadings(md)`. It rewrites Markdown ATX headings (`^#{1,6}\s+(.+)$`)
  to `**$1**` on their own line, outside fenced code. This caps recap
  headings at body-bold, so "Meeting recap" and "Summary" no longer
  outsize the overlay.
- The grip (`notesGrip`), `notesHeight`, `notesMax` and the export status
  line are unchanged.

**Search.**
- The placeholder depends on the tab: `"Search meetings, chats and
  transcripts"` on Restore, `"Search settings, chats, profiles"` elsewhere.
- **No results** (`objectName: "noResults"`, dim caption under the
  search field) shows on Restore when `query.trim().length >= 2`, the
  filtered session list is empty, and `lineHits` is empty.

**Objects kept for the tests:**

| objectName | Now on |
|---|---|
| `restoreList`, `sessionWhen`, `restore`, `notesBox`, `notesGrip`, `notesDrag`, `notesStale`, `linesBox`, `lineHits`, `exportStatus` | unchanged |
| `openNotes` | the meeting row item, with `signal clicked()` |
| `exportNow`, `deleteSession` | line 3 buttons, visible after `rowMore` is clicked |
| new: `rowMore`, `openChat`, `notesTitle`, `notesState`, `noResults`, `selectedBar` | |

### Tests

- **`smoke.sh` export flow (~581-625):**
  - Before `find(card, "exportNow")`, click the first meeting row's
    `rowMore`.
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
  - the recap header text is rendered bold, with no `#`;
  - the fake's meeting row gets `"duration_seconds": 2520, "line_count":
    412`, and the second line reads `42 min · 412 lines`;
  - geometry snapshots pass `check-geometry.py`.
- **The late-reply guard:** the fake daemon delays `notes.get` with
  `--slow-notes` (0.8 s). The harness selects meeting A, then at once
  selects chat B, then waits 1.2 s; the pane stays hidden.
- **Rust:** in `session.rs` `command_tests`, `session.list` meeting rows
  carry `duration_seconds` and `line_count`. The agent filter test still
  passes.

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
