# Speaker separation with CAM++ online clustering: gate result (#61)

**Result: the gate fails.** On real meeting audio, CAM++ embeddings clustered
online cannot label most remote lines without giving many to the wrong
speaker. Latency and the owner voice check are fine, but accuracy on
meetings is far from the bar.

Measured on p620, 2026-11-04, with `examples/diarize_eval` (plan step 4).
Thresholds were chosen on the dev half and scored on the held-out half,
whose speakers do not overlap with the dev half.

## Bar (spec S2), against measured values

The gate condition is AMI after an Opus round trip at 16 kbps (the "call"
condition), held-out half. Per the owner's choice on 2026-11-04, `ihm` (each
person's headset, mixed like a call) is the gate set. `sdm` (one room
microphone) is reported as the hard case.

| Bar | Target | AMI `ihm` (gate) | AMI `sdm` | LibriSpeech |
|---|---|---|---|---|
| Errors (labelled lines wrong, mixed lines included) | ≤ 5% | **9.3% ✗** | 7.4% ✗ | 5.7% ✗ (just over) |
| Coverage (single-speaker lines ≥ 1.5 s labelled) | ≥ 80% | **36% ✗** | 18% ✗ | 93% ✓ |
| Mixed-line abstention (≥ 3 s) | ≥ 75% | 98% ✓ | 100% ✓ | 98% ✓ |
| Chunk-end-to-line latency, separation on vs off | +< 100 ms median | +0 ms ✓ (system 252 vs 253 ms; mic 3 663 vs 3 728 ms) | | |
| Owner check: results / time | unchanged / +< 100 ms | identical / +11 ms ✓ | | |
| Embedding cost | < 100 MB RSS, < 5 CPU-s/min | about 140–250 MB RSS ✗ on the slice; 0.1–0.8 CPU-s/min ✓ | | |

The latency and owner-check rows come from the `--live` run on p620: 5
minutes of AMI EN2002a (call condition) on channel 1, and a LibriSpeech owner
track on channel 0. razer was not measured, because the accuracy result
decides the gate on its own.

## Thresholds

Chosen on the dev half as the most coverage at no more than 5% errors:
- AMI: `JOIN 0.70`, `NEW 0.25`, `MIXED 0.35`;
- LibriSpeech: `JOIN 0.50`, `NEW 0.35`, `MIXED 0.45`.

The best any threshold reached on the dev half:

| Set (dev, call) | Coverage at ≤ 5% errors |
|---|---|
| LibriSpeech | 90% |
| AMI `ihm` | 39% |
| AMI `sdm` | 28% |

On the held-out AMI `ihm` half, no threshold in the grid reaches 5% errors.

## Test sets

- **LibriSpeech `test-clean`:** 60 synthetic conversations of 2–4 speakers
  (`tools/diarize/librispeech_convs.py`), 30 dev and 30 held-out, split by
  speaker.
- **AMI (CC BY 4.0), from the Hugging Face copy `edinburghcstr/ami`:**
  rebuilt by `tools/diarize/ami_hf.py`. The Edinburgh mirror refuses most
  downloads.
  - Dev (validation): ES2011a/c, IB4001, IB4003; 1 h 48 min.
  - Held-out (test): EN2002a–c; 1 h 54 min.
- **Call condition:** every set after an Opus round trip at 16 kbps
  (`tools/diarize/opus_roundtrip.sh`).

## Reading

Close-mic read speech (LibriSpeech) nearly passes. Real meetings do not,
even when every person is on their own headset (`ihm`). The likely causes:
- short turns;
- natural conversation, where one person's voice varies more between turns
  than read speech does;
- an embedding model trained for verification, against a clean enrolment,
  rather than for clustering short remote turns online.

The per-call embedder lock (plan step 1) is independent of this result. It
fixes the owner check's existing concurrent use of the model.
