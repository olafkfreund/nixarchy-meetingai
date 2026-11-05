#!/usr/bin/env python3
"""Short real utterances from AMI (#83): a tuning half and a held-back half, split by meeting.

Build (needs pyarrow):
  nix-shell -p 'python3.withPackages(p:[p.pyarrow])' --run 'python3 -I tools/accuracy/short_replies.py'
Count lines of a replay run against a built set (no pyarrow):
  python3 -I tools/accuracy/short_replies.py --count RUN.jsonl --set short/tune/set.json [--check CHECKED.jsonl]

Each half is a 16 kHz stereo WAV: utterances on channel 1 (remote), each followed by 3 s of silence,
channel 0 silent. set.json lists each utterance's window and reference. Audio stays in the cache.
"""
import argparse
import array
import glob
import json
import pathlib
import random
import re
import struct
import sys
import wave

sys.dont_write_bytecode = True
RATE = 16000
MAX_WORDS, MAX_SECS, CAP, GAP = 3, 1.5, 400, 3.0
CACHE = pathlib.Path.home() / ".cache" / "meetingai-eval"


def keep(r: dict) -> bool:
    words = r["text"].split()
    return (0 < len(words) <= MAX_WORDS and any(c.isalpha() for c in r["text"])
            and round(r["end_time"] - r["begin_time"], 3) <= MAX_SECS)


def split_meetings(meetings, seed: int) -> tuple[list[str], list[str]]:
    ms = sorted(set(meetings))
    random.Random(seed).shuffle(ms)
    h = len(ms) // 2
    return sorted(ms[:h]), sorted(ms[h:])  # tune, held


def pick(rows: list[dict], cap: int, seed: int) -> list[dict]:
    rows = sorted(rows, key=lambda r: r["audio_id"])
    if len(rows) > cap:
        rows = random.Random(seed).sample(rows, cap)
    return sorted(rows, key=lambda r: r["audio_id"])


def decode(data: bytes) -> array.array:
    """RIFF WAV, 16 kHz mono, 16-bit PCM or 32-bit float (the AMI parquet holds float) -> int16."""
    if data[:4] != b"RIFF" or data[8:12] != b"WAVE":
        raise SystemExit("audio is not a WAV")
    pos, fmt, pcm = 12, None, None
    while pos + 8 <= len(data):
        cid, n = data[pos:pos + 4], struct.unpack_from("<I", data, pos + 4)[0]
        body = data[pos + 8:pos + 8 + n]
        if cid == b"fmt ":
            fmt = struct.unpack_from("<HHIIHH", body)  # tag, channels, rate, byterate, align, bits
        elif cid == b"data":
            pcm = body
        pos += 8 + n + (n & 1)
    if fmt is None or pcm is None or fmt[1:3] != (1, RATE) or (fmt[0], fmt[5]) not in ((1, 16), (3, 32)):
        raise SystemExit(f"audio is not 16 kHz mono 16-bit PCM or 32-bit float: {fmt}")
    if fmt[0] == 1:
        a = array.array("h")
        a.frombytes(pcm[:len(pcm) // 2 * 2])
        return a
    f = array.array("f")
    f.frombytes(pcm[:len(pcm) // 4 * 4])
    return array.array("h", (max(-32768, min(32767, round(x * 32767))) for x in f))


def write_half(rows: list[dict], out: pathlib.Path) -> dict:
    out.mkdir(parents=True, exist_ok=True)
    pcm, utts, pos = array.array("h"), [], 0  # pcm = channel 1 only
    gap = array.array("h", bytes(2 * round(GAP * RATE)))
    for r in rows:
        s = decode(r["wav"])
        utts.append({"id": r["audio_id"], "meeting": r["meeting_id"], "text": r["text"],
                     "start": pos / RATE, "end": (pos + len(s)) / RATE})
        pcm += s + gap
        pos += len(s) + len(gap)
    st = array.array("h", bytes(4 * len(pcm)))
    st[1::2] = pcm
    with wave.open(str(out / "set.wav"), "wb") as w:
        w.setnchannels(2), w.setsampwidth(2), w.setframerate(RATE)
        w.writeframes(st.tobytes())
    doc = {"utterances": utts, "meetings": sorted({u["meeting"] for u in utts}), "seconds": pos / RATE}
    (out / "set.json").write_text(json.dumps(doc, indent=1), encoding="utf-8")
    return doc


def build(rows: list[dict], out: pathlib.Path, seed: int, cap: int = CAP) -> dict:
    rows = list({r["audio_id"]: r for r in rows if keep(r)}.values())
    tune_m, held_m = split_meetings((r["meeting_id"] for r in rows), seed)
    return {name: write_half(pick([r for r in rows if r["meeting_id"] in ms], cap, seed), out / name)
            for name, ms in (("tune", set(tune_m)), ("held", set(held_m)))}


def words(t: str) -> set[str]:
    return set(re.findall(r"[^\W\d_]+", t.lower()))


def count(utts: list[dict], lines: list[dict]) -> dict:
    """Give each channel-1 line to the one utterance it overlaps most; kept = it got a line with a word."""
    got = {u["id"]: [] for u in utts}
    for ln in lines:
        if ln.get("kind") != "line" or ln.get("channel") != 1:
            continue
        ov = [(min(ln["end"], u["end"]) - max(ln["start"], u["start"]), -i) for i, u in enumerate(utts)]
        best, neg = max(ov)  # a tie goes to the earlier utterance
        if best > 0 and words(ln["text"]):
            got[utts[-neg]["id"]].append(ln["text"])
    kept = [u["id"] for u in utts if got[u["id"]]]
    shared = sum(1 for u in utts for t in got[u["id"]] if words(t) & words(u["text"]))
    return {"kept": kept, "lines": sum(map(len, got.values())), "shared": shared}


def read_lines(p: str) -> list[dict]:
    return [json.loads(x) for x in pathlib.Path(p).read_text(encoding="utf-8").splitlines() if x.strip()]


def lost(utts: list[dict], base: dict, check: dict) -> list[dict]:
    gone = set(base["kept"]) - set(check["kept"])
    return [u for u in utts if u["id"] in gone]


def load_rows(raw: pathlib.Path) -> list[dict]:
    import pyarrow.parquet as pq
    files = sorted(glob.glob(str(raw / "*.parquet")))
    if not files:
        raise SystemExit(f"no parquet under {raw}")
    rows = []
    for f in files:
        for b in pq.ParquetFile(f).iter_batches(batch_size=256):
            for r in b.to_pylist():
                if keep(r):
                    r["wav"] = r.pop("audio")["bytes"]
                    rows.append(r)
                    r.pop("microphone_id", None)
    return rows


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--raw", type=pathlib.Path, default=CACHE / "ami-hf-ihm" / "raw")
    ap.add_argument("--out", type=pathlib.Path, default=CACHE / "en83" / "short")
    ap.add_argument("--seed", type=int, default=83)
    ap.add_argument("--count", metavar="RUN.jsonl")
    ap.add_argument("--set", dest="setfile")
    ap.add_argument("--check", metavar="CHECKED.jsonl")
    a = ap.parse_args()
    if a.count:
        if not a.setfile:
            raise SystemExit("--count needs --set set.json")
        utts = json.loads(pathlib.Path(a.setfile).read_text(encoding="utf-8"))["utterances"]
        base = count(utts, read_lines(a.count))
        print(json.dumps({**base, "kept_n": len(base["kept"]), "of": len(utts)}))
        if a.check:
            chk = count(utts, read_lines(a.check))
            print(json.dumps({"checked_kept_n": len(chk["kept"]), "checked_lines": chk["lines"],
                              "checked_shared": chk["shared"]}))
            for u in lost(utts, base, chk):
                print(f"lost {u['id']} [{u['start']:.2f}-{u['end']:.2f}s]: {u['text']}")
        return
    for name, d in build(load_rows(a.raw), a.out, a.seed).items():
        print(f"{name}: {len(d['utterances'])} utterances, {len(d['meetings'])} meetings, "
              f"{d['seconds']:.0f} s -> {a.out / name / 'set.wav'}")


if __name__ == "__main__":
    main()
