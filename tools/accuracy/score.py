#!/usr/bin/env python3
"""Score the daemon's batch transcription of a stereo WAV against a reference file.

Usage: python3 -I score.py FILE.wav REFERENCE.txt (--url URL | --fake TEXT)
Reference: one utterance per line, `me: words` or `them: words`; `[silence]` lines are markers.
Prints bleed lines, lost owner words, invented lines and mean latency per chunk.
"""
import json, re, subprocess, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
words = lambda t: re.findall(r"[\w']+", t.lower())


def main(argv):
    if len(argv) < 3:
        sys.exit(__doc__)
    wav, ref, rest = argv[0], argv[1], argv[2:]
    me, them = [], []
    for line in Path(ref).read_text().splitlines():
        who, _, text = line.partition(":")
        if who.strip() == "me":
            me += words(text)
        elif who.strip() == "them":
            them += words(text)
    run = subprocess.run(
        ["cargo", "run", "-q", "--example", "accuracy_replay", "--", wav, *rest],
        cwd=ROOT, capture_output=True, text=True,
    )
    if run.returncode:
        sys.exit(run.stderr)
    out = [json.loads(l) for l in run.stdout.splitlines() if l.strip()]
    mic = [o for o in out if o["channel"] == 0]
    inside = lambda ws, pool: sum(w in pool for w in ws) / len(ws) if ws else 0.0
    bleed = [o for o in mic if inside(words(o["text"]), set(them)) > 0.5 and inside(words(o["text"]), set(me)) < 0.5]
    mic_words = {w for o in mic for w in words(o["text"])}
    lost = [w for w in me if w not in mic_words]
    allref = set(me) | set(them)
    invented = [o for o in out if words(o["text"]) and inside(words(o["text"]), allref) < 0.5]
    latency = sum(o["latency"] for o in out) / len(out) if out else 0.0
    for name, items in (("bleed", [o["text"] for o in bleed]), ("invented", [o["text"] for o in invented])):
        for t in items:
            print(f"{name}: {t}")
    print(f"lost owner words: {len(lost)} {lost}")
    print(f"bleed lines: {len(bleed)}  invented lines: {len(invented)}  lines: {len(out)}  mean latency/chunk: {latency:.2f}s")


main(sys.argv[1:])
