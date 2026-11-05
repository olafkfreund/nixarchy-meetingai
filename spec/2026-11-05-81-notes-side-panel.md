---
status: draft
issue: 81
intent: intent/2026-11-05-81-notes-side-panel.md
---

# Spec: Meeting notes in a right-hand column

**From the approved intent:** two columns, not a pop-out tab; a fixed split at first; the theme's font; no automatic opening of a meeting.

## Design

All in `shell/Library.qml` and its tests. No daemon change.

### S1. Layout

Below the search box, the Restore tab becomes one `Item` with two parts:

- **Left:** `restoreList`, then `linesBox` (transcript search hits), as now.
- **Right:** a `notesColumn` holding `exportStatusText`, then `notesBox`, at full height.

`readonly property bool wideNotes: width >= minimum`, where the minimum comes from what the columns need. Rows need about 420 units, and notes at least 320, so about 760 × `theme.ts`; the plan measures it.
The default window (`Style.space(900)`, `shell/Overlay.qml:79`) clears it. The list takes about 45% of the width, and notes the rest.

**`notesGrip`, `notesHeight`, `notesMax`, `notesDefault` and `setNotesHeight` go**, along with their reset at :310, because a full-height column has nothing to drag. Tests that use them change (`shell/tests/smoke.sh:660–686`).

### S2. Before anything is selected

On wide windows the right column is always there, showing a quiet message:

- no selection: "Select a meeting to read its notes";
- a chat is selected: "Chats have no notes. Use Restore to open this one."

This is what makes the preview findable. Selection works as today: clicking a row selects it, and Restore stays a one-click button on the row.

### S3. Rows in a narrower column

Today one row holds type, title, date, Restore and the menu (:422). Below a width where that fits, the date moves to the row's second line, beside the duration. The title keeps at least enough width to read, and elides. Restore and the menu stay on the first line.

### S4. Narrow windows

When `wideNotes` is false:

- Selecting a meeting shows the notes in place of the list and search hits, at full height, with a `Back to sessions` button at the top. The search text and scroll position are kept.
- `Back` and `Escape` clear the view. `Escape` returns to the list first, and closes the Library only from the list.
- Widening the window with notes open shows both columns.

### S5. Reading comfort

Within the theme:

- **Line length:** the notes text is capped at about 80 characters' width, and the pane is padded. On a very wide window, the column simply has spare space on the right.
- **Hierarchy:** a larger, bold meeting title (`theme.title` if one exists, otherwise the largest size in use); the Markdown headings keep their bold, with more space above; Timeline, Items and Bookmarks use the same heading style, not dim caption text (:670).
- **Font:** unchanged.

### S6. Keyboard and screen readers

- Tab order: search, list rows, then the notes column, whose links stay reachable.
- Up and Down in the list move between rows as today, and the notes follow the selected row.
- The right column gets an accessible name, "Meeting notes", and the empty message is readable by a screen reader.
- The narrow view moves focus to `Back to sessions`, and returning puts focus on the row it came from.

## Alternatives rejected

- **A pop-out tab on the right edge:** the owner's first idea. It covers the dates, Restore and menu while reading, needs a click to find, and adds open, close and focus behaviour. The geometry checker also flags overlaps.
- **A draggable split:** extra code before anyone has asked. Easy to add later.
- **A full view on wide windows:** brings back very long lines, and back-and-forth between meetings.
- **Keeping today's arrangement on narrow windows:** at 520 × 420 it leaves about 100 px for notes.

## Risks

- **The list gets narrower:** long titles are cut more often. They elide, and the full title shows in the notes column.
- **Tests rely on today's structure.** `harness-library.qml`, `harness-lines.qml` and `smoke.sh` find `notesBox` and the grip; they change with it. Keeping the `notesBox` object name limits this.
- **The hit list on the left can be short.** With many search hits, `linesBox` and the list share the left column, as they share the window today.
- **Not run on a real screen reader.** Accessible names are checked in code, and an actual reader test is the owner's or a later step.

## Verification

- **Automated:** `bash shell/tests/ci.sh`, with `check-geometry` at 0 problems at 100% and 150%, including wide, the 760 boundary and 520 × 420. New smoke checks: the empty message shows before selection; selecting a meeting shows notes in the right column with the list still visible; a chat shows its message; Restore still works in one click; on a narrow window, notes replace the list, `Back` and `Escape` return, and the search text is kept.
- **Manual, by the owner:** a real meeting's notes on the deployed shell, and whether it feels better to read.
