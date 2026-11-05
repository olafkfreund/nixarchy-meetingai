---
status: approved
issue: 81
author: olafkfreund
---

# Intent: A meeting notes preview that is easy to find and read

## Problem

In the Library's Restore tab, a meeting's notes are the thing people come to read, and they are hard to get to (owner report, with a screenshot, 2026-11-05):

- **Hard to find.** Nothing shows there is a preview until a row is clicked. Clicking the row's main button, "Restore", does something else.
- **Cramped.** The notes sit under the session list (`shell/Library.qml:583`), sharing the height. The default height is fixed (`notesDefault`, :39), so reading a long recap means scrolling inside a short pane, while the list above stays large and its rows have an empty stretch across the middle.
- **Too wide to read.** Lines run the full window width, about 1,450 px in the screenshot, far longer than comfortable reading.
- **Resizing is obscure and forgotten.** A 2 px grip (:553) is the only control, and the height resets whenever the Library opens (:310).
- **Hard to scan.** Title, date, notes, items and bookmarks run in one column, all in the same small monospace text.

## Proposed outcome

- A person opening the Restore tab can see where notes appear, and that selecting a meeting shows them, without being told.
- Selecting a meeting shows its notes on the right of the window, without leaving the list, as the owner suggested.
- Notes are comfortable to read: a sensible line length, clear headings, and enough height for a recap without constant scrolling.
- It works on narrow windows too. The overlay can be as small as 520 × 420 (`shell/Overlay.qml:69`).
- Everything the pane does today still works: stale notice, timeline, items, bookmarks, links into the transcript, transcript search hits, the export status line, and keyboard use.
- Focus order and how the notes read in a screen reader are checked, not assumed. The rows and grip have keyboard support and accessible names today; whether the notes body and headings read well is unverified.

## Affected users and systems

Everyone using the Library. `shell/Library.qml` and its tests (`shell/tests/harness-library.qml`, geometry checks). This includes list rows, which put type, title, date, Restore and a menu on one line (:422) and must fit a narrower column, and the transcript search hits (:488) and export status (:544), which need a place in the new layout. No daemon or database change is expected.

## Constraints

- **No daemon change,** unless the design genuinely needs one.
- **Keyboard and accessibility stay at least as good.** The grip and rows have keyboard support and accessible names today.
- **The small-window minimum still has to work,** with no overlap, as checked by `check-geometry`.
- **The theme stays:** colours and fonts come from the shell's theme.
- **Restoring a session must not get harder.** "Restore" remains one click on the row.
- **Smallest change that works.** No redesign of the whole Library.

## Open questions

1. **Which shape?** The owner suggested a tab that pops out on the right. **Recommended, by Codex and agy independently: a two-column view instead, list on the left and notes on the right, whenever the window is wide enough.**
   - **Always visible,** with a "Select a meeting to read its notes" message until one is chosen. That answers "hard to find" better than a tab, which is one more thing to find and click.
   - **Keeps the list usable.** A drawer covers the right side of the rows, where the date, Restore and the menu live, so you could not move between meetings or restore while reading. One design guide calls a drawer that hides its own list "a worse modal".
   - **Line length and height come almost for free:** the column holds lines near a readable length, and gets the full height.
   - **Fewer new behaviours.** A drawer adds open and close, focus handling and overlap, which the geometry checker flags.
   - **The cost:** rows must fit a narrower column.
   - **The owner's pop-out tab remains an option.** Choosing it means accepting a covered list and an extra click.
2. **Below what width?** Recommended, by both: when two columns no longer fit, selecting a meeting shows its notes in place of the list, with a labelled "Back to sessions". Today's arrangement leaves too little height on a small window. The width comes from measuring what the rows need; agy's estimate is about 760 units, which the default window clears.
3. **Remember the panel's width?** Recommended, by both: not as a daemon setting. Start with a sensible fixed split, and add dragging only if it is missed.
4. **Readability within the theme?** Recommended: keep the theme's font, cap lines at roughly 65–80 characters, pad the pane, and give the title and section headings clearer weight. Timeline, Items and Bookmarks use dim caption text today (:670), and that is the first thing to improve. A different font for notes is a separate question.
5. **Open the most recent meeting automatically?** agy suggests it, so notes show at once. Recommended: not at first. A clear empty message is simpler, and fetching notes on every open is extra work. Revisit if the empty panel feels wasted.
