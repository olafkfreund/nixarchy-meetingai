---
status: approved
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
  sentences; the 23.9% comes from 20 built calls of different sentences
  (`tools/accuracy/no_mix.py:887`). Those calls have no bleed or overlap, which
  only the separate double-talk set adds (`no_mix.py:601`), so harder audio does
  not explain the gap. What is left is the sample, or the pipeline.
- **"Lost" does not mean dropped.** `lost` is a word count: `Counter(me)` minus the
  words in the microphone lines the scorer judges clean
  (`tools/accuracy/score.py:318`). A misheard word counts as lost, and so does every
  owner word in a line it judges to be bleed. It also covers the owner's channel
  only, while the 23.9% covers both (`score.py:346`), so the two are not views of
  the same thing.
- **The issue's "Parakeet and turbo show the same pattern" is not supported.** The
  gate has no single-sentence figure for either engine
  (`docs/eval/2026-11-norwegian-stt.md`, table row marked "—").
- **The test calls are built, not recorded.** Whether real calls behave the same
  has not been checked.

Candidates if the loss is real, none confirmed and not a complete list:

- **`LineFilter` dropping lines** (`src/speech/stt/filter.rs`). The bleed hold only
  delays lines; drops come from the echo check and the repeat rules. The echo
  check's stop words are English only (`filter.rs:36`), so Norwegian function words
  count as content, which could make ordinary Norwegian look like an echo. The
  repeat rules can drop a second "ja" within 10 s (`filter.rs:190`).
- **`BatchChunks` cutting audio** (`src/speech/stt/transcription.rs:512`). It closes
  a chunk after 0.6 s of quiet or at 15 s, which gives the model less context. It
  keeps nothing from before the first loud sample, so soft word starts can be cut
  (`transcription.rs:608`). It also discards chunks with under 0.2 s of sound
  (`transcription.rs:575`).
- **The VAD** trimming word starts in the Whisper engines, on top of the above.
- **Dropped chunks** when the batch queue backs up or a request fails, which show a
  notice.

## Proposed outcome

- A like-for-like figure: the same audio scored with and without the pipeline, for
  Parakeet on English and NB-Whisper small on Norwegian, separating words that
  never reached the model, words the filter removed, and words the model got wrong.
  Other engines and languages are not claimed unless measured.
- If the loss is real and passes the threshold the spec sets before any fix is
  judged, it is fixed at its cause, and the same test set shows it.
- If it is mostly an artefact, this ends with the corrected figures, the scorer and
  docs fixed so they stop reporting it as loss, and no change to the pipeline.

## Affected users and systems

- Calls on every host through a non-streaming engine: Parakeet, Norwegian Whisper,
  and HTTP batch providers (`transcription.rs:872`). Streaming Deepgram does not use
  `BatchChunks`.
- `tools/accuracy/` and `docs/eval/` for the measurement. Only if the loss is real:
  `src/speech/stt/filter.rs` and the chunking in `transcription.rs`.
- Dictation does not use `LineFilter`. How far each candidate reaches is for the
  spec to establish.

## Constraints

- **#14's guarantees hold.** A fix must not bring back speaker bleed or invented
  lines. `LineFilter` exists to stop the other side's words appearing as yours, so
  losing a few owner words may be a fair price; any change there is judged on both
  numbers together.
- **No added delay.** The time from speech to a line appearing must not grow, on
  Parakeet above all; the spec sets the baseline. #74 separately covers Norwegian
  delay.
- **No engine or model change here.** This is about what happens around the model.
- **Measure before changing code.** No pipeline edit until the like-for-like figure
  exists and the owner has seen it.
- **Measuring waits.** The owner asked for no measurements on 2026-11-04, so it
  happens from 2026-11-05 at the earliest.

## Open questions

1. **How much loss is worth fixing?** Recommended: the spec proposes a threshold
   from what the measurement shows, and the owner decides at that gate. Setting a
   number now would be a guess.
2. **Which engines and languages?** Recommended: the two pairs in the outcome.
   Parakeet on English is the owner's default path, and NB-Whisper small on
   Norwegian is where the question arose. Polish or others only if asked for.
3. **Should real call audio be part of the test?** Recommended: yes, if the owner
   can provide a recording with consent from everyone on it. Otherwise built calls
   only, with that limit stated in the result.
