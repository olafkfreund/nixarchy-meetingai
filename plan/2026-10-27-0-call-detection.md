---
status: draft
issue: none (issues are disabled on olafkfreund/mixarchy-meetingai)
spec: spec/2026-10-27-0-call-detection.md
reviewers:   # read-only, after the Opus review
  - codex
  - agy
---

# Plan: catch every call reliably and make taking notes one click (meetingai side)

The detector work is in `nixarchy-meeting-mode`
(`spec/2026-10-27-call-confidence.md` there, its own gates). This plan
covers meetingai and starts after the UI quality plan lands and the
meeting-mode branch `feat/call-confidence` is merged or pinned.

## Approved decisions (copied from the spec)

- meeting-mode is the only detector. Its state file
  `$XDG_RUNTIME_DIR/meeting-mode.json` carries `call`
  (`none|maybe|call`), `callApp`, `callTitle`, `callSince`, `active`
  (quiet-desktop applied) and `callDetected` (= `call === "call"`).
- meetingai reacts to `call`, never to `active`:
  - `none → call`: auto-record if `auto_record_on_call` and the mode is
    not interviewer; otherwise show the offer
  - `→ maybe`: dim bar dot only
  - `call → none` while recording: offer "Stop notes", never stop alone
  - `callTitle` (or the calendar event) names the session
- **Offer surfaces:** a "Take notes?" chip in `BarWidget.qml` (click
  starts, right-click dismisses for this call); a `notify-send -A`
  notification; IPC `omarchy-shell meetingai offer`; the record hotkey
  accepts. Bar: dim dot = maybe, outlined = call not recording, filled +
  timer = recording; tooltip = app and title.
- **Calendar** (setting `calendar_enabled`, default off): the daemon runs
  `gog calendar events` read-only (now −10 min to +10 min, JSON) every
  5 min while idle and on each `call` change; an event with a Meet,
  Teams or Zoom link names the session and is written to
  `$XDG_RUNTIME_DIR/nixarchy-meetingai/calendar.json` (0600) for
  meeting-mode to use as a confirmer.
- **Missing detector:** no state file 30 s after start → Settings shows
  "Call detection is off: enable the meeting-mode plugin" with the
  command.
- **Nix:** meetingai's flake takes `nixarchy-meeting-mode` as an input;
  `programs.nixarchy-meetingai.callDetection.enable` (default true) adds
  the plugin to `programs.nixarchy.plugins` unless it is already there.

## Repo traps

- Same as the UI plan (shell tests with `env -u SIZE`, crops checked, the
  lead commits, cargo via devenv, rustfmt only touched files).
- `gog` must run with the user's own login; never pass tokens; the
  calendar call is skipped when `gog` is missing or not logged in.
- Window and event titles stay out of logs (`tracing` lines name the app
  only).
- The HM module must not add the plugin twice when the owner's
  `omarchy-meeting-mode.nix` already adds it: compare by plugin id.

## Steps

1. **Service.qml:** read `call`, `callApp`, `callTitle`; the
   transitions above; session title from `callTitle`; interviewer mode
   never auto-records; the missing-detector check.
   → verify by a new `shell/tests/call-state.sh` that writes fake state
   files (manual `active:true, call:"none"` gives no offer; `none → call`
   gives the offer; `call → none` gives "Stop notes") and greps the fake
   daemon log and `notify-send` stub.
2. **Offer surfaces:** BarWidget chip and dot states, tooltip, IPC
   `offer`, dismissal per call; `fake-daemon.py --call` and
   `live-shots.sh SCENARIO=call`.
   → verify by lint, smoke, crops at 100 % and 150 %, geometry check.
3. **Calendar:** `src/calendar.rs` (parse `gog` JSON; pick the event that
   overlaps now and has a conference link), the daemon timer, the
   `calendar_enabled` setting, the 0600 file, `state.calendar_title`.
   → verify by unit tests with a fixture JSON (no network), a test that
   a missing `gog` is a quiet no-op, clippy.
4. **Nix:** flake input, the HM option, an eval check that the plugin is
   added exactly once (with and without the owner's existing entry).
   → verify by `nix flake check`.
5. **Docs:** README "Calls" section (what is detected, maybe vs call,
   auto-record, calendar, privacy).
6. **Review:** Opus, then Codex and agy; fix confirmed findings.
7. **Real calls with the owner:** Meet in Chrome and Firefox, Teams PWA,
   Zoom, Discord/Vesktop, Slack huddle, Messenger; record maybe/call/end
   and latency per app in `docs/call-detection.md`; adjust the title
   patterns in meeting-mode if needed (a deviation there).

## Tests

- `call-state.sh`, lint, smoke, smoke-real, contrast, geometry check.
- `cargo test --workspace --no-fail-fast`, clippy `-D warnings`.
- `nix flake check`.

## Rollback

Revert the commits; meetingai falls back to the old `active` behaviour
only if step 1 is reverted. The HM option can be set to false to stop
adding the plugin.

## Implementation split

One `coder` does steps 1–5 in order; the lead commits each step, runs
step 6, and step 7 is done live with the owner.

## Deviations log (recorded with the implementation commit)

None yet.
