---
status: approved
issue: 53
intent: intent/2026-11-02-53-restore-dates-resize.md
---

# Spec: Restore tab dates and a resizable notes pane

## Design

### Daemon: chats carry their time

`src/daemon/session.rs` `session_list` (line ~3272) adds the chat's
`updated_at` (Unix ms, already in `ConversationRecord`) to each chat row:
`{"id","kind":"chat","title","updated_at"}`. Meeting rows already carry
`started_at` (SQLite `datetime('now')` text, UTC, `YYYY-MM-DD HH:MM:SS`).
Nothing else changes: the agent filter and the MCP callers keep working, and
the extra field is ignored there.

### Shell: time, order, resize (`shell/Library.qml`)

1. A function `when(s)` returns ms since the epoch: `s.updated_at` for a chat;
   for a meeting, `Date.parse(s.started_at.replace(" ", "T") + "Z")`.
   A missing or unparseable value gives `0`.
2. The Restore model is the filtered sessions sorted by `when` descending,
   so chats and meetings mix newest first. Entries with `0` sort last.
3. Each row gets a dimmed caption `UiText` (`objectName: "sessionWhen"`)
   between the title and the buttons, showing local time with
   `Qt.formatDateTime(new Date(ms), Qt.locale().dateTimeFormat(Locale.ShortFormat))`.
   Empty when `when` is `0`. The title keeps `fillWidth` and elides first.
4. `notesBox` height comes from a new `property real notesHeight`, defaulting
   to today's `Style.space(160) * theme.ts`. A 6 px grip (`objectName:
   "notesGrip"`, `Qt.SizeVerCursor`, visible with `notesBox`) sits between the
   export status and `notesBox`. Dragging it up grows the notes pane, down
   shrinks it. Clamp: at least `Style.space(48) * ts` (about three lines), and
   at most what leaves the list two rows (`2 * (theme.ctl + Style.spacing.xs)`).
   The ListView height formula already subtracts `notesBox.height`, so the list
   follows on its own.
5. `notesHeight` goes back to the default each time the Library becomes
   visible (approved intent decision 1: not remembered).

## Alternatives rejected

- **QtQuick.Controls `SplitView`.** It would mean restructuring the tab's
  Column and styling a second kind of handle. A grip plus one property is
  smaller and matches the Overlay's own resize grips.
- **Sorting in the daemon.** The daemon has no date crate, and parsing the
  SQLite text by hand in Rust buys nothing over the shell doing it.
- **Remembering the height in settings.** Declined in the intent.
- **Changing meetings' `started_at` to ms.** A schema change for display only.

## Risks

- `started_at` is UTC text without a zone. Appending `Z` is right for rows
  written by `datetime('now')`; a legacy row with another format shows no time
  and sorts last instead of breaking the list.
- Locale short format varies by user; that is the point, and the column width
  is set by the text, so a long format only elides the title more.
- Sorting changes which rows are visible first: meetings no longer sit
  after 40 chats. The daemon still returns at most 40 of each.

## Verification

- `cargo fmt --check; cargo clippy --workspace --all-targets -q -- -D warnings; cargo test --workspace -q`
  in the devenv shell. The existing `session.list` test in
  `src/daemon/session.rs` (~line 7075) also asserts a chat row has a numeric
  `updated_at`.
- Headless shell suite (`shell/tests/ci.sh`, no display variables). The fake
  daemon's `session.list` returns an older meeting and a newer chat; the
  export-flow harness in `shell/tests/smoke.sh` checks that the chat is first,
  that both `sessionWhen` texts are non-empty, and that moving `notesGrip`
  changes `notesBox` height within the clamp.
- Deploy to p620 and look at the Restore tab: rows show times, newest first,
  and the notes pane resizes.
