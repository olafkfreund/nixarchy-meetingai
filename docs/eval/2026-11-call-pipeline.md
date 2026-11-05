# Call pipeline word loss: measurement (#73)

Measured on p620, 2026-11-05, Parakeet TDT 0.6B v3 on English, with `tools/accuracy/` and `examples/accuracy_replay.rs`.
Two samples of 20 built calls from LibriSpeech test-clean, sharing no sentence (200 each). No bleed or overlap in these calls.

## Result

**On clean built calls in English, the pipeline costs Parakeet under 1 point of word errors, and the filter costs nothing.**
The "about 10 points" seen in #70 does not appear here.

| Run | Sample 1 (3,424 words) | Sample 2 (3,190 words) |
|---|---|---|
| A: whole turns | 3.7% | 2.5% |
| B: chunks, no filter | 3.7% | 3.3% |
| B+F: chunks, filter offline | 3.7% | 3.3% |
| C: production, real speed | 3.7% | 3.3% |

| Cost | Sample 1 | Sample 2 |
|---|---|---|
| Chunking, A → B | +0.1 points (me +0.2, them −0.1); worse in 8 of 20 calls | +0.8 points (me +0.4, them +1.2); worse in 11 of 20 calls |
| Filter, B → B+F | 0; no line removed | 0; no line removed |
| Timing, B+F → C | 0; identical scores | 0; identical scores |

No run was invalid, and no lost-speech notice appeared.

## Against the decision rule

The approved rule: a cause of 2 points or more on the channel it affects, worse in most calls, in both samples, means design a fix. A whole
pipeline of 2 points or more means the owner decides. **Under 2 points means correct the record and stop.**

The whole pipeline costs 0.1 and 0.8 points, and the largest single figure is 1.2 points on one channel in one sample. Nothing reaches the line, so
the rule points to stopping. Chunking does have a small real cost, and sample 2 shows it can vary. It stays on record here if someone wants it later.

## Reading

- **`LineFilter` did not remove a word on these calls.** The worry about its English-only stop words is untested here, since these calls are English, and it no longer matters for local Norwegian.
- **Offline filtering matched production exactly** on both samples, so at real speed timing does not change the filter's result here.
- **#70's gap is not explained by this.** It compared different sentences, on NB-Whisper in Norwegian, at `--speed 8`. That engine has been removed by #77, so those runs were dropped, and the cause
  will not be established. The likeliest explanations remain the different sentences, Whisper losing context in chunks, and the speed.

## Guard baselines, today's build (sample 1, real speed)

For any later change to the pipeline.

| Set | Word errors | Bleed lines | Invented lines | Speech to line, median / p95 |
|---|---|---|---|---|
| Plain calls | 3.7% | 0 | 0 | me 3.28 / 3.73 s; them 0.36 / 0.79 s |
| Double talk | 13.4% (me 19.0%, them 4.3%) | 7 | 0 | me 3.31 / 3.91 s; them 0.32 / 0.95 s |
| Remote only | 10.8%, counting mic bleed (them 3.0%) | 6 | 0 | me 3.15 / 3.42 s; them 0.40 / 1.32 s |
| Silence, 5 min | — | 0 | 0 | — |
| Room tone, 5 min | — | 0 | 1 | — |
| Non-speech bursts, 5 min | — | 0 | **16** | — |

Short words kept: 18 of 20. This is the weak English fallback, 1.2 s clips of longer sentences, not real one-word replies, so it says little about "yes" or "no".

**Worth a look separately:** Parakeet wrote a line for 16 of the non-speech bursts in 5 minutes. These are noise clips, so it is a harsh test, but those lines would appear in a real transcript.

## Limits

- **Built calls, clean read speech, 0.8 s gaps between turns.** Real calls have hesitation, noise and interruptions, and chunking may cost more there. No real call audio was tested.
- **English only.** Parakeet's other languages, including Polish, were not measured.
- **p620 was busy,** with CI and other sessions, so speech-to-line figures may be slightly high.
- **Changed from the approved plan:** the Norwegian pair, the 8× comparison and the run without VAD were dropped, because #77 removed that engine.
