# Noise lines: a voice-activity check doesn't pass (#83)

Measured on p620, 2026-11-05, Parakeet TDT 0.6B v3, with `tools/accuracy/short_replies.py` and `examples/vad_gate.rs`.

## Result

**No change.** A Silero check on each finished chunk removes most invented lines on noise, but on this set it also removes real short utterances, mostly clips with very little sound. No setting met the rule, so #83 closes with this finding.

Offline on the tuning half: the bursts of sample 1, and the real short utterances that today's build writes a line for.

| Silero threshold | Invented lines on bursts (today 16) | Real utterances lost (of 98) | Meets the rule? |
|---|---|---|---|
| 0.02* | 15 | 0 | No: noise falls 6% |
| 0.05* | 12 | 2 | No |
| 0.1* | 5 | 2 | No: noise falls 69%, and it loses replies |
| 0.2 | 1 | 11 | No |
| 0.3 | 1 | 21 | No |
| 0.5 | 0 | 39 | No |

\* Below the plan's range, tried afterwards to see whether anything came close.

The rule needed a fall of at least 75% in invented lines **and** no real utterance lost. Minimum speech of 0.05, 0.1 and 0.2 s gave identical results on the plan's thresholds, so it isn't shown. It wasn't varied at the lower thresholds.

Counts are lines after the filter. Removing a line can release a repeat the filter was holding back: at 0.1 the 5 burst lines are 3 survivors and 2 newly released, so 69% misses the target by one line, on a small count.

**Lost utterances, as written in the human transcript:**

- 0.05 and 0.1: MM-HMM, YEAH
- 0.2: LIKE WHAT, MM, MM-HMM, YEAH, YEAH, YEAH, MM-HMM, UH, SURE, THAT, YEAH
- 0.3: YEAH I KNOW, ALTHOUGH YEAH YEAH, LIKE WHAT, SO, MM-HMM, MM, MM, MM-HMM, YEAH, OH YEAH, YEAH, MM, YEAH, YEAH, MM-HMM, UH, YEAH, THAT, SURE, THAT, YEAH
- 0.5: YEAH, OH THERE'S MORE, YEAH, YEAH I KNOW, ALTHOUGH YEAH YEAH, LIKE WHAT, SO, YEAH YEAH YEAH, YEAH YEAH YEAH, OKAY, MM-HMM, YEAH, MM, MM, MM-HMM, YEAH, OH YEAH, OKAY, MM, YEAH, MM, OKAY, YEAH, YEAH, YEAH, MM-HMM, YEAH, MM-HMM, UM BUT, UH, YEAH, THAT, SURE, OKAY, OKAY, THAT, NO, YEAH, WE COULD

**The check's own time isn't settled.** It was about 5.5 ms median and 9 ms p95 per chunk, but only on these very short chunks, median 1.19 s. The saved timings fit about 4.7 ms per second of chunk. Real speech chunks have a median of about 5 s, which would extrapolate to about 24 ms, over the rule's 10 ms limit. That's an estimate, not a measurement.

As the plan says, with no setting passing on the tuning half, the held-back half was not used, and the guards and computed delay were not run against a check.

## Baselines

Today's build, re-measured.

| Set, 5 minutes unless noted | Sample 1 | Sample 2 |
|---|---|---|
| Silence: invented lines | 0 | 0 |
| Room tone | 1 | 2 |
| Bursts | 16 | 18 |

Sample 1 matches #73's 16, 1 and 0.

## Something else this found

Only 98 of the 400 real short utterances in the tuning half produced a line today (102 of 400 in the held-back half).

- 285 of the 400 never became a chunk, so no engine saw them. They're quiet, with a median peak of 0.022, against 0.243 for those that did, and they have about 0.05 s above the sound level, under the 0.2 s `MIN_SOUND_SAMPLES` needs (`src/speech/stt/transcription.rs`).
- Of the 115 chunks, 108 got a line from Parakeet, and the filter released 98. The rest were mostly repeats: "YEAH" went from 34 to 25, because the set puts them seconds apart.

So the check was really tested on about 100 utterances, not 400. It also suggests the loudness floor drops quiet short replies before any engine runs. This may just be how AMI's close-talk audio is recorded. It hasn't been measured on real call levels, and it's a separate question from noise.

## Limits

- Built sets, and AMI isn't the owner's calls. Each half comes from only 4 meetings, and the scored utterances are narrower still: 70 of the 98 are from one meeting, IB4001, and 35 from one speaker. Both losses at 0.05 and 0.1 are that speaker.
- The losses are mostly clips with very little sound. At 0.2, chunks with under 0.25 s above the level were rejected 10 times in 23, at 0.25 to 0.30 s 5 in 30, at 0.30 to 0.50 s 1 in 31, and at 0.50 s or more 0 in 31. Most of the rejected clips start right after digital silence, in a freshly reset detector, which may be harsher than a reply in a real call. This data can't separate the two, and a zero-loss rule fails either way.
- Noise during a conversation wasn't measured.
- Offline: the check is applied to saved chunks, not run live.
- MUSAN bursts are a harsh test. How often this happens in real calls is still unknown.

## What would change this

A real need. If invented lines become a problem in real calls, threshold 0.1 is the place to start measuring, on real call audio and normal-length chunks. It would need accepting that some real replies go missing, and its time on normal chunks would need checking. That choice is the owner's, and this result doesn't make it. The owner decided doing nothing is acceptable if real calls rarely show this.
