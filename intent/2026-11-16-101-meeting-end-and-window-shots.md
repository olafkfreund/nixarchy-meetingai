---
status: draft
issue: 101
author: olafkfreund
---

# Intent: Notes finish when the meeting does, and screenshots show only the meeting

## Problem

Two failures in real use (owner, 2026-11-16):

1. **The transcript does not finish when the meeting does.** After the call
   ends, recording and transcription keep running and capture whatever happens
   next in the room or on the machine.
2. **Auto screenshots capture the wrong thing.** #84 captures a fixed screen
   rectangle chosen with `slurp`. Once the owner switches workspace or opens
   another window there, that window is captured instead of the meeting. The
   owner asked for "only the call's window" in #84. It was dropped because
   `grim` was thought unable to isolate a window (`intent/2026-11-05-84-auto-screenshots.md`,
   question 2).

### What the code and a test on p620 show (verified)

Each item below is enough on its own to keep a recording running:

- **A stream stays a call while it exists.** `nixarchy-meeting-mode`
  (`~/.config/omarchy/plugins/nixarchy.meeting-mode/Service.qml:133`) keeps a
  mic stream as a call for as long as that PipeWire stream exists. If an app
  holds the mic after the meeting, the call never ends. Teams is documented to
  do this, Granola documents the same effect, and Discord holds it in a voice
  channel.
- **Confirmation is not tied to a window.** One "confirmed" flag, computed from
  every browser window's title, is applied to every mic stream (`:127`). A Meet
  title in one window confirms a mic held by another browser window.
- **The Teams pattern matches any Teams page.** `\| Microsoft Teams` also
  matches "Chat | Microsoft Teams" (`SettingsModel.js:18`).
- **Hand-started notes are never stopped.** Notes started by hand more than
  5 s before the call are not "owned" by it (`shell/Service.qml` `forCall()`,
  spec #64). They get only a "Stop notes?" notification. meeting-mode turns on
  Do Not Disturb during the call, which can hide that notification.
- **No silence backstop.** Nothing stops a recording that nobody is speaking
  into.
- **No record of why.** The daemon does not log when recording starts or
  stops, or the reason. The history cannot show which of these causes applied.

On screenshots:

- **One window can be captured.** `grim` 1.5 (`-T <identifier>`) together with
  Hyprland 0.57's `stableId` (`hyprctl clients -j`) captured a window on a
  hidden workspace, with only that window's pixels.
- **Not yet proven:** that a Chrome or Meet window on a hidden workspace gives
  a *current* frame rather than an old one.
- **A window is not a tab.** Capturing a normal Chrome window shows whichever
  tab is active in it.

### What others do (research by Fable, Codex and agy, sources in the issue)

- **Mic release:** Granola, Circleback and Jamie end on mic release.
- **Silence:** Granola stops after 15 min with no audio. Otter stops after
  10 min with no conversation, after a 15 s countdown. Jamie adds the calendar
  end and a 5–15 min idle failsafe.
- **Countdown:** all of them show a cancellable countdown before stopping.
- **Linux reference:** Char/Hyprnote ignores corked (paused) capture streams.
- **Windows and tabs:** Recall.ai ties a meeting to a window ID, but cannot see
  a background browser tab either. Granola pauses snapshots when its window is
  not visible.
- **Chrome extensions:** an extension can read Meet's or Teams' own in-call
  state. It cannot capture a background tab without a user gesture, and it
  does not cover native apps.

## Proposed outcome

- **The meeting is known, not guessed.** When a call starts, meetingai knows
  which app and which window hold it, and keeps that link while the owner works
  in other windows or on other workspaces.
- **Notes finish when the meeting ends,** after a short, visible, cancellable
  countdown ("Keep recording"):
  - this includes notes started by hand for that call;
  - it includes apps that keep the mic open after the meeting;
  - Do Not Disturb never hides the countdown.
- **A forgotten recording ends on its own.** A recording with no speech for a
  long time (the length is the owner's choice) stops after the same countdown,
  with or without a detected call.
- **Auto screenshots show only the meeting window,** wherever it is and
  whatever covers it. When the meeting cannot be seen safely (window closed,
  another tab active in its browser window), no screenshot is taken. The
  capture never falls back to the screen or another window.
- **Every automatic start and stop is logged** with its reason (no titles, no
  transcript text), so a wrong stop or a missed stop can be explained.

## Affected users and systems

- **The owner,** and everyone in their meetings (screenshots).
- **meetingai:**
  - shell: `Service.qml`, `CallState.qml`, the bar widget;
  - daemon: auto screenshots and recording stop in `src/daemon/session.rs`,
    and logging.
- **`nixarchy-meeting-mode`:** a second repository with its own gates. Any
  change to what it detects or writes to `meeting-mode.json` needs an
  approved intent → spec → plan there.
- **Hosts:** p620 and razer run the shell. Hyprland only.
- **Approved specs that change:** #64 (only owned notes stop) and #84 (a
  rectangle picked with `slurp`).

## Constraints

- **Never stop a live meeting silently.** A stop is preceded by the countdown,
  and only certain evidence may stop one. A lost window title, a detector
  restart or a remote side that is just quiet are not certain evidence.
- **Screenshot privacy rules from #84 stay:**
  - off by default and per meeting;
  - refused in Local-only, consent needed in interview mode;
  - never sent to a model;
  - skipped while the screen is locked;
  - capped, and deletable one at a time.
- **Logs never carry window titles, transcript text or screenshots.**
- **The live audio path is not slowed;** capture runs off the shared lock (as in #84).
- **No new heavy dependency.** A browser extension, if chosen, is a separate,
  explicit decision (question 3).
- **Two repositories:** meetingai must still work, as today, with an older
  meeting-mode that lacks any new fields.

## Open questions

1. **Should hand-started notes stop with the call?** **Recommended:** yes.
   Notes running when a call starts are adopted by that call and stop when it
   ends, with **Keep recording** as the escape. A room recording that should
   outlive a call needs one click.
2. **Should a silence backstop stop recordings?** **Recommended:** yes. Stop
   when there has been no finished transcript line on either channel for
   **10 min**, after the countdown, whether or not a call is detected. This is
   Otter's rule. Granola waits 15 min.
3. **Is a Chrome extension worth adding?** The researchers disagree:
   - **Codex:** yes, but only a narrow, state-only extension, and only if Meet
     or Teams must be followed through tab switches. It would report in-call
     and left-call state over native messaging.
   - **agy and Fable:** not now. It is a second detector to maintain against
     Meet and Teams DOM changes, and native apps still need the Linux path.

   **Recommended:** no extension in this task. Run meetings as PWAs or in
   their own window (the owner already has Chrome PWAs), and skip screenshots
   when the meeting tab is not the active one. Revisit if the measurements in
   question 4 show the title and mic signals are not enough.
4. **May we measure first?** Several key facts are unverified:
   - what Meet's and Teams' titles become after leaving;
   - whether Chrome, Teams or Discord keep the mic stream open, or only
     paused, after leaving;
   - whether a hidden Chrome window gives current frames.

   **Recommended:** before the spec, a short test with one real Meet call
   (tab and PWA) and one Teams call. The owner joins and leaves; the session
   records `meeting-mode.json`, `pw-dump` stream states, `hyprctl clients` and
   a `grim -T` of the hidden window. Images go to the scratchpad and are
   deleted after.
5. **Should screenshots start on their own?** #84 made auto screenshots off by
   default and switched on per meeting. **Recommended:** keep that, but turning
   it on binds to the detected meeting window instead of asking for a
   rectangle. `slurp` stays only for when no call window is known. Turning it
   on automatically for every detected call would reverse #84's privacy
   choice, so it is the owner's call.
