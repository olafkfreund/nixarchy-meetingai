---
status: draft
issue: 81
spec: spec/2026-11-05-81-notes-side-panel.md
coders:
  1: codex   # tricky: restructures the Restore layout and the narrow state, with Escape
  2: coder   # rows and reading comfort: ordinary QML, after step 1
  3: coder   # tests and README, in parallel with step 2, on separate files
reviewers:
  - codex    # read-only: layout, state, keyboard
  - agy      # read-only: tests and anything lost
---

# Plan: Meeting notes in a right-hand column

**Running order:** step 1 alone, because it restructures `Library.qml`; then steps 2 and 3 in parallel. Claude runs checks, reviews and commits.

## Decisions carried from the approved spec

- Two columns when wide: list and search hits left, about 45%; notes right, full height. Not a pop-out tab. A fixed split, no saved width.
- `wideNotes` is `root.width >= Style.space(760) * theme.ts`, where `root` is the Library, so it uses content width. Step 1 starts there; step 2 changes the number only if the rows do not fit, and records it. Below it, selecting a **meeting** swaps the list for its notes, with `Back to sessions`.
- Before selection, the right column says "Select a meeting to read its notes". For a chat: "Chats have no notes. Use Restore to open this one." `notesBox` stays hidden for a chat, with the message as a sibling.
- Restore stays one click. No daemon change. No Up or Down between rows.
- Reading: theme font; lines capped at 80 characters' width, from font metrics; padding; title in `theme.subtitle`; Timeline, Items and Bookmarks in the same heading style, not dim caption text.
- `notesBox` moves whole, keeping every state and link. `notesGrip`, `notesHeight`, `notesMax`, `notesDefault` and `setNotesHeight` go.

## Traps

- **Line numbers** are for `shell/Library.qml` as of this plan; find by object name.
- **Codex's sandbox cannot run the shell suite.** Claude runs it.
- **`root.find` only sees visible items,** which matters for the tests.
- **`agent-run` treats a moved `HEAD` as failure:** nothing is committed during a handoff.
- **The `grounded` smoke flake** is #79, separate from this.

## Steps

### 1. Layout and narrow state (codex) — `shell/Library.qml`, `shell/Overlay.qml`

1. Replace the Restore tab's stacked `restoreList`, `linesBox`, `exportStatusText`, `notesGrip`, `notesBox` (:377–:730) with one height-filling `Item`: `restoreColumn` (list, then `linesBox`) and `notesColumn` (`exportStatusText`, empty message, `notesBox`).
2. `readonly property bool wideNotes`, and `property bool narrowNotes`. `select()` (:136) sets it true for a meeting and false for a chat. **Exits, all to false:** `Back`, Escape, closing the Library (`onVisibleChanged`, :310), changing tab, and deleting the selected meeting. Typing in search leaves it. Widening shows both columns, and narrowing with a meeting selected keeps the list until one is chosen, since `narrowNotes` is only set by `select()`. While `narrowNotes && !wideNotes`, `restoreColumn` and `noResults` (:356) are hidden.
3. `Back to sessions` (`objectName: "notesBack"`) at the top of `notesColumn`, shown only in the narrow swap, and it takes focus when the swap opens. `function back()` returns true if it closed the swap, and returns focus to the selected row, using `restoreList.currentIndex` or `positionViewAtIndex`.
4. `shell/Overlay.qml:282`: Escape asks the Library's `back()` first, then closes the Library as today.
5. Remove the grip and height properties, including `notesMin`, or `lint.sh` fails on unused names, and the height reset at :310. `restoreList`'s height no longer subtracts notes.
6. The empty messages, `objectName: "notesEmpty"`, using `Accessible.role: Accessible.StaticText` and `Accessible.name`. `notesColumn` has `Accessible.role: Accessible.Pane`, and the accessible name "Meeting notes".

Check, by Claude: `bash shell/tests/lint.sh`, and a manual run of the fake-daemon Library harness. `smoke.sh` will fail on removed names until step 3.

### 2. Rows and reading (coder) — `shell/Library.qml`

1. The row (:422): the date moves to the second line, beside the duration, for every row; a chat gains a second line. `rowH` (:48) still holds. Restore and the menu stay on the first line, and the title elides. Remove the first-line width reserved through `dateMetrics` if nothing else uses it.
2. Notes text capped at 80 characters' width: `Math.min(availableWidth, 80 * metrics.averageCharacterWidth)`. Padding inside the column.
3. The title in `theme.subtitle`; more space above headings; Timeline, Items and Bookmarks (:670–:706) in one heading style.

Check: `bash shell/tests/ci.sh`, with step 3.

### 3. Tests and docs (coder) — `shell/tests/`, `README.md`

1. `harness-library.qml`, `smoke.sh` (:180, :190, :660–:687, and the `hasGrip` and resize assertions at :743): remove the grip checks. Keep the late-reply and chat checks, which still hold on `notesBox`. Update the date's position (`whenBefore`) to: `sessionWhen`'s y is below the title's. `harness-lines.qml` and `harness-grounded.qml` find rows after opening notes, so they run at wide sizes, where the list stays visible; confirm this.
2. New checks, in `harness-library.qml`:
   - wide, 1200 × 800: `notesEmpty` shows before selection; after `openNotes`, `notesBox` and `restoreList` are both visible and side by side; on a chat row, `notesEmpty` shows and `notesBox` does not; Restore works with one click.
   - narrow, 520 × 420, in this order, because `find` only sees visible items: open a meeting, `restoreList` is hidden, `notesBack` visible; call `back()`, the list returns and the search text is unchanged; open a chat, the list stays; open a meeting again, press Escape through the overlay's handler, the list returns; press it again, the Library closes.
   - `noResults` is hidden in the swap.
   - geometry dumps at 1200 × 800, 760 and 759 units wide, and 520 × 420, at 100% and 150%.
   Step 2's date and reading layout are asserted only through these observable positions, so the combined suite runs after both steps finish.
3. `README.md`: a short note on where notes appear.

Check: `bash shell/tests/ci.sh`, 0 geometry problems.

### 4. Ship (Claude)

Full checks, `nix flake check`; reviews by a fresh Opus agent, Codex and agy, read-only; a PR closing #81; merge after CI. Then one nixos PR bumping only the meetingai input; test build, CI, merge, and deploy to p620 from a clean worktree, only when nothing is recording, after checking the bus. Check the revision afterwards. The owner looks at it on a real meeting.

## Rollback

`git revert -m 1 <merge>`, and bump nixos again.

## Deviations

(Recorded here during implementation, in the same commit as the code.)
