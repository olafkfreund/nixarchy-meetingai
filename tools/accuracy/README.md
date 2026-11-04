# Transcript accuracy evaluation

Scripts live in git; **audio never does** (`tools/accuracy/*.wav` and `tools/accuracy/data/` are ignored, and evaluation sets live in `~/.cache/meetingai-eval/`).
Never send the recordings to a cloud engine unless you chose that engine for the run.

## Record the real set (owner only, with everyone's consent)

Tell everyone on the call it is being recorded for evaluation. Start a normal recording in the app
(not a local-only interview: the tap is refused there), then:

    tools/accuracy/record.sh tools/accuracy/data/set1.wav 120

This calls `nixarchy-meetingai ctl record.tap`, which copies exactly what the transcriber receives
(left = mic, right = system, after mute) into a 0600 WAV. Ctrl-C stops early. Include double talk and
silence; use speakers, not headphones, to measure echo bleed. Write the reference next to it, see
`ref.example.txt` (`me:` / `them:` lines, `[silence mm:ss-mm:ss]` spans).

## Score

With an OpenAI-compatible speech server running (`--url` is its base URL and `--model`, its model name, is required; for whisper.cpp's `whisper-server` start it with `--inference-path /v1/audio/transcriptions`):

    python3 -I tools/accuracy/score.py tools/accuracy/data/set1.wav tools/accuracy/data/set1.txt --url http://127.0.0.1:8178 --model large-v3-turbo

With local Parakeet (`--engine parakeet` or `--parakeet`), use the model directory in `NIXARCHY_MEETINGAI_PARAKEET_DIR` (the same setting as the daemon), or override it for this run:

    python3 -I tools/accuracy/score.py tools/accuracy/data/set1.wav tools/accuracy/data/set1.txt --engine parakeet --model-dir /path/to/parakeet-model

With local Norwegian Whisper or the official Whisper large-v3-turbo export (`--engine whisper-nb` or `--engine turbo`):

    python3 -I tools/accuracy/score.py FILE.wav REFERENCE.txt --engine turbo \
        --model-dir ~/.cache/meetingai-eval/no/models/sherpa-onnx-whisper-turbo \
        --vad ~/.cache/meetingai-eval/no/models/silero_vad.onnx \
        --parakeet-dir /path/to/parakeet-model \
        --speaker-model /path/to/speaker-model.onnx \
        --load-mode resident

Choose exactly one of `--engine parakeet|whisper-nb|turbo`, `--parakeet`, `--url URL`, or `--fake TEXT`.
`examples/accuracy_replay.rs` paces stereo audio at 1× wall-clock through `LiveTranscriber::connect_config_with` (or `transcription::transcribe` when `--dictation` is given) and reports `VmHWM` peak RSS after loading Parakeet and the speaker model under `--load-mode resident|unload`.
`score.py` prints word error rate (WER, sequence edit distance with NPSC normalisation), code-switch WER, bleed lines, lost owner words, remote words on the mic channel, invented lines per hour on silence/non-speech, phantom phrases anywhere in a line, short words kept, dictation status, median and p95 chunk inference time, and peak RSS (`VmHWM`).

## Without audio (CI, tests)

    python3 -I tools/accuracy/synthetic.py tools/accuracy/data
    python3 -I tools/accuracy/score.py tools/accuracy/data/echo.wav tools/accuracy/ref.example.txt --fake "hello there"

`--fake TEXT` returns TEXT for every chunk instead of calling an engine.

## Norwegian evaluation sets (#70)

All raw datasets and generated sets stay in `~/.cache/meetingai-eval/no/` (override the root with `MEETINGAI_EVAL_CACHE`).

### Licences and sizes

- **NPSC (`NbAiLab/NPSC`, Norwegian Parliamentary Speech Corpus)** — National Library of Norway (`NbAiLab`), licensed under **CC0 1.0** (`https://huggingface.co/datasets/NbAiLab/NPSC`). Test split (`16K_mp3_bokmaal` parquetshards): ~1.2 GB raw parquet; a fixed-seed ~2 h subset is selected by `no_mix.py`.
- **FLEURS `nb_no` (`google/fleurs`)** — Google, licensed under **CC BY 4.0** (`https://huggingface.co/datasets/google/fleurs`). Norwegian Bokmål `test` split (`test.tar.gz` + `test.tsv`): ~233 MB archive. Note: FLEURS WAVs are 32-bit float; `no_mix.py` converts them with `ffmpeg -c:a pcm_s16le`.
- **MUSAN (`noise/` subset, OpenSLR 17)** — David Snyder, Guoguo Chen and Daniel Povey, licensed under **CC BY 4.0** (`https://www.openslr.org/17/`). Only the `noise/` subset is extracted: ~1.0 GB (`musan.tar.gz` full archive is ~11 GB; extract `musan/noise` only).
- **Models in `~/.cache/meetingai-eval/no/models/`:**
  - `csukuangfj/sherpa-onnx-whisper-turbo` (`sherpa-onnx-whisper-turbo.tar.bz2`, **MIT**, ~1.0 GB extracted int8 ONNX);
  - `NbAiLab/nb-whisper-large` or `-medium` (**Apache-2.0**, exported to int8 ONNX via `tools/whisper-export/`, ~0.8–1.6 GB);
  - `silero_vad.onnx` (**MIT**, ~2 MB).

### Download commands

```bash
mkdir -p ~/.cache/meetingai-eval/no/{npsc,clips,musan,models}

# 1. NPSC test split parquet shards (CC0, ~1.2 GB)
for i in 00000 00001 00002 00003; do
    curl -fL --retry 3 -o ~/.cache/meetingai-eval/no/npsc/test-${i}-of-00004.parquet \
        "https://huggingface.co/datasets/NbAiLab/NPSC/resolve/main/16K_mp3_bokmaal/test-${i}-of-00004.parquet"
done

# 2. FLEURS nb_no test split (CC BY 4.0, ~233 MB)
curl -fL --retry 3 -o ~/.cache/meetingai-eval/no/clips/test.tar.gz \
    "https://huggingface.co/datasets/google/fleurs/resolve/main/data/nb_no/audio/test.tar.gz"
curl -fL --retry 3 -o ~/.cache/meetingai-eval/no/clips/test.tsv \
    "https://huggingface.co/datasets/google/fleurs/resolve/main/data/nb_no/test.tsv"
tar -xzf ~/.cache/meetingai-eval/no/clips/test.tar.gz -C ~/.cache/meetingai-eval/no/clips

# 3. MUSAN noise/ subset only (CC BY 4.0, ~1.0 GB extracted)
curl -fL --retry 3 "https://www.openslr.org/resources/17/musan.tar.gz" \
    | tar -xz -C ~/.cache/meetingai-eval/no --strip-components=1 musan/noise

# 4. Whisper turbo int8 export and Silero VAD (MIT)
curl -fL --retry 3 "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-whisper-turbo.tar.bz2" \
    | tar -xj -C ~/.cache/meetingai-eval/no/models
curl -fL --retry 3 -o ~/.cache/meetingai-eval/no/models/silero_vad.onnx \
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx"
```

### Build and score the Norwegian sets

Build the full evaluation suite (2 h NPSC by seed, FLEURS `nb_no`, silence, room tone, 0.2–1.5 s non-speech bursts, remote-only, double talk with delayed bleed, short words, 1–3 s and 40–60 s dictations, code-switching, and optional `--owner-dir`):

```bash
nix shell --impure --expr 'with import <nixpkgs> {}; python3.withPackages (p: [ p.pyarrow ])' \
    -c python3 -I tools/accuracy/no_mix.py --seed 70
```

For a fast smoke slice (1 file per condition):

```bash
python3 -I tools/accuracy/no_mix.py --slice 1
```

Score a single call or the entire suite directory:

```bash
# Single call (saves raw JSONL replay log showing 3 s bleed-hold release times)
python3 -I tools/accuracy/score.py \
    ~/.cache/meetingai-eval/no/slice/fleurs/call_001.wav \
    ~/.cache/meetingai-eval/no/slice/fleurs/call_001.txt \
    --engine turbo \
    --model-dir ~/.cache/meetingai-eval/no/models/sherpa-onnx-whisper-turbo \
    --vad ~/.cache/meetingai-eval/no/models/silero_vad.onnx \
    --load-mode resident \
    --replay-log ~/.cache/meetingai-eval/no/slice/replay-turbo.jsonl

# Dictation clip (mono 16 kHz PCM via transcription::transcribe)
python3 -I tools/accuracy/score.py \
    ~/.cache/meetingai-eval/no/slice/dictation_short/dict_short_001.wav \
    ~/.cache/meetingai-eval/no/slice/dictation_short/dict_short_001.txt \
    --engine turbo \
    --model-dir ~/.cache/meetingai-eval/no/models/sherpa-onnx-whisper-turbo \
    --vad ~/.cache/meetingai-eval/no/models/silero_vad.onnx \
    --dictation

# Entire suite directory
python3 -I tools/accuracy/score.py ~/.cache/meetingai-eval/no/sets \
    --engine turbo \
    --model-dir ~/.cache/meetingai-eval/no/models/sherpa-onnx-whisper-turbo \
    --vad ~/.cache/meetingai-eval/no/models/silero_vad.onnx \
    --load-mode resident
```

## Alkove Focus comparison (owner recordings only)

Use a throwaway Python 3.13 venv in the scratchpad. These hashes pin the Linux x86-64 wheels; the Alkove 0.3.2 package pins and checks its model files. No API key is needed.

    python3.13 -m venv ~/meetingai-eval/alkove-venv
    cat > ~/meetingai-eval/alkove-requirements.txt <<'EOF'
    alkove==0.3.2 --hash=sha256:8eeda691d6b030064f80b2440c7c1adb6a0b1849d84c79455c195039a7cdcdb9
    numpy==2.2.6 --hash=sha256:1bc23a79bfabc5d056d106f9befb8d50c31ced2fbc70eedb8155aec74a45798f
    EOF
    ~/meetingai-eval/alkove-venv/bin/pip install --require-hashes --only-binary=:all: -r ~/meetingai-eval/alkove-requirements.txt
    ~/meetingai-eval/alkove-venv/bin/python -I tools/accuracy/alkove_filter.py IN.wav SAMPLE.wav OUT.wav

`IN.wav` is stereo 16 kHz 16-bit PCM (mic left, system right); `SAMPLE.wav` is mono owner-only audio in the same format. The output is stereo with the system channel unchanged and prints `{"rtf": ...}`. Output must not already exist.

## Without the owner: LibriSpeech calls

    python3 -I tools/accuracy/libri_mix.py WAVDIR ENROLLDIR OUTDIR

Builds stereo test calls with speaker bleed and double talk from LibriSpeech test-clean, with references and an owner sample each (see the script). `cargo run --release --example echo_gate_file IN.wav OUT.wav` runs a call through the daemon's echo gate. The October results are in `reports/2026-10-voice-filters.md`.
