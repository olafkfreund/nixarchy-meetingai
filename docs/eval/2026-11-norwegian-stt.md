# Norwegian speech-to-text: gate measurement (#70)

Measured on p620 (128 threads, `NIXARCHY_MEETINGAI_WHISPER_NB_THREADS=16`),
2026-11-04. All engines run in-process through sherpa-onnx 2.0.1, the
Whisper engines with the Silero VAD and the phantom filter. The test sets
are built by `tools/accuracy/no_mix.py` (plan step 3). razer is **not**
measured yet.

## Results against the gate (spec S2)

The sample is 20 conversational NPSC calls (21 min) through the real call
pipeline (`BatchChunks`, `LineFilter`). The latency row comes from one
double-talk call at real time (1×).

| Bar | Target | Parakeet v3 (today) | Whisper turbo | **NB-Whisper small** | NB-Whisper medium¹ |
|---|---|---|---|---|---|
| Word error rate, NPSC calls | ≤ 20% | 85.1% ✗ | 32.7% ✗ | **23.9% ✗** | 29.2%¹ ✗ |
| Word error rate, single NPSC sentences² | (model only) | — | — | **14.4% ✓** | — |
| Invented lines / phantom phrases | ≤ 1 per h / 0 | 0 / 0 ✓ | 0 / 0 ✓ | **0 / 0 ✓** | 0 / 0 ✓ |
| Bleed lines | ≤ Parakeet (1) | 1 | 0 ✓ | **1 ✓** | 0 ✓ |
| Short words kept | ≥ 9 in 10 | 0/16 ✗ | 7/16 ✗ | **7/16 ✗** | 8/16 ✗ |
| Dictations (1–3 s, 40–60 s) | all succeed | 0/20 ✗ | 20/20 ✓ | **20/20 ✓** | 20/20 ✓ |
| Chunk time, median / p95, at 1× | < 1.5 s / < 3 s | 0.55 / 1.95 s | 3.2 / 8.3 s ✗ | **2.4 / 7.8 s ✗** | about 10× small ✗ |
| Peak RSS, with Parakeet and the speaker model | < 4 GB | 1.6 GB | 3.1 GB ✓ | **2.1 GB ✓** | 3.5 GB ✓ |
| Code-switch word error rate (reported) | — | 88.8% | 48.8% | 42.5% | 47.1%¹ |

¹ Measured before the truncation fix (below), so it is about 7 points high,
judging by small's change.

² 40 single NPSC sentences of 2–14 s, decoded with no call pipeline. The
National Library publishes 14.6% "exact" WER for NB-Whisper small on clean
Stortinget speech, the same domain.

## Reading

- **Today's engine cannot do Norwegian.** Parakeet v3 writes Swedish, at
  85% word error rate.
- **NB-Whisper small is the best candidate.** It is the most accurate,
  needs the least RAM of the Whisper candidates, never invents text, and
  handles every dictation.
  - **The model passes the accuracy bar on its own** (14.4%).
  - About 10 points are lost **in the call pipeline**: 336 owner words
    "lost", likely in chunking or the `LineFilter` bleed hold.
  - That loss would apply to any engine and deserves its own look.
- **Latency fails on CPU for every Whisper candidate.** NB-Whisper small
  takes 2.4 s per chunk at median on p620, so Norwegian lines would arrive a
  few seconds after the speech. razer is slower.
- **Short words** ("ja", "nei") are kept at 7 in 16, against a bar of 9 in
  10.

## Measurement fixes made during the gate

- **Truncation.** `no_mix.py` cut sentences over 14 s of audio but kept the
  full reference text, which scored every cut word as an error. Over-long
  sentences are now left out. NB-Whisper small went from 31.2% to 23.9%.
- **NPSC layout.** NPSC is read from its real per-sitting JSONL and MP3
  archives, not parquet files.
- **Speed.** The replay's `--speed N` feeds the accuracy sets faster. The
  bleed hold counts audio time, and latency is measured only at 1×.
- **NB-Whisper exports.**
  - Its weights come from safetensors; the CTranslate2 `model.bin` is not
    used.
  - The export forces PyTorch's legacy ONNX exporter (`dynamo=False`); the
    dynamo exporter baked static decoder shapes.
  - large: the export failed at runtime before the `dynamo=False` fix and
    was not retried.
  - small (rev `e9bb5cb`) and medium (rev `0ed074d`): both decode
    correctly.

## Decision (owner, 2026-11-04)

**Approved: ship NB-Whisper small, opt-in, with relaxed bars for Norwegian.**
- It is used only when the speech language is set to Norwegian and
  `whisperNb.enable` is on (off by default).
- Accepted for this release:
  - 23.9% word error rate through the call pipeline (14.4% for the model);
  - 2.4 s median and 7.8 s p95 per chunk on p620;
  - 7 of 16 short words kept.

  All three are far better than today's Swedish output (85% word error
  rate).
- The loading mode is `Resident` (peak 2.1 GB).
- Follow-ups are filed as their own issues: the call pipeline's word loss,
  which affects every engine; Norwegian latency and short words; razer
  measurement.
