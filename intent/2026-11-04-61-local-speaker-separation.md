---
status: draft
issue: 61
author: olafkfreund
---

# Intent: Tell remote speakers apart without sending audio anywhere

## Problem

With the default speech engine, Parakeet, which runs locally, every line
from the other side of a call has no speaker (`speaker: None`; the batch
path in `src/speech/stt/transcription.rs` ~1249). Only Deepgram, a cloud
service, separates speakers.

So in a local-only setup, which includes every local-only interview:

- The transcript shows everyone on the other side as one "Them". A panel
  interview, or a call with three colleagues, reads as one person talking to
  themselves.
- Answers and notes cannot say who said what. "The customer wants X" and
  "their engineer doubts X" collapse into one voice.
- Phase A of #60 lets the user name speakers, but with Parakeet only "Them"
  as a whole can be named.

The user has to choose between privacy (local) and knowing who spoke (cloud).

#60's approved intent moved this out (open questions 1 and 2): its RAM,
closure size and accuracy must be measured before committing to an approach.

## Proposed outcome

1. With Parakeet, remote lines carry a speaker number ("Speaker 1",
   "Speaker 2", …), so the transcript, answers, notes and exports separate
   the people on the other side, as they already do with Deepgram.
2. Those speakers can be named with phase A's rename, and the names show
   everywhere, as for Deepgram speakers today.
3. No audio or voice data leaves the machine. Nothing new is stored on disk
   beyond the speaker number on each line.
4. Where separation is unsure (one short line, crosstalk), the line stays
   unlabelled ("Them") rather than being given to the wrong person.
5. The cost is known and acceptable before anything ships: extra RAM,
   closure size, CPU per minute of audio, and how often lines are given to
   the wrong speaker, measured on real multi-speaker audio.

## Affected users and systems

- Everyone using Parakeet (the default) for calls with more than one remote
  speaker, and every local-only interview with a panel.
- `src/speech/stt/transcription.rs` (where remote lines get their speaker),
  `src/speech/voice_check.rs` (`SherpaEmbedder`, the CAM++ model already
  used for the owner voice check, #31), `src/speech/session/`, the daemon
  name cache (`session.rs`), `nix/speaker-model.nix`.
- p620 and razer (laptop, where RAM and CPU matter most).

## Constraints

- **Local-first.** It runs on this machine, with no network and no cloud
  model. Local-only interviews must be able to use it.
- **Live.** Lines are labelled as they are spoken (or within a few seconds),
  not only after the meeting. Any later relabelling must use the phase A
  `relabel` pattern, never a silent rewrite of stored text.
- **Budget.** The answer path must not get slower. Separation runs off the
  speech-to-text and answer threads, with bounded memory per recording.
- **Reuse before adding.** The CAM++ speaker-embedding model is already in
  the closure. A new model (for example a segmentation model) needs measured
  evidence that the existing one is not enough.
- **No regression for Deepgram users.** Their speaker labels keep coming
  from Deepgram.
- **Testable in CI without audio hardware.** The labelling decision must be
  unit-testable on fixed embeddings, and accuracy must be measured offline,
  as `examples/voice_eval.rs` does for the owner check.

## Open questions

1. **Approach.** Recommended: label each remote line by clustering CAM++
   embeddings of its audio online (nearest known speaker above a threshold,
   else a new speaker), reusing `SherpaEmbedder`. The spec measures it
   against a dedicated diarization pipeline (sherpa-onnx segmentation plus
   embedding) and picks the cheaper one that meets the accuracy bar.
2. **Accuracy bar.** Recommended: at most 1 line in 20 given to the wrong
   speaker on a 2–4-speaker test set, with unsure lines left as "Them" not
   counted as wrong. The spec sets the final number from the measurements.
3. **On by default?** Recommended: on for Parakeet once it meets the bar,
   with a setting to turn it off.
