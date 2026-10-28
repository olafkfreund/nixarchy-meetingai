# Transcript accuracy evaluation

Scripts live in git; **audio never does** (`tools/accuracy/*.wav` and `tools/accuracy/data/` are ignored).
Never send the recordings to a cloud engine unless you chose that engine for the run.

## Record the real set (owner only, with everyone's consent)

Record a call where you and the other side both speak, including some double talk and silence:

    tools/accuracy/record.sh tools/accuracy/data/set1.wav 120

Left channel = default microphone, right = the default output's monitor. Use speakers (not headphones) to
measure echo bleed. Write `tools/accuracy/data/set1.txt`, one line per utterance, `me:` or `them:`,
optional `[silence]` markers:

    them: can you hear me
    me: yes loud and clear
    [silence]

## Score

With the local whisper-server running:

    python3 -I tools/accuracy/score.py tools/accuracy/data/set1.wav tools/accuracy/data/set1.txt --url http://127.0.0.1:8178/inference

Prints bleed lines (mic words that belong to `them:`), lost owner words, invented lines, mean latency per chunk.

## Without audio (CI, tests)

    python3 -I tools/accuracy/synthetic.py tools/accuracy/data
    python3 -I tools/accuracy/score.py tools/accuracy/data/echo.wav ref.txt --fake "hello there"

`--fake TEXT` returns TEXT for every chunk instead of calling an engine.
