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
  (`src/core/models/settings.rs`) and is copied into the provider config
  (`src/daemon/session.rs` ~1227).
  - The OpenAI-compatible and Deepgram paths, and custom raw and JSON
    bodies, send it.
  - The custom `multipart-wav` body drops it
    (`src/speech/stt/transcription.rs` ~308).
  - The in-process Parakeet path never reads it, so setting `nb` changes
    nothing, and nothing warns the user.
- **Codes and "Auto" are not normalised.** Only the OpenAI path filters
  "Auto", so the other paths can send `language=Auto`. Engines also expect
  different codes for Norwegian (`no`, `nb`, `nn`).
- **It is hard to find.** `speech_language` shows only in the generic
  settings search. There is no picker next to the speech engine.

The result is a transcript in the wrong language. The answers, notes,
recap, items and search built from it are then wrong too, silently.

History: #14 replaced Whisper with Parakeet because Whisper invents text from
quiet audio ("Amen", "Thank you for watching") and repeats its own prompt.
The obvious Norwegian-capable engines are Whisper-family, so that risk comes
back.

## Proposed outcome

1. **Norwegian works locally.** A Norwegian call is transcribed as Norwegian
   on this machine, so local-only interviews keep working.
2. **No silent wrong language for a set language.** When `speech_language`
   is set to a language the active engine cannot do, the user is told at
   `record.start` and `dictate.start`, before capture, and offered an engine
   that can.
   - **In Auto,** Parakeet has no language detector. The spec decides
     whether a language check runs on the first speech. If it does, a
     switch happens only with a notice, and lines already transcribed are
     never silently rewritten.
   - **Uncertain detection, a change of language mid-call, and mixed
     languages** (Norwegian with English terms) each get a defined,
     observable behaviour in the spec.
3. **The language can be set.** The speech language can be chosen in
   Settings, next to the speech engine (Auto, or a fixed language).
   - An engine that supports the choice uses it, with the code mapped to
     what that engine expects.
   - One that does not leads to outcome 2.
   - **Engine choice is the user's.** With the default local engine
     (`parakeet-local`), a Norwegian setting uses the local Norwegian engine
     automatically: still local, same class. A provider the user chose
     (Deepgram, OpenAI-compatible, custom) is never replaced; if it cannot
     do the language, outcome 2 applies.
   - The speech language does **not** change the answer and notes language
     (`response_language`), which stays its own setting.
4. **Dictation too.** `dictate.start`/`dictate.stop` use the same speech
   path and get the same behaviour.
5. **Nothing gets worse for the others.** Parakeet's 25 languages keep their
   current accuracy and speed.
6. **#14's guarantees hold.** Whatever engine handles Norwegian must not
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
- **No invented text and no bleed** (#14). Whisper-family engines produce
  phrases from non-speech; Parakeet returns nothing for it. The guarantee
  covers:
  - silence and quiet audio;
  - **short non-speech chunks**: the chunker sends any 0.2 s above -42 dBFS
    (`transcription.rs` ~389), so a cough, typing or chair noise reaches the
    engine;
  - **prompts on both channels**: #14 removed self-prompting on the mic
    only, and the system channel's prompt and the glossary
    (`filter.prompt`, ~1206) still reach OpenAI-style engines;
  - bleed and lost owner words through the `LineFilter` hold, if the new
    engine is slower than Parakeet.

  The measurement includes Norwegian silence, non-speech noise, remote-only
  speech, double talk and mixed-language speech. It counts invented lines,
  lost owner words and bleed alongside accuracy, as #14's
  `tools/accuracy/` does.
- **Budget.**
  - Parakeet stays loaded once used (about 1.1 GB, `parakeet.rs` ~28).
  - A second local model loads only when used.
  - Peak RAM with both, plus the speaker model, is measured on p620 and
    razer.
  - The spec decides whether the inactive engine is unloaded.
- **Local-only stays exact.** The local-only allowlists
  (`speech_config` ~1199, `stays_local` ~628) and the old-Whisper migration
  (`migrate_whisper` ~744, which rewrites `whisper-local`) must admit the
  new local engine deliberately, and never a cloud one.
- **One runtime if possible.** Parakeet and the speaker model run in
  sherpa-onnx in-process. Prefer a Norwegian engine with a sherpa-onnx
  Whisper export over a second native inference stack.
- **Packaging.** Any model is fetched by Nix with a pinned hash, like
  Parakeet and the speaker model, and its licence must allow it.
- **Settings.** Existing users' settings keep working. An empty
  `speech_language` still means Auto.

## Out of scope

- **Norwegian live triggers.** The automatic-answer heuristics only know
  English: `is_question` (~456) and `meaningful_remark` (~511), plus the
  English quick-action wording. A faithful Norwegian transcript still won't
  trigger answers the same way. It gets its own issue.
- **The meeting language column.** `meetings::start` stores `"en"`
  (~2564). Recording the real speech language there is a follow-up unless
  the spec needs it.

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

## Review record (draft)

| Finding | From | Verdict |
|---|---|---|
| "Never silent" cannot hold in Auto: Parakeet has no detector, and the first chunk is exposed | Codex, agy | confirmed → outcome 2 guaranteed for a set language; Auto behaviour decided in the spec |
| Engine selection could replace a user's chosen provider | Codex | confirmed → only the default local engine routes; the user's choice is never replaced |
| Custom multipart drops the language; "Auto" is sent literally; code mapping | Codex, agy | confirmed → problem and outcome 3 |
| #14's risk is wider: 0.2 s noise chunks, prompts on channel 1 and glossary, `LineFilter` timing | agy, Codex | confirmed → constraint and measurement cases |
| Parakeet stays resident; RAM | agy, Codex | confirmed → budget constraint |
| Answer/notes language is separate; English-only live triggers; meeting language `"en"` | Codex, agy | confirmed → outcome 3; out of scope (own issue) |
| Dictation has the same problem | agy | confirmed → outcome 4 |
| Local-only allowlists and `migrate_whisper` | agy | confirmed → constraint |
| The setting is in `settings.search`, not invisible | agy | confirmed → problem corrected |
| sherpa-onnx Whisper export preferred | agy | confirmed → constraint |

