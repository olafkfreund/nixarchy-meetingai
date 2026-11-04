#!/usr/bin/env python3
"""Build synthetic multi-speaker conversations from LibriSpeech test-clean (#61 step 3).

Usage:
    python3 -I tools/diarize/librispeech_convs.py [LIBRI_DIR] [OUT_DIR] [--per-count N] [--speakers 2,3,4]

LIBRI_DIR may be a directory holding <speaker>/<chapter>/<utt>.(flac|wav) or a parent
containing LibriSpeech/test-clean or wav/. Defaults:
    LIBRI_DIR = ~/.cache/meetingai-eval/librispeech/LibriSpeech/test-clean
    OUT_DIR   = ~/.cache/meetingai-eval/librispeech/convs

Writes 16 kHz mono 16-bit PCM WAVs and truth TSVs (`speaker<TAB>start<TAB>end` in
seconds, per voiced turn) under:
    <OUT_DIR>/<dev|test>/clean/<2|3|4>spk/<conv_id>.wav
    <OUT_DIR>/<dev|test>/clean/<2|3|4>spk/<conv_id>.tsv
    <OUT_DIR>/<dev|test>/truth/<2|3|4>spk/<conv_id>.tsv
"""
from __future__ import annotations

import argparse
import array
import os
import pathlib
import random
import shutil
import subprocess
import sys
import wave

RATE = 16_000
SEED = 61

# Fixed speaker split across the 40 speakers of LibriSpeech test-clean (20 dev, 20 test).
# Balanced by gender (10 F + 10 M in each half); no speaker appears in both halves.
DEV_SPEAKERS: tuple[str, ...] = (
    "61",
    "121",
    "237",
    "260",
    "672",
    "908",
    "1089",
    "1188",
    "1221",
    "1284",
    "1320",
    "1580",
    "1995",
    "2094",
    "2300",
    "2830",
    "2961",
    "3570",
    "3575",
    "3729",
)

TEST_SPEAKERS: tuple[str, ...] = (
    "4077",
    "4446",
    "4507",
    "4970",
    "4992",
    "5105",
    "5142",
    "5639",
    "5683",
    "6829",
    "6930",
    "7021",
    "7127",
    "7176",
    "7729",
    "8224",
    "8230",
    "8455",
    "8463",
    "8555",
)

assert not (set(DEV_SPEAKERS) & set(TEST_SPEAKERS)), "dev and test speakers must be disjoint"

TURN_MIN_S = 1.0
TURN_MAX_S = 8.0
GAP_MIN_S = 0.2
GAP_MAX_S = 1.0
OVERLAP_S = 0.3
OVERLAP_PROB = 0.20
TURNS_PER_CONV = 16


def resolve_libri_root(path: pathlib.Path) -> pathlib.Path:
    for candidate in (
        path,
        path / "LibriSpeech" / "test-clean",
        path / "test-clean",
        path / "wav",
    ):
        if candidate.is_dir() and any(
            (candidate / spk).is_dir() for spk in DEV_SPEAKERS[:3] + TEST_SPEAKERS[:3]
        ):
            return candidate
    raise SystemExit(f"librispeech_convs.py: could not find speaker folders under {path}")


def collect_utterances(root: pathlib.Path, speakers: tuple[str, ...]) -> dict[str, list[pathlib.Path]]:
    utts: dict[str, list[pathlib.Path]] = {}
    for spk in speakers:
        spk_dir = root / spk
        if not spk_dir.is_dir():
            continue
        files: list[pathlib.Path] = []
        for ch_dir in sorted(p for p in spk_dir.iterdir() if p.is_dir()):
            for f in sorted(ch_dir.iterdir()):
                if f.suffix.lower() in (".flac", ".wav"):
                    files.append(f)
        if files:
            utts[spk] = files
    return utts


def decode_16k_mono(path: pathlib.Path, cache: dict[pathlib.Path, array.array]) -> array.array:
    cached = cache.get(path)
    if cached is not None:
        return cached
    if path.suffix.lower() == ".wav":
        with wave.open(str(path), "rb") as w:
            if w.getframerate() == RATE and w.getnchannels() == 1 and w.getsampwidth() == 2:
                samples = array.array("h")
                samples.frombytes(w.readframes(w.getnframes()))
                if sys.byteorder != "little":
                    samples.byteswap()
                cache[path] = samples
                return samples
    cmd = [
        "ffmpeg",
        "-nostdin",
        "-v",
        "error",
        "-i",
        str(path),
        "-f",
        "s16le",
        "-acodec",
        "pcm_s16le",
        "-ac",
        "1",
        "-ar",
        str(RATE),
        "-",
    ]
    raw = subprocess.run(cmd, check=True, stdout=subprocess.PIPE).stdout
    samples = array.array("h")
    samples.frombytes(raw)
    if sys.byteorder != "little":
        samples.byteswap()
    cache[path] = samples
    return samples


def trim_silence(samples: array.array, rms_thresh: float = 0.005) -> array.array:
    frame = int(RATE * 0.030)
    if len(samples) < frame:
        return samples
    thresh_sq = (rms_thresh * 32768.0) ** 2 * frame
    first = 0
    last = len(samples)
    for start in range(0, len(samples) - frame + 1, frame):
        energy = sum(int(s) * int(s) for s in samples[start : start + frame])
        if energy >= thresh_sq:
            first = start
            break
    for start in range(len(samples) - frame, -1, -frame):
        energy = sum(int(s) * int(s) for s in samples[start : start + frame])
        if energy >= thresh_sq:
            last = min(len(samples), start + frame)
            break
    return samples[first:last] if last > first else samples


def pick_turn_samples(
    rng: random.Random,
    spk_files: list[pathlib.Path],
    cursors: dict[str, int],
    spk: str,
    cache: dict[pathlib.Path, array.array],
) -> array.array:
    target_len = int(rng.uniform(TURN_MIN_S, TURN_MAX_S) * RATE)
    min_len = int(TURN_MIN_S * RATE)
    max_len = int(TURN_MAX_S * RATE)

    buf = array.array("h")
    attempts = 0
    while len(buf) < target_len and attempts < len(spk_files) * 2:
        idx = cursors[spk] % len(spk_files)
        cursors[spk] += 1
        attempts += 1
        piece = trim_silence(decode_16k_mono(spk_files[idx], cache))
        if len(piece) < int(0.4 * RATE):
            continue
        if not buf:
            buf.extend(piece)
        else:
            # Tiny 80 ms pause when concatenating two short utterances from the same speaker.
            buf.extend([0] * int(0.08 * RATE))
            buf.extend(piece)
        if len(buf) >= min_len and len(buf) >= target_len:
            break

    if len(buf) > max_len:
        buf = buf[:max_len]
    if len(buf) > target_len and target_len >= min_len:
        buf = buf[:target_len]
    return buf


def place_samples(mix: list[int], start: int, samples: array.array) -> None:
    end = start + len(samples)
    if len(mix) < end:
        mix.extend([0] * (end - len(mix)))
    for i, s in enumerate(samples):
        mix[start + i] += int(s)


def write_wav(path: pathlib.Path, mix: list[int]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    pcm = array.array("h", (max(-32768, min(32767, v)) for v in mix))
    if sys.byteorder != "little":
        pcm.byteswap()
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(pcm.tobytes())


def write_tsv(path: pathlib.Path, turns: list[tuple[str, float, float]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = [f"{spk}\t{start:.3f}\t{end:.3f}" for spk, start, end in turns]
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def build_conversation(
    rng: random.Random,
    chosen_speakers: list[str],
    utts: dict[str, list[pathlib.Path]],
    cursors: dict[str, int],
    cache: dict[pathlib.Path, array.array],
    turns_count: int = TURNS_PER_CONV,
) -> tuple[list[int], list[tuple[str, float, float]]]:
    mix: list[int] = []
    truth: list[tuple[str, float, float]] = []
    cursor_sample = int(0.25 * RATE)  # 250 ms leading quiet
    prev_spk: str | None = None

    for turn_idx in range(turns_count):
        if prev_spk is None:
            spk = rng.choice(chosen_speakers)
        else:
            others = [s for s in chosen_speakers if s != prev_spk]
            spk = rng.choice(others)

        samples = pick_turn_samples(rng, utts[spk], cursors, spk, cache)
        if len(samples) < int(TURN_MIN_S * RATE):
            continue

        if turn_idx == 0:
            start_sample = cursor_sample
        else:
            # Guarantee some 0.3 s overlaps and a spread of 0.2-1.0 s gaps.
            if turn_idx in (3, 9) or (turn_idx > 1 and rng.random() < OVERLAP_PROB):
                overlap = int(OVERLAP_S * RATE)
                start_sample = max(0, cursor_sample - overlap)
            else:
                gap = int(rng.uniform(GAP_MIN_S, GAP_MAX_S) * RATE)
                start_sample = cursor_sample + gap

        end_sample = start_sample + len(samples)
        place_samples(mix, start_sample, samples)
        truth.append((spk, start_sample / RATE, end_sample / RATE))
        cursor_sample = max(cursor_sample, end_sample)
        prev_spk = spk

    mix.extend([0] * int(0.65 * RATE))  # trailing silence so BatchChunks closes the final chunk
    truth.sort(key=lambda row: (row[1], row[2], row[0]))
    return mix, truth


def build_half(
    half: str,
    speaker_list: tuple[str, ...],
    utts: dict[str, list[pathlib.Path]],
    out_dir: pathlib.Path,
    speaker_counts: list[int],
    per_count: int,
    seed_offset: int,
) -> int:
    available = [s for s in speaker_list if s in utts]
    if len(available) < max(speaker_counts):
        raise SystemExit(
            f"librispeech_convs.py: need at least {max(speaker_counts)} speakers for {half}, "
            f"found {len(available)}"
        )

    rng = random.Random(SEED + seed_offset)
    cursors: dict[str, int] = {s: 0 for s in available}
    cache: dict[pathlib.Path, array.array] = {}
    built = 0

    for n_spk in speaker_counts:
        for conv_idx in range(per_count):
            # Rotate through the half's speakers deterministically before random draws so every
            # speaker in the half gets coverage.
            start = (conv_idx * n_spk) % len(available)
            rotated = (available + available)[start : start + n_spk]
            if conv_idx * n_spk >= len(available):
                chosen = rng.sample(available, n_spk)
            else:
                chosen = rotated

            mix, truth = build_conversation(rng, chosen, utts, cursors, cache)
            conv_id = f"{half}_{n_spk}spk_{conv_idx + 1:02d}"

            clean_sub = out_dir / half / "clean" / f"{n_spk}spk"
            truth_sub = out_dir / half / "truth" / f"{n_spk}spk"
            wav_path = clean_sub / f"{conv_id}.wav"
            tsv_path = clean_sub / f"{conv_id}.tsv"
            truth_path = truth_sub / f"{conv_id}.tsv"

            write_wav(wav_path, mix)
            write_tsv(tsv_path, truth)
            write_tsv(truth_path, truth)
            built += 1
            dur = len(mix) / RATE
            print(f"{half}\t{n_spk}spk\t{conv_id}\t{dur:.1f}s\t{len(truth)} turns\t{','.join(chosen)}")

    return built


def main(argv: list[str] | None = None) -> None:
    default_cache = pathlib.Path(
        os.environ.get("MEETINGAI_EVAL_CACHE", os.path.expanduser("~/.cache/meetingai-eval"))
    )
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "libri_dir",
        nargs="?",
        default=str(default_cache / "librispeech" / "LibriSpeech" / "test-clean"),
        help="Path to LibriSpeech test-clean directory (or parent directory)",
    )
    parser.add_argument(
        "out_dir",
        nargs="?",
        default=str(default_cache / "librispeech" / "convs"),
        help="Output directory for generated conversations",
    )
    parser.add_argument(
        "--per-count",
        type=int,
        default=10,
        help="Number of conversations per speaker count in each half (default: 10)",
    )
    parser.add_argument(
        "--speakers",
        default="2,3,4",
        help="Comma-separated speaker counts to generate (default: 2,3,4)",
    )
    parser.add_argument(
        "--half",
        choices=("both", "dev", "test"),
        default="both",
        help="Which speaker split half to build (default: both)",
    )
    args = parser.parse_args(argv)

    if shutil.which("ffmpeg") is None:
        raise SystemExit("librispeech_convs.py: ffmpeg is required on PATH")

    speaker_counts = [int(x) for x in args.speakers.split(",") if x.strip()]
    if any(k < 2 or k > 4 for k in speaker_counts):
        raise SystemExit("librispeech_convs.py: speaker counts must be in 2..=4")

    root = resolve_libri_root(pathlib.Path(args.libri_dir).expanduser())
    out_dir = pathlib.Path(args.out_dir).expanduser()

    total = 0
    if args.half in ("both", "dev"):
        dev_utts = collect_utterances(root, DEV_SPEAKERS)
        total += build_half("dev", DEV_SPEAKERS, dev_utts, out_dir, speaker_counts, args.per_count, 0)
    if args.half in ("both", "test"):
        test_utts = collect_utterances(root, TEST_SPEAKERS)
        total += build_half(
            "test", TEST_SPEAKERS, test_utts, out_dir, speaker_counts, args.per_count, 1000
        )

    print(f"wrote {total} conversations to {out_dir}")


if __name__ == "__main__":
    main()
