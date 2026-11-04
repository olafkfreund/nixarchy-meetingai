#!/usr/bin/env python3
"""Assert-based self-check for the #73 tools. Synthetic data only, no downloads, no engine.

Run: python3 -I tools/accuracy/selfcheck.py
"""
import array
import importlib.util
import json
import math
import pathlib
import sys
import tempfile
import wave

sys.dont_write_bytecode = True  # `-I` still writes caches; keep them out of the tree
HERE = pathlib.Path(__file__).resolve().parent


def load(name: str):
    spec = importlib.util.spec_from_file_location(name, HERE / f"{name}.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


nm = load("no_mix")
RATE = nm.RATE


def tone(n: int) -> array.array:
    return array.array("h", (int(8000 * math.sin(i / 7)) for i in range(n)))


def pool(n: int) -> list[dict]:
    return [
        {"id": f"u{i:03d}", "speaker": f"s{i % 2}", "text": f"word{i} other{i}",
         "duration": 0.5 + 0.01 * i, "samples": tone(int((0.5 + 0.01 * i) * RATE))}
        for i in range(n)
    ]


def frames(path: str) -> int:
    with wave.open(path, "rb") as w:
        return w.getnframes()


def check_turns_and_determinism(tmp: pathlib.Path) -> None:
    items = pool(24)
    a = nm.build_calls(items, tmp / "a", seed=1, max_calls=2)
    b = nm.build_calls(items, tmp / "b", seed=1, max_calls=2)
    assert len(a) == 2
    for ca, cb in zip(a, b):
        turns = json.loads(pathlib.Path(ca["wav"]).with_suffix(".turns.json").read_text())
        assert len(turns) == 10
        assert [t["channel"] for t in turns] == [1, 0] * 5
        # last turn ends GAP + TAIL before the end of the file
        assert frames(ca["wav"]) == round(turns[-1]["end"] * RATE) + nm.GAP + nm.TAIL
        assert all(t["end"] > t["start"] >= 0 for t in turns)
        assert pathlib.Path(ca["wav"]).read_bytes() == pathlib.Path(cb["wav"]).read_bytes()
        assert pathlib.Path(ca["wav"]).with_suffix(".turns.json").read_bytes() == \
            pathlib.Path(cb["wav"]).with_suffix(".turns.json").read_bytes()
    dt = nm.build_calls(pool(40), tmp / "dt", seed=1, max_calls=1, double_talk=True)
    dturns = json.loads(pathlib.Path(dt[0]["wav"]).with_suffix(".turns.json").read_text())
    assert len(dturns) == 12 and max(t["end"] for t in dturns) * RATE <= frames(dt[0]["wav"])
    assert sorted(nm.used_ids(a)) == sorted(set(nm.used_ids(a))) and len(nm.used_ids(a)) == 20


def check_exclude_and_supply(tmp: pathlib.Path) -> None:
    root = tmp / "LibriSpeech" / "test-clean" / "1" / "2"
    root.mkdir(parents=True)
    ids = [f"1-2-{i:04d}" for i in range(6)]
    for i in ids:
        nm.write_mono_wav(root / f"{i}.wav", tone(RATE))
    (root / "1-2.trans.txt").write_text("".join(f"{i} HELLO WORLD\n" for i in ids), encoding="utf-8")
    full = nm.load_librispeech(tmp / "LibriSpeech")
    assert [r["id"] for r in full] == ids and full[0]["text"] == "hello world"
    assert abs(full[0]["duration"] - 1.0) < 1e-6
    kept = nm.load_librispeech(tmp / "LibriSpeech", frozenset(ids[:2]))
    assert [r["id"] for r in kept] == ids[2:]
    try:
        nm.build_measured(pool(24), tmp / "small", seed=1, calls=20)
    except SystemExit as e:
        assert e.code != 0 and "of 20" in str(e.code)
    else:
        raise AssertionError("a pool too small for 20 calls must exit non-zero")


with tempfile.TemporaryDirectory() as d:
    check_turns_and_determinism(pathlib.Path(d) / "t")
    check_exclude_and_supply(pathlib.Path(d) / "e")
print("selfcheck: ok")
