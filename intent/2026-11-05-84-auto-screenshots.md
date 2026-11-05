---
status: draft
issue: 84
author: olafkfreund
---

# Intent: Automatic screenshots during a meeting

## Problem

Slides, documents and shared screens carry much of a meeting, but the app keeps only what was said, plus any screenshot taken by hand with a key and a region. Nobody remembers to press it while listening,
so the notes lose the visual record (owner request, 2026-11-05). The owner asks for screenshots taken automatically, as a timelapse and on phrases such as "I will share my desktop" or "here is the document".

The pieces exist: `grim` capture (`src/daemon/session.rs:1064`), screenshots stored against a meeting (`src/db/repos/screenshots.rs`), and export to the meeting's `screenshots/` folder. Nothing decides when to take one.

## Proposed outcome

- A person can switch this on for a meeting and afterwards find the shared slides and documents among the meeting's screenshots, without having pressed anything.
- It captures moments that matter and not hundreds of near-identical frames.
- It is obvious while it is on, and easy to stop, review and delete.
- Nothing about a meeting changes when it is off, which is the default.

## Affected users and systems

Anyone who turns it on, and everyone in their meetings, whose faces and shared content may be captured. The daemon's recording path, the screenshot store, export, the transcript line path if phrases are used, and the shell. Possibly a Home Manager option.

## Constraints

- **Privacy comes first.** An unattended capture can catch other windows, messages, notifications and passwords, and whatever others share, which may be confidential. So:
  - off by default, and a deliberate choice per meeting or by a clear setting;
  - clearly visible while active;
  - never in a Local-only interview, unless explicitly chosen for it;
  - automatic screenshots are never sent to an AI model unless the user attaches one to a question.
- **Consent.** Capturing what others show may need the same consent as recording their voices. Interview mode already requires consent, and this must be covered by it.
- **Exports reach other tools.** Screenshots copied to an export folder may sync elsewhere, such as the owner's Obsidian vault, so this must be clear, and deletion must be possible.
- **Limits.** Storage and the number of captures per meeting are capped, and the live call must not slow down.
- **No new heavy dependency** unless needed.

## Open questions

1. **What triggers a capture?** Recommended: screen change plus phrases, with a slow timelapse as a floor. Change detection catches each new slide with no words needed, phrases raise attention, and the timelapse fills gaps. The owner asked for timelapse and phrases; change detection is an addition.
2. **What is captured?** Recommended: only the call's window, if it can be found, and otherwise nothing, not the whole screen. It has not been verified that this is reliable on Hyprland, and a call in a browser tab cannot be told apart from the rest of that window.
3. **Which phrases and languages?** Recommended: a small English list first, in line with #71, since the phrases come from the transcript.
4. **Where do they show?** Recommended: with the meeting's other screenshots, and in the notes column from #81 if that lands first.
5. **On by default for anyone?** Recommended: never, including in Nix defaults.
