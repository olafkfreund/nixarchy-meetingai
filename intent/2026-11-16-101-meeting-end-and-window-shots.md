---
status: draft
issue: 101
author: olafkfreund
---

# Intent: Notes finish when the meeting does, and screenshots show only the meeting

## Problem

Two failures in real use (owner, 2026-11-16):

1. **The notes do not finish when the meeting does.** After the call ends,
   meetingai keeps capturing audio and transcribes whatever happens next in the
   room or on the machine. (The normal finish is different: after a stop, the
   daemon stops capture at once and drains the queued transcription for up to
   15 s, `src/daemon/session.rs:3438`, `:3498`. The complaint is capture that
   never stops.)
2. **Auto screenshots capture the wrong thing.** #84 captures a fixed screen
   rectangle chosen with `slurp` (`spec/2026-11-05-84-auto-screenshots.md:33`).
   Once the owner switches workspace or opens another window there, that window
   is captured instead of the meeting. The owner asked for "only the call's
   window" in #84. It was dropped because `grim` was thought unable to isolate a
   window (`intent/2026-11-05-84-auto-screenshots.md`, question 2).

### Paths in the code that keep capture running (verified)

Which of these caused the owner's cases is not known. The next point explains
why.

- **A stream stays a call while it exists.** `nixarchy-meeting-mode`
  (`~/.config/omarchy/plugins/nixarchy.meeting-mode/Service.qml:133`) keeps a
  mic stream as a call for as long as that PipeWire stream exists. If an app
  holds the mic after the meeting, the call never ends. Teams is documented to
  do this, Granola documents the same effect, and Discord holds it in a voice
  channel.
- **Confirmation is not tied to a window.** Two things confirm a call: a
  meeting title in any browser window, or any screen share (`:69`, `:127`).
  That one flag is applied to every browser mic stream. A browser mic also
  becomes a call after 60 s with no confirmation at all (`maybeDelay`,
  `SettingsModel.js:19`).
- **Any other mic user keeps the call alive.** The detector takes the highest
  state across all qualifying streams (`Service.qml:129`). An idle Discord
  voice channel, or a second browser mic, keeps `call` on after the real
  meeting ends.
- **Loose patterns.** `\| Microsoft Teams` also matches "Chat | Microsoft
  Teams", and `^Zoom( Meeting| Workplace)?` matches an idle Zoom Workplace
  (`SettingsModel.js:18`).
- **Ownership is by timestamp.** The call stops notes automatically only when
  they started no more than 5 s before it (`shell/Service.qml:29`). Notes
  started earlier, for example by hand before joining, get only a "Stop notes?"
  notification. meeting-mode turns on Do Not Disturb during the call, which can
  hide it.
- **Accepted #64 limits fall back to the offer.** These all leave the notes
  running behind an offer (`spec/2026-11-03-64-auto-stop-on-call-end.md:105`,
  `shell/Service.qml:183`):
  - a shell restart during the 30 s countdown;
  - a restart after a takeover;
  - the detector dying while a stop is pending;
  - a detector restart mid-call: the new `callSince` replaces `ownerSince`
    (`shell/Service.qml:158`), so the running notes stop being the call's;
  - a recording started from the CLI or an agent with no shell running.
- **After the call, the system channel keeps hearing things.** meeting-mode
  resumes the media it paused (`resumeMedia`, `Service.qml:247`), and
  meetingai transcribes system audio. Music or a video after the call is
  transcribed, which is the owner's symptom.
- **No inactivity backstop.** Nothing ends a recording nobody is in a meeting
  for. A backstop based on "no transcript text" would not fire anyway while
  resumed media or room speech produces text.

Diagnosis is also hard:

- **No record of the lifecycle.** For ordinary shell requests, the daemon does
  not log recording start or stop, why they happened, or detector decisions.
  Only agent (MCP) requests are logged (`src/daemon/session.rs:5911`). So the
  history cannot say which path above applied.

On screenshots:

- **One window can be captured.** On p620, `grim` 1.5 (`-T <identifier>`)
  together with Hyprland 0.57's `stableId` (`hyprctl clients -j`) captured a
  window on a hidden workspace, with only that window's pixels.
- **Not yet proven:** that a Chrome or Meet window on a hidden workspace gives
  a *current* frame rather than an old one.
- **A window is not a tab.** Capturing a normal Chrome window shows whichever
  tab is active in it.

### What others do (research by Fable, Codex and agy; sources in the issue)

- **Mic release:** Granola, Circleback and Jamie end on mic release.
- **Inactivity:** Granola stops after 15 min with no audio. Otter stops after
  10 min with no conversation, after a 15 s countdown. Jamie adds the calendar
  end and a 5–15 min failsafe.
- **Countdown:** all of them show a cancellable countdown before stopping.
- **Linux reference:** Char/Hyprnote ignores corked (paused) capture streams.
- **Windows and tabs:** Recall.ai ties a meeting to a window ID, but cannot see
  a background browser tab either. Granola pauses snapshots when its window is
  not visible.
- **Chrome extensions:** an extension can read Meet's or Teams' in-call state.
  It cannot capture a background tab without a user gesture, and it does not
  cover native apps.
- **Unverified everywhere:**
  - Meet's and Teams' titles after leaving;
  - whether Chrome, Teams or Discord keep the mic open after leaving on this
    machine;
  - whether a hidden Chrome window gives current frames.

## Proposed outcome

Observable by the owner:

- **Capture stops when the meeting ends.** Within about a minute of leaving
  (the exact delays are for the spec), no new audio is captured. The notes then
  finish as they do today. This holds:
  - for notes started by hand for that call;
  - for apps that keep the mic open after the meeting.
- **Every stop is announced.** A stop is preceded by a visible countdown with
  **Keep recording**, which Do Not Disturb cannot hide.
- **The meeting is identified, or meetingai says it is not.** During a call,
  the shell shows which app and window it took as the meeting. When it cannot
  tell (several candidates, or no window), it says so. It never guesses
  silently.
- **Auto screenshots show only the meeting window,** wherever it is and
  whatever covers it, while the call is on. No capture is taken:
  - when the meeting window is unknown, or more than one window could be it;
  - when it may not be showing the meeting: it was closed, another tab is
    active in its browser window, or the call has ended (including during
    the stop countdown);
  - when its frames may be stale.

  The control says why. There is no automatic fallback to the screen, a
  rectangle or another window.
- **A forgotten recording does not run for hours.** How and when is question 2.
- **Wrong and missed stops can be explained afterwards.** The daemon log holds
  enough to tell these apart, without titles or transcript text:
  - the call detected, ended, or in an unknown state;
  - the notes owned, adopted or kept;
  - a countdown started, cancelled or completed;
  - a stop that was offered but not taken.
- **Older detector:** with a meeting-mode that lacks any new information,
  meetingai behaves as it does today.

## Affected users and systems

- **The owner,** and everyone in their meetings (screenshots).
- **meetingai:**
  - shell: `Service.qml`, `CallState.qml`, the bar widget;
  - daemon: recording stop, auto screenshots and logging in
    `src/daemon/session.rs`.
- **`nixarchy-meeting-mode`:** a second repository with its own gates. Any
  change to what it detects or writes to `meeting-mode.json` needs an approved
  intent → spec → plan there.
- **Hosts:** p620 and razer run the shell. Hyprland only.
- **Approved specs that change:**
  - #64: only owned notes stop, and its accepted restart limits;
  - #84: a rectangle picked with `slurp`.

## Constraints

- **A live meeting is never stopped silently.** Every automatic stop goes
  through the countdown. A lost window title, a detector restart, a quiet
  remote side or a failed transcription are not, on their own, evidence that
  the meeting ended.
- **Screenshot privacy rules from #84 stay:**
  - off by default and per meeting;
  - visible while on, with the count;
  - refused in Local-only, consent needed in interview mode;
  - never sent to a model;
  - skipped while the screen is locked;
  - capped, and deletable one at a time;
  - the export and sync disclosure;
  - agents (MCP) can neither start them nor read them, and any new window
    information gets the same treatment, including Local-only redaction.

  Capturing a hidden window removes "I can see what is being captured" as a
  safeguard, so the indicator matters more.
- **Logs and diagnostics never carry window titles, transcript text or
  images.** This includes any measurement done for this task.
- **The live audio path is not slowed.**
- **No new heavy dependency.** A browser extension would be a separate,
  explicit decision (question 3).
- **Two repositories:** each stays usable when the other is older.

## Open questions

1. **Notes running before the call.** If notes are already running when a call
   starts, are they the call's notes, stopping when it ends? **Recommended:**
   yes, with **Keep recording** as the escape. A room recording that should
   outlive a call then needs one click. The risk: a false call (an idle
   Teams chat, Zoom Workplace) would adopt a room recording and could stop it.
   So this needs the false-call paths above fixed first. The spec must show
   that it does.
2. **Inactivity.** This conflicts with "never stop a live meeting silently":
   a quiet webinar looks like a forgotten recording. The options:
   - **(a)** After a long stretch with no speech on either channel, show the
     countdown, then stop. Codex and Otter use 10 min; Granola uses 15 min.
   - **(b)** As (a), but only when no call is detected. During a detected call,
     only remind.
   - **(c)** Remind only, never stop.

   **Recommended:** (b), with a setting for the length and an Off choice.
   - **What counts as speech:** "no speech" means no voice detected. Missing
     transcript text, for example after a speech engine error, does not count.
   - **Resumed media:** music or a video after the call must not reset the
     clock. That needs a decision in the spec, such as counting only the mic
     once no call is detected.
   - **Keep recording:** choosing it restarts the clock. It does not switch the
     backstop off.
3. **A Chrome extension.** The researchers disagree:
   - **Codex:** a narrow, state-only extension (in call / left call) is worth
     it if Meet or Teams must be followed through tab switches.
   - **agy and Fable:** not now. It is a second detector to maintain against
     Meet and Teams DOM changes, and native apps still need the Linux path.

   **Recommended:** none in this task. The accepted limit: a meeting kept as a
   background tab in an ordinary browser window gets no screenshots while
   another tab is shown, and its end may be found later. Meetings in their own
   window or PWA work fully. Revisit if the measurement in question 4 shows
   the window and mic signals are not enough.
4. **Measuring before the spec.** Real behaviour decides the design and is
   unverified:
   - titles after leaving;
   - whether mic streams stay open, or only paused;
   - whether hidden-window frames are current.

   **Recommended:** one test call on p620 between the owner's own accounts or
   devices, with no other people: Meet in a tab, Meet as a PWA, Teams, and a
   Discord voice channel.
   - The owner joins and leaves.
   - A small script samples state and writes only the redacted fields below,
     so no agent reads raw titles or `hyprctl` output.
   - Kept: stream state, app name, window class, `stableId` and workspace, plus
     whether a title matched, never the title itself.
   - Hidden-window captures are viewed once to judge freshness, then deleted
     with the rest of the samples.
   - The findings go into the spec as text.
5. **Turning screenshots on.** #84 keeps auto screenshots off by default and
   switched on per meeting. **Recommended:** keep that, with the meeting window
   as the target instead of a drawn rectangle. Two choices are the owner's:
   - **Drawn rectangle:** keep it as an explicit, separate choice with #84's
     weaker guarantee, or drop it.
   - **On for every call:** turning it on automatically for every detected call
     would reverse #84's privacy decision.

   Also for the spec: the window closing and reopening, and Stop → Play on the
   same meeting, which keeps auto screenshots on today.
