---
status: draft
issue: 73
author: olafkfreund
---

# Intent: Find out how many words the call pipeline really loses, and recover them

## Problem

#70's gate suggested the call pipeline loses words: NB-Whisper small scored
14.4% word errors on single sentences and 23.9% on calls, with "336 owner words
lost". If that is real, every engine and every language transcribes worse on a
call than it could, and the transcript is what notes and answers are built on.

The evidence is weaker than the issue says. Nobody knows yet whether the loss is
real, or how large it is:

- **The two numbers are not like for like.** The 14.4% comes from 40 single
  sentences with clean audio. The 23.9% comes from 20 built calls with different
  sentences, plus microphone bleed and overlapping speech
  (`tools/accuracy/no_mix.py:595`). Some of the gap may be the sample or the
  harder audio, not the pipeline.
- **"Lost" does not mean dropped.** `lost` is `Counter(me)` minus the words in the
  clean microphone lines (`tools/accuracy/score.py:318`), so a misheard word counts
  as lost. Lost audio and ordinary recognition errors cannot be told apart from it.
- **The issue's "Parakeet and turbo show the same pattern" is not supported.** The
  gate has no single-sentence figure for either engine
  (`docs/eval/2026-11-norwegian-stt.md`, table row marked "—").
- **The test calls are built, not recorded.** Whether real calls behave the same
  has not been checked.

Candidates if the loss is real, none confirmed: `LineFilter`'s bleed hold dropping
owner lines (`src/speech/stt/filter.rs:115`); `BatchChunks` closing a chunk after
0.6 s of quiet or at 15 s (`src/speech/stt/transcription.rs:512`), which also gives
the model less context; and the VAD trimming the starts of words.

## Proposed outcome

- A like-for-like figure: the same audio scored with and without the pipeline,
  for the engines and languages we ship, separating words that never reached the
  model, words the filter removed, and words the model got wrong.
- If the loss is real and large enough to matter, it is fixed at its cause, and the
  transcript gets measurably better on the same test set.
- If it is mostly an artefact, this ends with the corrected figures, the scorer and
  docs fixed so they stop reporting it as loss, and no change to the pipeline.

## Affected users and systems

- Every call on every host and engine, since all local engines share this path.
- `tools/accuracy/` and `docs/eval/` for the measurement. Only if the loss is real:
  `src/speech/stt/filter.rs` and the chunking in `transcription.rs`.
- Dictation does not use `LineFilter`, and streaming Deepgram does not use
  `BatchChunks`; how far each candidate reaches is for the spec to establish.

## Constraints

- **#14's guarantees hold.** A fix must not bring back speaker bleed or invented
  lines. `LineFilter` exists to stop the other side's words appearing as yours, so
  losing a few owner words may be a fair price; any change there is judged on both
  numbers together.
- **No added delay.** Lines must not arrive later than today, on Parakeet above all.
  #74 separately covers Norwegian delay.
- **No engine or model change here.** This is about what happens around the model.
- **Measure before changing code.** No pipeline edit until the like-for-like figure
  exists and the owner has seen it.
- **Measuring waits.** The owner asked for no measurements on 2026-11-04, so it
  happens from 2026-11-05 at the earliest.

## Open questions

1. **How much loss is worth fixing?** Recommended: the spec proposes a threshold
   from what the measurement shows, and the owner decides at that gate. Setting a
   number now would be a guess.
2. **Which engines and languages?** Recommended: Parakeet on English, since most
   calls use it, and NB-Whisper small on Norwegian. Others only if those disagree.
3. **Should real call audio be part of the test?** Recommended: yes, if the owner
   can provide a recording with consent from everyone on it. Otherwise built calls
   only, with that limit stated in the result.
