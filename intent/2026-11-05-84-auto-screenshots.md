---
status: approved
issue: 84
author: olafkfreund
---

# Intent: Automatic screenshots during a meeting

## Problem

Slides, documents and shared screens carry much of a meeting, but the app keeps only what was said, plus any screenshot taken by hand with a key and a region. Nobody remembers to press it while listening,
so the notes lose the visual record (owner request, 2026-11-05). The owner asks for screenshots taken automatically, as a timelapse and on phrases such as "I will share my desktop" or "here is the document".

Parts exist, with limits: `grim` capture (`src/daemon/session.rs:1064`), which without a region takes **all monitors**; screenshots stored against a meeting (`src/db/repos/screenshots.rs`), which today are tied to a chat question, can only be deleted with the whole meeting, and normally go on to a model; and export to the meeting's `screenshots/` folder. The only automatic capture is `use_image_on_send`, at chat send. Nothing takes one unattended.

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
  - automatic screenshots are never sent to an AI model unless the user attaches one to a question. They stay out of the existing chat path (`pending_capture`, `src/daemon/session.rs:5618`), which sends images to the provider;
  - nothing is captured while the screen is locked;
  - the area captured is defined, never every monitor by accident.
- **Consent.** Capturing what others show may need the same consent as recording their voices. Interview mode already requires consent, and this must be covered by it.
- **Exports reach other tools.** Screenshots copied to an export folder may sync elsewhere, such as the owner's Obsidian vault, so this must be clear. Deleting a capture must be possible one at a time, and the spec says whether it also removes an exported copy.
- **Limits.** Storage and the number of captures per meeting are capped, and the live call must not slow down.
- **No new heavy dependency** unless needed.

## Open questions

1. **What triggers a capture?** The owner approved screen change plus phrases, plus a slow timelapse. **Both reviewers then advised against screen-change detection:** it was not asked for, needs frequent captures anyway, and a call's moving video looks like constant change. **Recommended now:** what the owner first asked for, a slow timelapse plus phrases with a delay, because a share appears several seconds after the words, with duplicates dropped when stored. The cost is that a slide shown without any phrase, between timelapse shots, is missed. **The owner decides.**
2. **What is captured?** `grim` works in screen coordinates, so it cannot isolate a covered window or one on another workspace. **Recommended:** the user picks the area once, when turning it on for a meeting, reusing `slurp`. It is simple and predictable, but it captures whatever is in that area. Automatic window finding is less predictable, and would have to skip captures when the window is covered. **This replaces "only the call's window", which is not reliably possible.**
3. **Which phrases and languages?** Recommended: a small English list first, in line with #71, since the phrases come from the transcript.
4. **Where do they show?** Recommended: with the meeting's other screenshots, and in the notes column from #81 if that lands first.
5. **On by default for anyone?** Recommended: never, including in Nix defaults, and per meeting, not a global setting that auto-record would start unattended.
