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


sc = load("score")


def line(ch: int, start: float, end: float, text: str, rel=None) -> dict:
    return {"kind": "line", "channel": ch, "start": start, "end": end, "released_at": rel, "text": text}


def check_scoring() -> None:
    me, them = ["hello", "there", "friend"], ["good", "morning", "everyone", "today"]
    recs = [
        line(0, 0, 1, "hello there frend", 1.5),   # one misheard owner word
        line(0, 5, 6, "good morning", 7.0),         # bleed of a remote turn on the mic
        line(1, 2, 3, "good morning everyone today", 3.5),
        line(1, 8, 9, "extra", 11.0),
    ]
    m = sc.score_ext(recs, me, them, [])
    assert m["edits"] == m["edits_me"] + m["edits_them"] and m["ref_total"] == m["ref_me"] + m["ref_them"] == 7
    assert (m["edits_me"], m["edits_them"]) == (1 + 2, 1)  # mic hyp has 2 extra words; sys has "extra"
    assert abs(m["wer"] - m["edits"] / 7) < 1e-9 and abs(m["wer_me"] - 3 / 3) < 1e-9
    assert "friend" in m["missing_words"] and "lost" not in m
    assert m["notices"] == 0 and not m["invalid"]
    assert sorted(m["lags"][0]) == [0.5, 1.0] and sorted(m["lags"][1]) == [0.5, 2.0]
    assert sc.stl_text(m["lags"]).startswith("me median 0.75s p95 1.00s")
    assert sc.score_ext(recs, me, them, [], paced=False)["lags"] == {0: [], 1: []}
    assert sc.stl_text({0: [], 1: []}) == "me n/a, them n/a"
    assert sc.score_ext([line(0, 0, 1, "x")], me, them, [])["lags"] == {0: [], 1: []}  # null released_at
    for text, bad in (("lost speech for 3.0s on the microphone", True), ("chunk decode failed: x", True), ("model loaded", False)):
        n = sc.score_ext(recs + [{"kind": "notice", "text": text}], me, them, [])
        assert n["notices"] == 1 and n["invalid"] is bad, text
    assert sc.is_paced([]) and sc.is_paced(["--speed", "1"])
    assert not sc.is_paced(["--speed", "8"]) and not sc.is_paced(["--no-filter"]) and not sc.is_paced(["--turns", "t"])


def check_from_logs_and_compare(tmp: pathlib.Path) -> None:
    import contextlib
    import io

    suite = tmp / "suite"
    (suite / "s").mkdir(parents=True)
    for name in ("c1", "c2"):
        (suite / "s" / f"{name}.wav").write_bytes(b"")
        (suite / "s" / f"{name}.txt").write_text("me: hello there\nthem: good morning\n", encoding="utf-8")
        (suite / "s" / f"{name}.turns.json").write_text(json.dumps([
            {"id": "a", "channel": 0, "start": 0.0, "end": 2.0, "text": "hello there"},
            {"id": "b", "channel": 1, "start": 3.0, "end": 5.0, "text": "good morning"},
        ]), encoding="utf-8")
    (suite / "manifest.json").write_text(json.dumps({"s": [
        {"name": n, "wav": str(suite / "s" / f"{n}.wav"), "txt": str(suite / "s" / f"{n}.txt"), "duration": 6.0}
        for n in ("c1", "c2")
    ]}), encoding="utf-8")

    def write_logs(d: pathlib.Path, extra: list[dict]) -> pathlib.Path:
        (d / "s").mkdir(parents=True)
        for n in ("c1", "c2"):
            recs = [
                {"kind": "chunk", "channel": 0, "start": 0.0, "end": 2.0},
                {"kind": "chunk", "channel": 1, "start": 3.0, "end": 4.0},
                line(0, 0.0, 2.0, "hello there"),
                line(1, 3.0, 4.0, "good morning"),
            ] + (extra if n == "c1" else [])
            (d / "s" / f"{n}.jsonl").write_text("".join(json.dumps(r) + "\n" for r in recs), encoding="utf-8")
        return d

    a_logs = write_logs(tmp / "a", [line(1, 4.5, 5.0, "good morning")])  # a repeat that B+F would remove
    b_logs = write_logs(tmp / "b", [])
    out = {}
    for tag, logs in (("a", a_logs), ("b", b_logs)):
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            sc.score_suite(suite, ["--no-filter"], from_logs=logs, json_out=tmp / f"{tag}.json")
        out[tag] = buf.getvalue()
    assert "[s] files=2" in out["b"] and " WER=0.0% (0/8)" in out["b"] and "missing=0" in out["b"], out["b"]
    assert "WER_me=0.0%" in out["b"] and "stl=[me n/a, them n/a]" in out["b"], out["b"]
    assert " WER=25.0% (2/8)" in out["a"], out["a"]  # the repeat is an insertion
    cr = load("compare_runs")
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        cr.main(str(tmp / "a.json"), str(tmp / "b.json"))
    text = buf.getvalue()
    assert "removed c1 ch1 [4.5-5.0s] (exact repeat): good morning" in text, text
    assert "B worse in 0 of 2 calls" in text and "all: A 25.0%  B 0.0%  diff -25.0 points" in text, text
    assert "A: turn audio under no chunk span: 2.0s" in text and "upper bound" in text, text
    try:
        sc.score_suite(suite, [], from_logs=tmp / "nowhere")
    except SystemExit as e:
        assert "missing log" in str(e.code)
    else:
        raise AssertionError("a missing log must stop the run")


with tempfile.TemporaryDirectory() as d:
    check_turns_and_determinism(pathlib.Path(d) / "t")
    check_exclude_and_supply(pathlib.Path(d) / "e")
    check_scoring()
    check_from_logs_and_compare(pathlib.Path(d) / "f")
print("selfcheck: ok")
