---
status: approved
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

For each pair, score the same turns of the same calls:

| Run | Path | What it adds |
|---|---|---|
| **A: whole turns** | each turn's exact audio, cut from the call, sent whole through `transcribe` | the baseline: the engine as shipped, given a complete turn |
| **B: chunks** | the call through `BatchChunks` and `transcribe_chunk`, with no filter; the decoded chunks are saved | A → B is the cost of processing chunks instead of whole turns |
| **B+F: chunks, filtered** | B's saved lines fed through `LineFilter` offline, in finish order | B → B+F is what the filter costs, on identical decoding |
| **C: production** | the call through `LiveTranscriber` at 1× | checks that B+F matches what production really does |

- **Filter is the only changed step.** B+F reuses B's decoded text, so a decoding or
  request-handling difference cannot be charged to the filter. Any B+F → C difference is
  reported on its own: it means timing matters, which is itself a finding.
- **No production change.** `BatchChunks`, `transcribe_chunk` and `filter::LineFilter` are
  public, so `examples/accuracy_replay.rs` gets the modes it needs. B has no wall-clock
  dependency, so it runs as fast as it decodes, with the same tail silence as C.
- **A → B is not purely "audio cut off".** It also includes lost context and, on
  NB-Whisper, the VAD running on chunks whose lead-in was already removed. It is reported
  as the cost of chunking as a whole.
- **Pairs (approved in the intent):** Parakeet on English, NB-Whisper small on Norwegian.
- **Speed.** The #70 call runs used `--speed 8`. `LineFilter` runs on the audio clock while
  lines arrive by wall clock. At 8× most lines can arrive after the audio has ended, when
  they are held until the final flush and are checked against system lines that would not
  have arrived yet at 1×. C at 8× is compared with C at 1× on the Norwegian set, so #70's
  figures can be explained.
- **VAD (NB-Whisper only).** `examples/whisper_spike.rs` only loads `turbo-*` files on 4
  threads, so it gets model-file and thread arguments. It then runs A's turns on the same
  NB-Whisper model with no VAD. This shows the VAD's cost on whole turns only.

### S2. Test sets and scoring

- **Norwegian:** the 20 NPSC calls from #70 (`no_mix.py`), with no bleed or overlap.
- **English:** the same call shape from LibriSpeech test-clean. `build_calls` gets a
  LibriSpeech loader; `libri_mix.py` is not reused because its calls have bleed.
- **Guard sets for English:** double-talk and remote-only sets built the same way, because
  Parakeet's bleed cannot be judged on Norwegian text it writes as Swedish.
- **Turn data.** `build_calls` also writes each turn's channel, start, end and text next to
  the call, so run A cuts the same audio. Seeds stay fixed.
- **A second sample with different sentences.** Another seed is not enough: `load_npsc` keeps
  every short and code-switched sentence whatever the seed (`no_mix.py:543`), and can return
  the same pool. The second sample leaves out every sentence ID used in the first, and the
  build fails if they overlap.
- **`score.py` reports per channel:** word errors for `me`, `them` and both, plus per-call
  differences between runs. The filter's echo check only touches the `me` channel, so a loss
  there would be diluted in an overall figure.
- **`lost` is renamed `missing words`**, and its description says it includes misheard words
  and lines judged as bleed.
- **Audio not reaching the model.** Run B reports turn audio covered by no chunk. This is an
  upper bound in seconds, since it includes silence before the first word.
- **Removed lines.** B and B+F have identical timestamps, so each line the filter removed is
  listed exactly, with its text, channel and rule where the filter's code makes that visible.
- **Notices.** The replay discards `LiveEvent::Notice` today (`accuracy_replay.rs:408`). It
  starts printing them, and the scorer reports them. A lost-speech or failed-chunk notice
  invalidates that run.

### S3. Decision rule, set before the numbers exist

The runs in a sample use identical audio, so a difference between them is not caused by the
choice of sentences. What the second sample tests is whether a cause holds on other
sentences. The owner sees every number and decides; the proposal is:

| Result | Meaning | Next |
|---|---|---|
| A cause costs **2 points or more on the channel it affects**, worse in most calls, in both samples | real, worth fixing | S4: design a fix for it |
| No single cause does, but the **whole pipeline** (A → C) costs 2 points or more in both samples | real but spread out | owner decides whether smaller fixes are worth it |
| The whole pipeline costs under 2 points, or the samples disagree | not worth fixing now | S5: correct the record and stop |

Two points is a priority line, about one word in 50, not proof that a smaller loss does not
exist. Smaller losses stay listed in the results. The word "artefact" is used only for a gap
that the runs show is not from the pipeline, such as the speed or the sentence sample.

### S4. If a cause is real

The fix for each cause gets a new section here, and the owner approves it before any code
changes. Candidates from the intent, none chosen: keeping a little audio from before the first
loud sample; echo-check stop words that are not English only; narrower repeat rules; different
chunk limits.

**Every fix must pass these guards against today's build, on both pairs unless stated:**

- bleed lines and remote words on the microphone channel, on the double-talk and remote-only
  sets: no more than today;
- invented lines on the silence, room-tone and burst sets, and phantom phrases on any set: no
  more than today;
- short words kept: no fewer than today;
- speech-to-line time at 1×, per channel, because the bleed hold makes the two channels very
  different: median and p95 no more than 0.1 s above today (`score.py` starts computing this
  from `released_at`, since it reports only per-chunk decode time now);
- chunk decode time on NB-Whisper: no more than 0.1 s above today at the median;
- the gain in S1's score holds in both samples.

Bleed and invented lines are #14's guarantees, so a fix that gains words but fails either is
not acceptable.

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
- **Sentence supply.** Two samples with no shared sentences may not be possible at this size
  for every set; if the pool runs out, the build fails rather than reusing sentences.
- **Time.** At 1×, each C run takes about 21 minutes per sample, so a few hours in all, though
  pairs can run side by side. Speech-to-line time needs a run with nothing else loading the
  machine.
- **razer is not measured.** p620 only, as in #70.

## Verification

- `cargo test --workspace`, clippy and fmt pass; `shell/tests/ci.sh` is untouched by
  this work.
- `score.py` and `no_mix.py` changes have small self-checks: per-channel totals add up
  to the overall figure, and turn data matches the audio length.
- **Sanity checks before trusting a result:** rebuilding a set with the same seed gives
  identical audio; the two samples share no sentence ID; B+F and C agree on a Parakeet English
  sample, or the difference is explained; and C at 1× on Parakeet English reproduces itself
  within 1 point.
- `git diff` shows no change under `src/` before the S3 gate.
