# whisper.cpp model benchmark

Produced by `tools/whisper-bench.sh` (run it again to refresh this file).

- Reference set: `hf-internal-testing/librispeech_asr_dummy` (LibriSpeech
  dev-clean sample), 73 utterances, 481 s of audio, 16 kHz mono WAV.
- Requests: `POST /inference`, `temperature=0`, `response_format=json`,
  `language=en`, one warm-up request not counted.
- WER: lowercase, punctuation stripped. RTF: total request time / total audio.

| model | WER % | RTF | p50 s | p95 s |
|---|---|---|---|---|
| small.en | 6.26 | 0.021 | 0.12 | 0.27 |
| medium.en | 6.26 | 0.040 | 0.22 | 0.52 |
| large-v3-turbo | 4.26 | 0.024 | 0.15 | 0.22 |

## Machine

- GPU: AMD Radeon RX 7900 XT (Navi 31), Mesa RADV, via Vulkan
- whisper-cpp 1.9.5 (`nixpkgs#whisper-cpp-vulkan`), Linux 7.3.0, 2026-10-27

## Recommended default

Rule: the most accurate model with RTF <= 0.3 and p95 <= 1.5 s. All three
pass; `large-v3-turbo` is the most accurate, so it is the default
(`nix/hm-module.nix`, `nix/whisper-model.nix`).

Limits: the set is clean read speech, so absolute WER is optimistic for real
meetings and the ordering matters more than the numbers. Less capable GPUs
(or CPU only) must re-run the script before keeping this default.
