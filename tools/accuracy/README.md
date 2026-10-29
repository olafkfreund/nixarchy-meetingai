# Transcript accuracy evaluation

Scripts live in git; **audio never does** (`tools/accuracy/*.wav` and `tools/accuracy/data/` are ignored).
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

With local Parakeet, use the model directory in `NIXARCHY_MEETINGAI_PARAKEET_DIR` (the same setting as the daemon), or override it for this run:

    python3 -I tools/accuracy/score.py tools/accuracy/data/set1.wav tools/accuracy/data/set1.txt --parakeet --model-dir /path/to/parakeet-model

Choose exactly one of `--url URL`, `--fake TEXT`, or `--parakeet`. Prints bleed lines (decided on words unique to one side), lost owner words, remote words on the mic channel, invented lines on silence spans, lines of mostly unknown words, and inference latency per chunk. Extra flags: `--model-dir`, `--model`, `--language`, `--glossary`.

## Without audio (CI, tests)

    python3 -I tools/accuracy/synthetic.py tools/accuracy/data
    python3 -I tools/accuracy/score.py tools/accuracy/data/echo.wav tools/accuracy/ref.example.txt --fake "hello there"

`--fake TEXT` returns TEXT for every chunk instead of calling an engine.

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
