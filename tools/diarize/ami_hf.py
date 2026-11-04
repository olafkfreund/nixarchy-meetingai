#!/usr/bin/env python3
"""Rebuild AMI meetings from the Hugging Face copy (edinburghcstr/ami, CC BY 4.0) (#61 step 3).

The Edinburgh mirror refuses most whole-file downloads (HTTP 403, rate-limited), so the
meetings come from the `sdm` (single distant microphone) config instead: one row per
utterance with `meeting_id`, `speaker_id`, `begin_time`, `end_time` and its WAV bytes.
Each meeting is rebuilt on one 16 kHz track by placing every segment at `begin_time`. Where
segments overlap, the samples already placed are kept, not added: both segments are the same
microphone, so adding would double the overlap.

The halves follow AMI's official partition, which keeps speakers apart:
`validation` is the dev half, `test` the held-out half.

Usage (pyarrow comes from nixpkgs; nothing is added to the dev shell):
    nix shell --impure --expr 'with import <nixpkgs> {}; python3.withPackages (p: [ p.pyarrow ])' \\
        -c python3 -I tools/diarize/ami_hf.py [OUT_DIR] [--meetings N]

Writes <OUT_DIR>/<dev|test>/clean/<k>spk/<meeting>.wav and
<OUT_DIR>/<dev|test>/truth/<k>spk/<meeting>.tsv (`speaker<TAB>start<TAB>end`, seconds).
"""
from __future__ import annotations

import argparse
import os
import pathlib
import struct
import subprocess
import wave

import pyarrow.parquet as pq

BASE = "https://huggingface.co/datasets/edinburghcstr/ami/resolve/main"
# One parquet per half is enough for a few meetings; more files mean more meetings.
FILES = {
    "dev": ["validation-00000-of-00005.parquet"],
    "test": ["test-00000-of-00004.parquet"],
}
RATE = 16_000


def fetch(name: str, raw: pathlib.Path) -> pathlib.Path:
    path = raw / name.replace("/", "-")
    if not path.is_file() or path.stat().st_size == 0:
        part = path.with_suffix(".part")
        subprocess.run(["curl", "-fL", "--retry", "3", "-o", str(part), f"{BASE}/{name}"], check=True)  # name includes the config dir
        part.rename(path)
    return path


def pcm(wav_bytes: bytes) -> bytes:
    """16-bit PCM of one segment. The segments are 32-bit float WAV (format 3), which `wave`
    cannot read, so walk the RIFF chunks by hand."""
    fmt = data = None
    pos = 12
    while pos + 8 <= len(wav_bytes):
        cid, size = wav_bytes[pos : pos + 4], struct.unpack_from("<I", wav_bytes, pos + 4)[0]
        body = wav_bytes[pos + 8 : pos + 8 + size]
        if cid == b"fmt ":
            fmt = struct.unpack_from("<HHIIHH", body)
        elif cid == b"data":
            data = body
        pos += 8 + size + (size & 1)
    if fmt is None or data is None:
        raise ValueError("segment has no fmt or data chunk")
    tag, channels, rate, _, _, bits = fmt
    if channels != 1 or rate != RATE:
        raise ValueError(f"expected 16 kHz mono, got {channels} ch at {rate} Hz")
    if tag == 1 and bits == 16:
        return data
    if tag == 3 and bits == 32:
        floats = struct.unpack(f"<{len(data) // 4}f", data[: len(data) // 4 * 4])
        return struct.pack(f"<{len(floats)}h", *(max(-32768, min(32767, int(v * 32767))) for v in floats))
    raise ValueError(f"unsupported WAV format {tag}/{bits}-bit")


def rebuild(table, meeting: str, out: pathlib.Path, half: str, mix: bool) -> None:
    """`mix`: add overlapping segments (ihm: each headset carries only its own voice, so the sum
    is what a call's system channel hears). Otherwise keep the first sample (sdm: one shared mic)."""
    rows = [r for r in table.to_pylist() if r["meeting_id"] == meeting]
    rows.sort(key=lambda r: r["begin_time"])
    speakers = sorted({r["speaker_id"] for r in rows})
    end = max(r["end_time"] for r in rows)
    track = bytearray(int(end * RATE + RATE) * 2)
    filled = bytearray(len(track) // 2)  # 1 where a sample is already placed
    for r in rows:
        data = pcm(r["audio"]["bytes"])
        start = int(r["begin_time"] * RATE)
        for i in range(len(data) // 2):
            j = start + i
            if j >= len(filled):
                break
            new = struct.unpack_from("<h", data, 2 * i)[0]
            if filled[j]:
                if not mix:
                    continue
                new = max(-32768, min(32767, new + struct.unpack_from("<h", track, 2 * j)[0]))
            struct.pack_into("<h", track, 2 * j, new)
            filled[j] = 1
    k = f"{len(speakers)}spk"
    clean = out / half / "clean" / k
    truth = out / half / "truth" / k
    clean.mkdir(parents=True, exist_ok=True)
    truth.mkdir(parents=True, exist_ok=True)
    with wave.open(str(clean / f"{meeting}.wav"), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(bytes(track))
    with open(truth / f"{meeting}.tsv", "w") as f:
        for r in rows:
            f.write(f"{r['speaker_id']}\t{r['begin_time']:.3f}\t{r['end_time']:.3f}\n")
    print(f"{half}\t{k}\t{meeting}\t{len(rows)} segments\t{end / 60:.1f} min")


def main() -> None:
    cache = pathlib.Path(os.environ.get("MEETINGAI_EVAL_CACHE", os.path.expanduser("~/.cache/meetingai-eval")))
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("out_dir", nargs="?", default=None)
    parser.add_argument("--config", choices=("sdm", "ihm"), default="sdm",
                        help="sdm: one room mic (worst case); ihm: headsets mixed, like a call")
    parser.add_argument("--meetings", type=int, default=4, help="meetings per half (default 4)")
    args = parser.parse_args()
    out = pathlib.Path(args.out_dir or cache / ("ami-hf" if args.config == "sdm" else "ami-hf-ihm"))
    raw = out / "raw"
    raw.mkdir(parents=True, exist_ok=True)
    for half, names in FILES.items():
        names = [f"{args.config}/{n}" for n in names]
        done = 0
        for name in names:
            table = pq.read_table(fetch(name, raw), columns=["meeting_id", "speaker_id", "begin_time", "end_time", "audio"])
            for meeting in sorted(set(table.column("meeting_id").to_pylist())):
                if done >= args.meetings:
                    break
                rebuild(table, meeting, out, half, mix=args.config == "ihm")
                done += 1


if __name__ == "__main__":
    main()
