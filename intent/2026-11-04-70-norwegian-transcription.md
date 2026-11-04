---
status: draft
issue: 70
author: olafkfreund
---

# Intent: Calls in Norwegian are transcribed as Norwegian, locally

## Problem

A call held in Norwegian was transcribed as Swedish (owner report,
2026-11-04). The default engine cannot do Norwegian at all:

- **The model has no Norwegian.** The default speech engine is
  `parakeet-local`, NVIDIA `parakeet-tdt-0.6b-v3`. Its model card lists 25
  European languages, including Swedish and Danish but **not Norwegian**
  (`no`/`nb`/`nn`).
- **It picks the language itself.** Parakeet takes no language input, so
  Norwegian speech comes out as the closest language it knows.
- **The setting is ignored.** `speech_language` exists
  (`src/core/models/settings.rs`) and is passed to the OpenAI-compatible,
  Deepgram and custom engines (`src/daemon/session.rs` ~1227). The
  in-process Parakeet path never reads it, so setting `nb` changes nothing,
  and nothing warns the user.
- **It is invisible.** The shell has no visible control for the speech
  language.

The result is a transcript in the wrong language. The answers, notes,
recap, items and search built from it are then wrong too, silently.

History: #14 replaced Whisper with Parakeet because Whisper invents text from
quiet audio ("Amen", "Thank you for watching") and repeats its own prompt.
The obvious Norwegian-capable engines are Whisper-family, so that risk comes
back.

## Proposed outcome

1. **Norwegian works locally.** A Norwegian call is transcribed as Norwegian
   on this machine, so local-only interviews keep working.
2. **No silent wrong language.** When the chosen or detected language is one
   the active engine cannot do, the user is told before or as recording
   starts, and offered an engine that can. Transcribing as a neighbouring
   language is never silent.
3. **The language can be set.** The speech language can be chosen in
   Settings (Auto, or a fixed language). An engine that supports the choice
   uses it; one that does not leads to outcome 2.
4. **Nothing gets worse for the others.** Parakeet's 25 languages keep their
   current accuracy and speed.
5. **#14's guarantees hold.** Whatever engine handles Norwegian must not
   bring back invented text or speaker bleed. Its accuracy, latency and RAM
   are measured on p620 and razer, and approved by the owner, before it is
   the default for Norwegian.

## Affected users and systems

- Anyone holding calls in Norwegian, or in any language outside Parakeet's 25
  (for example Icelandic), on the default engine.
- `src/speech/stt/` (`parakeet.rs`, `transcription.rs`, engine selection),
  `src/daemon/session.rs` (`speech_config`, `record.start`),
  `src/core/models/settings.rs`, the shell Settings (`shell/Overlay.qml`),
  and Nix packaging (`nix/`) if a new local model or server joins the
  closure.
- p620 and razer: RAM and CPU for a second local model.

## Constraints

- **Local-first.** The Norwegian path must run on this machine. Deepgram
  (cloud) stays an option, not the fix.
- **No invented text** (#14). Silence and quiet audio must not produce
  phrases, and the engine's own output must not be fed back as a prompt that
  causes repeats.
- **Budget.** A second local model must not load unless it is used, and must
  fit alongside Parakeet on razer.
- **Packaging.** Any model is fetched by Nix with a pinned hash, like
  Parakeet and the speaker model, and its licence must allow it.
- **Settings.** Existing users' settings keep working. An empty
  `speech_language` still means Auto.

## Open questions

1. **Engine for Norwegian.** Recommended: measure NB-Whisper (National
   Library of Norway, Apache-2.0, trained on Norwegian) against Whisper
   large-v3-turbo, both local, on Norwegian test audio. Pick by accuracy,
   invented-text rate, latency and RAM.
2. **How to choose the engine.** Recommended: by the speech-language setting
   first (`nb`/`no`/`nn` uses the Norwegian engine); in Auto, by a cheap
   language check on the first speech, and switching only with a notice. The
   spec decides whether Auto detection is reliable enough to switch on.
3. **Norwegian test audio.** Recommended: a public, licensed set (for
   example FLEURS `nb_no`, or NB Tale) for the offline measurement, plus the
   owner's live check on a real call.
