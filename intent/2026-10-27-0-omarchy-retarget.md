---
status: approved
issue: none (issues are disabled on olafkfreund/mixarchy-meetingai)
author: olafkfreund
---

# Intent: Retarget lennette to an Omarchy-native meeting notes copilot

## Problem

The repo is a fork of lennette (~34k lines of Rust + GPUI, version 0.18.23).
It was built as a cross-platform product, and much of it is not needed here:

- **Screen-share hiding** is a large feature we do not want: the
  `private_overlay` Hyprland compositor patch (`packaging/hyprland/`, 1.2k
  lines), its smoke tests (`tools/src/hyprland/`, 1.5k lines), Windows
  `WDA_EXCLUDEFROMCAPTURE` (`src/platform/display/window.rs:373`), and the
  "Go stealth / Privacy mode" tray, CLI and settings paths.
- **Windows support** is spread over 13 files under `#[cfg(windows)]`, plus
  `packaging/windows/`, `docs/windows.md`, `build.rs`, and the dependencies
  `windows-sys` and `embed-resource`.
- **Marketing and release surfaces** that are not for us: the `web/` site
  (4.4k lines of TS), and `tools/src/release.rs`, which requires Windows
  artifacts.
- **Providers do not match ours.** The bundled chat providers are Gemini and
  Merge Gateway (`src/ai/catalog/providers.rs:28`). Claude and OpenAI exist
  only as custom presets. Ollama is explicitly retired by a test
  (`providers.rs:219`). The native Anthropic path sends `Authorization:
  Bearer` instead of `x-api-key` (`src/ai/chat/engine.rs:279`), so bundled
  Claude would fail.
- **It does not look or behave like Omarchy.** The "System" theme is
  hard-coded dark (`src/ui/theme/tokens.rs:206`). It ships its own tray icon
  (`ksni`) instead of a bar widget, and its own Hyprland packaging instead of
  plain window rules and keybindings.
- **Notes from long meetings lose the middle.** The recap reads a
  120,000-character excerpt of the beginning and the end
  (`src/ai/recap.rs:17`, `src/db/repos/meetings.rs:259`), so decisions made
  mid-meeting can be missing.
- **Speech-to-text requires the cloud.** Deepgram is the only STT provider,
  and a dropped connection stops recording until it is restarted by hand
  (`src/speech/stt/transcription.rs:681`).

## Proposed outcome

A Linux-only, Omarchy-native app for live meeting, interview and discussion
notes:

- **Kept unchanged in behavior:** live transcripts from both the mic and
  system audio, with separate speaker context; streamed answers with context
  (prompt, profiles, past conversations, screenshots, PDF/text/image
  attachments); General and Interview modes; editable answer and follow-up
  prompts; a cached live model picker; "No context"; Fast, Balanced and
  Relaxed pacing; the composer (grows to five lines, attachments,
  Shift+Enter); New, Restore and Quit from the pill; restored recordings
  append to the same meeting; Settings search; Reduce motion.
- **Providers:** Claude, OpenAI (Codex models), Gemini, and local Ollama (no
  key, `localhost`). Merge Gateway is removed.
- **Looks and behaves like Omarchy:** colors and fonts come from the active
  Omarchy theme and follow theme changes. A Quickshell bar widget shows
  recording state and starts and stops recording. Launching goes through
  Walker or the Omarchy menu. Placement uses Hyprland keybindings and window
  rules.
- **Notes you can trust:** notes are built up during the meeting from
  time-stamped, speaker-labelled turns, then consolidated at the end. They
  cover the whole meeting, and the full transcript stays as the source.
  Interviews get a fitting note shape, for example question, answer and
  assessment.
- **Optional local STT:** whisper.cpp, already used by `nixarchy-voice`,
  works offline next to Deepgram. Whether it covers only final notes or also
  the live transcript is decided in the spec.
- **Gone:** screen-share hiding, Windows, the web site, the release tooling,
  the tray icon, and the compositor patch.

## Affected users and systems

- One user (the owner) on nixarchy (Omarchy on NixOS, Hyprland, Quickshell
  0.3.2 omarchy-shell).
- Data: existing encrypted history (`*.db` + `.key` sidecar) and the
  credential vault must still open after the change.
- Related repos that could be reused: `nixarchy-meeting-mode` (detects when a
  call starts, which could auto-start recording) and `nixarchy-voice`
  (whisper.cpp packaging on NixOS).

## Constraints

- Must not lose or corrupt existing meeting history or saved keys.
  Retired-provider records must keep failing closed.
- Keys must not be sent over plain HTTP. Allow `http://` only for loopback
  (Ollama).
- Remove each deleted feature completely: code, settings fields, shortcuts,
  CLI flags, tests and dependencies. Leave nothing dead behind.
- Packaging is a Nix flake or devenv for nixarchy. No Arch/AUR, no Windows
  installer.
- License: `LICENSE` is AGPL-3.0 but `Cargo.toml` says MIT. Make them agree,
  and keep the upstream attribution.

## Open questions

1. **Quickshell or GPUI for the main UI?** You asked for both "Quickshell as
   the UI" and "built with Rust + GPUI". On this machine the Omarchy shell is
   itself Quickshell with QML plugins.
   - (a) **Hybrid (recommended):** keep the Rust + GPUI app as the main UI,
     themed from Omarchy, and add a small Quickshell plugin (bar widget plus
     a quick panel) that drives it over the existing control socket
     (`src/platform/shortcuts/control.rs`). Only a status feed back to the
     widget is new.
   - (b) **Full Quickshell:** Rust becomes a headless daemon (audio, STT,
     AI, DB), and about 18.7k lines of GPUI UI are rewritten in QML. That
     needs an IPC contract for transcript deltas, streamed tokens,
     attachments and settings. This is several times the work.
   - Codex recommends (a). agy gave no answer (see below).
2. **Claude and Codex: API keys or subscriptions?** API keys fit the existing
   engine. Using your Claude or ChatGPT *subscription* would mean driving the
   `claude` and `codex` CLIs as subprocesses, which is slower and has no
   image or streaming parity. The recommendation is API keys, plus Ollama for
   local.
3. **Local STT scope:** whisper.cpp only for the final notes (cheap), or also
   live, replacing Deepgram for fully offline use (needs GPU-backed
   streaming)?
4. **Auto-start on calls:** should `nixarchy-meeting-mode` call detection
   start recording automatically, or only offer to?
5. **Rename?** Keep the name `lennette`, or rename it to fit the repo
   (`mixarchy-meetingai`)?

## Review inputs (read-only, advice only)

- Codex (model `gpt-6-sol`): its findings are the basis of the Problem list.
  Each cited line was checked against the code and confirmed.
- agy (model not reported): no review was obtained. It returned only its
  opening plan, with no findings.
