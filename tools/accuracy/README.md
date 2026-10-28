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

Prints bleed lines (decided on words unique to one side), lost owner words, remote words on the mic channel, invented lines on silence spans, lines of mostly unknown words, and inference latency per chunk. Extra flags: `--model`, `--language`, `--glossary`.

## Without audio (CI, tests)

    python3 -I tools/accuracy/synthetic.py tools/accuracy/data
    python3 -I tools/accuracy/score.py tools/accuracy/data/echo.wav tools/accuracy/ref.example.txt --fake "hello there"

`--fake TEXT` returns TEXT for every chunk instead of calling an engine.
