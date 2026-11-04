---
status: draft
issue: 73
intent: intent/2026-11-04-73-call-pipeline-word-loss.md
---

# Spec: Measure the call pipeline's word loss, then decide

This spec covers the measurement and the decision rule. It does not design a fix:
a fix, if the numbers call for one, gets its own spec section and approval (S4).
No production code changes until then.

**Nothing here runs on 2026-11-04.** The owner asked for no measurements that day.

## Design

### S1. One set of audio, three ways

For each pair, score the same turns of the same calls three ways:

| Run | Path | What it adds |
|---|---|---|
| **A: model alone** | each turn's exact audio, cut from the call, sent whole through `transcribe` | the baseline: what the engine does with a complete turn |
| **B: chunking** | the call through `BatchChunks` and `transcribe_chunk`, no filter | A → B is what chunking costs |
| **C: full pipeline** | the call through `LiveTranscriber`, as in production | B → C is what `LineFilter` costs |

- **Pairs (approved in the intent):** Parakeet on English, NB-Whisper small on
  Norwegian.
- **Run B needs no production change.** `BatchChunks` and `transcribe_chunk` are
  already public, and `examples/accuracy_replay.rs` already imports the former. It
  gets a `--no-filter` mode that drives them directly.
- **Real speed.** B and C run at 1×. The #70 call runs used `--speed`, which feeds
  audio faster than decoding keeps up. `LineFilter` runs on the audio clock while
  lines arrive by wall clock, so speed can change what it does. This alone may
  explain part of the gap, so a C run at the old speed is compared with C at 1× on
  the Norwegian set.
- **VAD (NB-Whisper only).** One extra run of A without the VAD, on the existing
  `examples/whisper_spike.rs` path, shows what the VAD costs. Parakeet has no VAD.

### S2. Test sets and scoring

- **Norwegian:** the 20 NPSC calls from #70 (`no_mix.py`), about 3,200 words, with
  no bleed or overlap.
- **English:** the same call shape from LibriSpeech test-clean, which is already
  downloaded. `no_mix.py`'s `build_calls` gets a LibriSpeech loader; `libri_mix.py`
  is not reused because its calls have bleed and double talk.
- **Turn data.** `build_calls` also writes each turn's channel, start, end and text
  next to the call, so run A can cut the same audio. Seeds stay fixed.
- **A second sample.** Each set is built a second time with another seed, so a
  difference is not just the choice of sentences.
- **`score.py` reports per channel:** word errors for `me`, `them` and both. A, B and
  C are compared on sequence edit distance over the same reference.
- **`lost` is renamed `missing words`**, and its description says it includes
  misheard words and lines judged as bleed.
- **Audio never reaching the model.** Run B reports the seconds of turn audio covered
  by no chunk. This is a duration, not a word count, and is reported as that.
- **Removed lines.** Each line in B with no counterpart in C is listed with its text
  and channel, so what the filter took out can be read directly. Line matching is by
  time overlap, so some lines are expected to differ for reasons other than the
  filter; this is a reading aid, and the score comes from S1.
- **Queue drops.** The replay reports `lost_speech_notice` events. None is expected:
  each call is about a minute and the cap is 120 s of queued audio. A non-zero count
  invalidates that run.

### S3. Decision rule, set before the numbers exist

The owner sees the numbers and decides. The proposed rule:

| Result | Meaning | Next |
|---|---|---|
| A cause costs **2 points or more** of word errors on a pair, in both samples | real, and worth fixing | S4: design a fix for that cause |
| Every cause is under 2 points, or the samples disagree | noise or an artefact | S5: correct the record and stop |

Two points is about one word in 50. On 3,200 words a difference smaller than that is
hard to tell from the choice of sentences, and that is why the rule also needs both
samples to agree. It is not a statistical test.

### S4. If a cause is real

The fix for each cause gets a new section here, and the owner approves it before any
code changes. Candidates from the intent, none chosen: keeping a little audio from
before the first loud sample; echo-check stop words that are not English only;
narrower repeat rules; different chunk limits.

**Every fix must pass the same guards, measured against today's build:**

- bleed lines on the double-talk and remote-only sets: no more than today;
- invented lines on the silence, room-tone and burst sets: no more than today;
- speech-to-line time at 1× (`released_at` minus the line's end): median and p95 no
  more than 0.1 s above today, on Parakeet;
- the gain in S1's score holds in both samples.

Bleed and invented lines are #14's guarantees, so a fix that gains words but fails
either is not acceptable.

### S5. If it is mostly an artefact

- Correct `docs/eval/2026-11-norwegian-stt.md`, which calls this "about 10 points".
- Keep the `score.py` changes from S2.
- Close #73 with the figures. No pipeline change.

In either outcome, S1's results go in `docs/eval/2026-11-call-pipeline.md`.

### S6. Real call audio (optional, owner only)

Approved only if the owner provides it. `tools/accuracy/record.sh` already captures
what the transcriber receives, with everyone's consent, and the audio stays out of git
and off cloud engines. It gets runs B and C and a hand-written reference. Without it,
the result states that only built calls were tested.

## Alternatives rejected

- **Compare 14.4% with 23.9% again, or with published figures.** Different sentences;
  this is the flaw that started the question.
- **Try the likely fixes and see.** Several interact, each needs its own guards, and a
  fix for an artefact would add risk for no gain.
- **A production switch to bypass the filter.** The example can already do it, and a
  switch that removes bleed protection should not exist in the daemon.
- **Word-level tracing through the pipeline.** More exact, but far more code, and A/B/C
  answers the question being asked.
- **Measuring every engine and language.** Not needed for the decision; the intent
  limits the claim to the two pairs.

## Risks

- **Built calls may not behave like real ones.** The turns are clean, with a 0.8 s gap
  between them. Real speech has more hesitation, so chunking may cost more than this
  shows, and the test may understate it. S6 is the only check.
- **LibriSpeech is read aloud, not conversation.** The English result is the weaker of
  the two.
- **Run A is not free of chunking.** Parakeet's own splitting and NB-Whisper's VAD still
  run inside `transcribe`. A is "the engine as we ship it, given a whole turn", not the
  bare model.
- **Sample size.** S3's rule is a judgement, not a significance test.
- **Time.** At 1×, each B or C run takes about 21 minutes per sample, so a few hours in
  all, though pairs can run side by side. Speech-to-line time needs a run with nothing
  else loading the machine.
- **razer is not measured.** p620 only, as in #70.

## Verification

- `cargo test --workspace`, clippy and fmt pass; `shell/tests/ci.sh` is untouched by
  this work.
- `score.py` and `no_mix.py` changes have small self-checks: per-channel totals add up
  to the overall figure, and turn data matches the audio length.
- **Sanity checks before trusting a result:** with `--fake`, A, B and C score the same
  on a line that passes through unchanged; rebuilding a set with the same seed gives
  identical audio; and a C run at 1× on Parakeet English reproduces itself within 1
  point.
- `git diff` shows no change under `src/` before the S3 gate.
