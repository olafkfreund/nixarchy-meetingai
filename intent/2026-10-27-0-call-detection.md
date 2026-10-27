---
status: approved
issue: none (issues are disabled on olafkfreund/mixarchy-meetingai)
author: olafkfreund
reviews: research agent (PipeWire, Hyprland, xdph, notifications, existing tools), 2026-10-27; local checks on p620
---

# Intent: catch every call reliably and make taking notes one click

## Problem

The owner wants the app to notice any call (Google Meet, Teams, Zoom,
Discord, Slack huddles, Messenger/Facebook, WhatsApp, Signal, Jitsi,
Webex, anything sharing screen or camera while talking to people) and
offer, or automatically start, notes.

Today meetingai has no detector of its own. `shell/Service.qml` reads the
state file of the sibling plugin `nixarchy-meeting-mode`
(`$XDG_RUNTIME_DIR/meeting-mode.json`). Verified gaps:

- **It reacts to the wrong field.** It watches `active` ("quiet-desktop
  actions are applied"), not `callDetected`. Turning meeting mode on by
  hand with no call says "A call started"; turning it off mid-call says
  "The call ended". `callDetected` is only written inside start/stop, so
  it can be stale.
- **Silent dependency:** if the meeting-mode plugin is off, nothing is
  ever detected and nothing says so.
- **Browsers are guesses:** any browser mic use counts as a call (voice
  typing, voice notes, mic tests). Chrome's capture streams carry no tab
  title or URL.
- **Missed apps:** Vesktop (`vesktop`) and the native Webex client are not
  in the pattern.
- **The prompt can be hidden:** meeting-mode turns on Do Not Disturb at
  the same moment, and Omarchy's notification server holds back pop-ups
  under DND, so "Take notes?" likely lands only in the panel (inferred
  from the code, not reproduced).
- **Unnamed meetings:** no window title or calendar event names the
  session.

## Proposed outcome

- **One detector, shared:** `nixarchy-meeting-mode` stays the only call
  detector, and meetingai reads it, so the desktop quieting and the notes
  agree on what a call is.
- **Confident detection** in two tiers:
  - native call apps: the app holds the mic for 3 s or more
  - browsers: mic for 3 s or more, plus a confirmer: a meeting window
    title (Meet, Teams, Zoom, Jitsi, Webex, huddle, Messenger call), an
    active screen share (the same `xdph-streaming-` check Omarchy uses),
    Firefox's sharing indicator, or a calendar event with a conference
    link happening now
  - mic alone in a browser is a "maybe"; it becomes a call after 60 s
  - a call ends 15 s after the mic is released (configurable)
- **The offer can't be missed:** the meetingai bar widget shows a
  "Take notes?" chip during a detected call that isn't being recorded.
  One click, or the record hotkey, starts notes. The notification is a
  second channel, sent before DND turns on.
- **Auto-record** stays opt-in and fires only for confirmed calls, never
  for a "maybe". The interviewer consent and local-only rules still apply.
- **Named sessions:** the window title or calendar event names the
  meeting.
- **One-step setup on nixarchy:** enabling meetingai in Home Manager also
  enables meeting-mode's detection (or warns clearly if it's missing).

## Affected users and systems

- The owner, on p620 (and razer, which also imports meeting-mode).
- `nixarchy-meeting-mode` (`Service.qml`, `SettingsModel.js` and its
  tests): the state file gains `callTitle` and `confidence`, is written on
  every detection change, and gets the browser-confirmer rule and the
  missing apps. This is a second repository.
- meetingai: `shell/Service.qml`, `shell/BarWidget.qml`, the daemon (only
  if the calendar part is done there), `nix/hm-module.nix`, `flake.nix`.
- The owner's system config: no new flake input if meetingai takes
  meeting-mode as its own input.

## Constraints

- No polling where PipeWire, Quickshell or Hyprland events exist.
- Titles and calendar names can be sensitive: keep them in the runtime
  state file (user-only) and in memory, never in logs.
- No browser extension, remote-debugging port or screen capture to
  detect calls.
- Changes to `nixarchy-meeting-mode` follow that repo's own
  intent/spec/plan gates.
- Real-call checks need the owner: a short test call per app family to
  confirm the title patterns and stream names.

## Open questions

1. **Calendar:** use Google Calendar (through the installed `gog` CLI) to
   name meetings and confirm browser calls? (Proposed: yes, optional, off
   by default, read-only.)
2. **Where to build it:** extend `nixarchy-meeting-mode` and have meetingai
   depend on it (proposed), or build a second detector into meetingai's
   daemon?
3. **"Maybe" calls:** show only a dim dot on the bar with no prompt
   (proposed), or prompt for those too?
4. **Order:** after the UI quality work (proposed), or first?
