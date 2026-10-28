#!/usr/bin/env python3
"""Score the daemon's batch transcription of a stereo WAV against a reference file.

Usage: python3 -I score.py FILE.wav REFERENCE.txt (--url URL | --fake TEXT) [--model M] [--language L] [--glossary G]
Reference (see ref.example.txt): `me: words` / `them: words` per utterance (prefix case-insensitive),
`[silence]` (ignored) or `[silence mm:ss-mm:ss]` (output lines inside the span are invented).
Extra flags go to examples/accuracy_replay.rs, which runs the live batch path.
"""
import json, re, subprocess, sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def words(text):
    return re.findall(r"[\w']+", text.lower().replace("’", "'"))


def parse_reference(text):
    me, them, silence = [], [], []
    for n, line in enumerate(text.splitlines(), 1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        if m := re.fullmatch(r"\[silence(?:\s+(\d+):(\d\d)-(\d+):(\d\d))?\]", line, re.I):
            if m[1]:
                silence.append((int(m[1]) * 60 + int(m[2]), int(m[3]) * 60 + int(m[4])))
            continue
        who, sep, rest = line.partition(":")
        who = who.strip().lower()
        if not sep or who not in ("me", "them"):
            raise ValueError(f"reference line {n}: expected 'me:' or 'them:', got {line[:30]!r}")
        (me if who == "me" else them).extend(words(rest))
    return me, them, silence


def score(records, me, them, silence):
    kept = [o for o in records if o["kept"]]
    me_only, them_only = set(me) - set(them), set(them) - set(me)
    mic = [o for o in kept if o["channel"] == 0]
    remote = lambda o: sum(w in them_only for w in words(o["text"]))
    owner = lambda o: sum(w in me_only for w in words(o["text"]))
    bleed = [o for o in mic if remote(o) > owner(o)]
    clean = [o for o in mic if o not in bleed]
    lost = Counter(me) - Counter(w for o in clean for w in words(o["text"]))
    remote_words = Counter(w for o in mic for w in words(o["text"]) if w in them_only)
    in_silence = [o for o in kept if any(o["start"] < b and o["end"] > a for a, b in silence)]
    allref = set(me) | set(them)
    unknown = [o for o in kept if words(o["text"]) and sum(w in allref for w in words(o["text"])) / len(words(o["text"])) < 0.5]
    latency = sum(o["latency"] for o in records) / len(records) if records else 0.0
    return bleed, lost, remote_words, in_silence, unknown, latency, len(kept)


def main(argv):
    if len(argv) < 3:
        sys.exit(__doc__)
    wav, ref, rest = str(Path(argv[0]).resolve()), argv[1], argv[2:]
    me, them, silence = parse_reference(Path(ref).read_text())
    run = subprocess.run(
        ["cargo", "run", "-q", "--release", "--example", "accuracy_replay", "--", wav, *rest],
        cwd=ROOT, capture_output=True, text=True,
    )
    if run.returncode:
        sys.exit(run.stderr)
    records = [json.loads(l) for l in run.stdout.splitlines() if l.strip()]
    bleed, lost, remote_words, in_silence, unknown, latency, n = score(records, me, them, silence)
    for name, items in (("bleed", bleed), ("invented on silence", in_silence), ("unknown-word line", unknown)):
        for o in items:
            print(f"{name}: [{o['start']:.1f}s] {o['text']}")
    print(f"lost owner words: {sum(lost.values())} {dict(lost)}")
    print(f"remote words on the mic channel: {sum(remote_words.values())} {dict(remote_words)}")
    print(f"bleed lines: {len(bleed)}")
    print(f"invented lines on silence: {len(in_silence)}")
    print(f"lines with mostly unknown words (second number): {len(unknown)}")
    print(f"inference latency per chunk: {latency:.2f}s over {len(records)} requests ({n} lines kept)")


if __name__ == "__main__":
    main(sys.argv[1:])
