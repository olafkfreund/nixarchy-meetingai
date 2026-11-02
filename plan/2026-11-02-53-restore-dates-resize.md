---
status: approved
issue: 53
spec: spec/2026-11-02-53-restore-dates-resize.md
---

# Plan: Restore tab dates and a resizable notes pane

## Approved decisions

- Chat rows from `session.list` gain `updated_at` (Unix ms). Meeting rows keep
  `started_at` (UTC text `YYYY-MM-DD HH:MM:SS`). No schema change.
- The shell computes `when(s)` in ms: chat → `updated_at`; meeting →
  `Date.parse(started_at.replace(" ", "T") + "Z")`. Missing or unparseable
  → `0`.
- The Restore list mixes chats and meetings sorted by `when`, newest first,
  `0` last.
- Each row shows a dimmed local time (`Locale.ShortFormat`), empty for `0`.
- A 6 px vertical grip resizes the notes pane between `Style.space(48)*ts` and
  "list keeps two rows". The height resets to `Style.space(160)*ts` whenever the
  Library becomes visible. It is not stored.
- Rejected: SplitView, sorting in the daemon, persisting the height, and
  changing `started_at` to ms.

## Steps

1. `src/daemon/session.rs:3272` (`session_list`): the chat `json!` becomes
   `{"id": r.id, "kind": "chat", "title": r.title, "updated_at": r.updated_at}`.
   In the test around line 7075 (`session.list` after `meetings.search`), add
   an assertion that every row with `kind == "chat"` has `updated_at.is_i64()`.
   Make sure that test creates a chat; if it doesn't, assert on the first test
   that does, or create one with `conversations::create(c, "t", 5)`.
   → verify with `TMPDIR=/tmp devenv shell -- bash -c 'cargo fmt --check; cargo clippy --workspace --all-targets -q -- -D warnings; cargo test --workspace -q'`
   Traps: the agent path (`for_agent`) must keep filtering. Don't change
   `meetings_search`.

2. `shell/tests/fake-daemon.py:105`: `session.list` returns
   `[{"id": 1, "kind": "meeting", "title": "Fake standup", "started_at": "2026-10-30 09:00:00"},
     {"id": 2, "kind": "chat", "title": "Fake chat", "updated_at": 1793400000000}]`
   The chat (2026-10-30 22:40 UTC) is newer than the meeting (09:00 UTC),
   so the sort must put the chat first, even though the daemon lists it second.
   → verify `python3 -m py_compile shell/tests/fake-daemon.py`
   Traps: `smoke.sh:585` expects the export of "Fake standup". Keep that
   meeting's id and title.

3. `shell/Library.qml`:
   - Next to `property string notes` (line 28), add
     `property real notesHeight: Style.space(160) * theme.ts` and a
     `function when(s)` as decided. Reset it in `onVisibleChanged: if (visible) notesHeight = Style.space(160) * theme.ts`.
   - Line 267: the model becomes the filtered list sorted with
     `.slice().sort(function(a, b) { return root.when(b) - root.when(a); })`.
     `filter` already returns a new array; don't sort `root.sessions` in place.
   - In the delegate (lines 274–284), add after the title `UiText`:
     `UiText { objectName: "sessionWhen"; Layout.alignment: Qt.AlignVCenter; textFormat: Text.PlainText; opacity: root.theme.dim; font.pixelSize: root.theme.caption; color: Commons.Color.popups.text; text: { var t = root.when(rrow.modelData); return t ? Qt.formatDateTime(new Date(t), Qt.locale().dateTimeFormat(Locale.ShortFormat)) : ""; } }`
   - Give the Notes button (line 285) `objectName: "openNotes"`.
   - Between `exportStatusText` (line 304) and `notesBox` (line 315), add a grip:
     an `Item { objectName: "notesGrip"; visible: notesBox.visible; width: parent.width; height: 6 }`
     holding a 1 px centred line (`Commons.Color.foreground`, `opacity: root.theme.dim`)
     and a `MouseArea { anchors.fill: parent; cursorShape: Qt.SizeVerCursor }`.
     On press, record the global y and the starting `notesHeight`. On move while
     pressed, set `notesHeight = clamp(start - (globalY - pressY), min, max)` with
     `min = Style.space(48) * root.theme.ts` and
     `max = <column height> - <list y> - 2 * (root.theme.ctl + Style.spacing.xs) - <grip and status heights and spacings>`.
     Use `mapToItem(null, …)` the way `Overlay.qml:765` `Grip` does.
   - `notesBox.height` (line 319) becomes `visible ? root.notesHeight : 0`.
   - The ListView height (line 265) also subtracts the grip
     (`notesGrip.visible ? notesGrip.height + Style.spacing.md : 0`).
   → verify with step 4's check.
   Traps: the Column positions children, so the grip goes between them in
   source order, with no anchors. `qml_bad` (`smoke.sh:7`) fails on any
   `Binding loop`, so `max` must not read `notesBox.height`. Compute it from
   the parent height and fixed sizes. The `find()` helper in the harness skips
   invisible items, so the chat row's hidden `exportNow` is not a problem.

4. `shell/tests/smoke.sh`, export-flow harness (line 495 onwards): at the
   phase that calls `overlay.showLibrary("restore")` (+2, the one that finds
   `exportNow`), also record:
   - `whens`: the `text` of every visible `sessionWhen`, in tree order (both
     non-empty)
   - `first`: whether the first `sessionWhen` belongs to the chat (compare the
     chat's formatted time, or give the delegate's title an `objectName` and
     compare its text with "Chat · Fake chat")
   Then click `openNotes`. Two phases later, read `notesBox` height `h0`
   (objectName it `notesBox`), drive the grip's MouseArea with `pressed`
   emulation, or set `lib.notesHeight` through the clamp function if
   direct drag events can't be sent headless. Prefer pulling the clamp into
   `function setNotesHeight(h)` that the MouseArea calls, so the test calls the
   same code. Check `h1 > h0`, and that `setNotesHeight(1e6)` stays below the
   card height and `setNotesHeight(0)` gives the minimum.
   Add the assertions to the Python block after line 576.
   → verify with `env -u WAYLAND_DISPLAY -u DISPLAY -u MEETINGAI_ON_DESKTOP XDG_RUNTIME_DIR=$(mktemp -d) nix develop .#shell -c bash shell/tests/ci.sh`
   (the runtime dir must be mode 0700).
   Traps: headless only. Never set display variables or touch the desktop.

## Tests

- Step 1's cargo command: everything passes.
- Step 4's shell suite: lint, smoke (including export-flow), geometry and
  contrast all pass.
- After merge and deploy: open the Library on p620. The Restore rows show
  times newest first, and the notes pane resizes.

## Rollback

Revert the merge commit in nixarchy-meetingai and bump the nixos flake lock
back. No data or schema changes.

## Deviations

- Step 3: the grip is drawn as a short centred 2 px bar (`Style.space(40)` wide)
  rather than a full-width 1 px line, so it reads as a handle.
- Step 1: the test creates its own chat (`conversations::create(c, "Chat", 5)`)
  and asserts `updated_at == 5`.
- Review fix: when the card is too short for both the notes minimum and two
  list rows, the list wins (`max(0, min(max(h, notesMin), max))`). The default
  height on open is still not clamped, matching the old fixed 160.
- Review fix: the test asserts the exact minimum and that the list keeps two
  rows at maximum; `__pycache__/` is ignored.
- Follow-up: the maximum is now a binding (`notesMax`) and `notesBox.height`
  clamps `notesHeight` to it. The default height on open, a window resize and a
  late export status line can no longer squeeze the list under two rows. The
  drag math lives in the grip's `begin(y)`/`moveTo(y)`, which the handlers
  call, and the test drives them. Emitting MouseArea `pressed` from JS isn't
  possible, because the property shadows the signal.
