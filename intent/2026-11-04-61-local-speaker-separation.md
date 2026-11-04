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
   - Identity holds **within one recording**.
   - When a meeting is resumed, new voices continue the numbering after the
     highest stored number, as `session_restore` does today
     (`speaker_offset`).
   - A returning voice is not matched to its old number, because no voice
     prints are stored (outcome 3). The user can rename it again.
3. No audio or voice data leaves the machine. Nothing new is stored on disk
   beyond the speaker number on each line.
4. Where separation is unsure, the line stays unlabelled ("Them") rather
   than being given to the wrong person. That covers a short line, crosstalk,
   and a chunk with two voices in it (one line-level embedding cannot split
   it).
5. The cost is known, and approved by the owner, before anything ships. It
   is measured on p620 and razer:
   - extra RAM and CPU per minute of audio;
   - closure growth;
   - **errors**: lines given to the wrong speaker;
   - **coverage**: lines labelled at all.

   Labelling nothing must not pass. The test set includes compressed call
   audio from the system channel, short turns, crosstalk and mixed-speaker
   chunks, not only clean read speech.

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
- **Live.** A line is labelled within a few seconds of its speech chunk
  closing, not only after the meeting. With Parakeet a chunk closes after
  0.6 s of quiet or 15 s of continuous speech
  (`src/speech/stt/transcription.rs` ~482/532), so the limit is measured from
  the chunk's end.
  - If a speaker is assigned after the line was final, the stored row, the
    live view and the names must agree.
  - That relabel must identify the line by its id and carry the speaker
    number and key.
  - Today's `transcript.relabel` carries only `who` and times, so it is not
    enough.
  - Never a silent rewrite of stored text.
- **Budget.** Separation runs off the speech-to-text and answer threads, with
  bounded memory per recording, and must leave untouched:
  - answer latency;
  - the owner voice check: the same `me`/`room` results, no added delay.
    That check already loads the same embedder on a blocking worker
    (`session.rs` ~2951).
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
2. **Accuracy bar.** Recommended, on a 2–4-speaker test set of real call
   audio:
   - at most 1 labelled line in 20 given to the wrong speaker;
   - at least 4 in 5 remote lines of 1.5 s or longer labelled (the owner
     check needs 1.5 s of voiced audio to score, `voice_check.rs` ~186).

   The spec sets the final numbers from the measurements. The CAM++ figures
   in `voice_check.rs` are for owner versus room on the microphone, so they
   do not show that clustering remote voices works. That is what the
   measurement must establish.
3. **On by default?** Recommended: decide after the measurements. On by
   default only if errors, coverage, latency and resource use all meet the
   bar, with a setting to turn it off.
4. **Remembering voices across resumes.** Recommended: no. Keeping voice
   prints would be new stored biometric data. Numbering continues instead
   (outcome 2).

## Review record (draft)

| Finding | From | Verdict |
|---|---|---|
| Speaker identity across a resumed meeting is unresolved | Codex | confirmed → outcome 2: within one recording; numbering continues; open question 4 |
| "1 in 20 wrong" passes if nothing is labelled; CAM++ figures are owner-check, not clustering | Codex | confirmed → coverage bar plus a real call-audio test set (outcome 5, question 2) |
| "Live" needs a limit; Parakeet chunks can run 15 s; mixed-speaker chunks | Codex | confirmed → latency measured from the chunk's end; mixed chunks stay unlabelled |
| `transcript.relabel` cannot carry a numbered speaker | Codex | confirmed → the constraint requires line id, number and key |
| Resource bar open-ended; the owner check shares the embedder | Codex | confirmed → owner-approved numbers on p620 and razer; the owner check is untouched |

