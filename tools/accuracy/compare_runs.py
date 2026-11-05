#!/usr/bin/env python3
"""Compare two runs scored with `score.py SET_DIR --from-logs ... --json-out FILE`.

Usage: python3 -I compare_runs.py A.json B.json

Prints each run's WER per channel and overall, the difference (B - A, in points), the number of
calls where B is worse, the lines A has that B lacks (matched on channel and identical start/end,
with the rule inferred from the text), and the seconds of turn audio no chunk span covers.
"""
import importlib.util
import json
import sys
from pathlib import Path

sys.dont_write_bytecode = True
_spec = importlib.util.spec_from_file_location("score", Path(__file__).resolve().with_name("score.py"))
score = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(score)


def rate(edits: int, ref: int) -> float | None:
    return edits / ref if ref else None


def totals(calls: dict) -> dict:
    t = {k: sum(c[k] for c in calls.values()) for k in ("edits_me", "ref_me", "edits_them", "ref_them")}
    t["me"] = rate(t["edits_me"], t["ref_me"])
    t["them"] = rate(t["edits_them"], t["ref_them"])
    t["all"] = rate(t["edits_me"] + t["edits_them"], t["ref_me"] + t["ref_them"])
    return t


def call_wer(c: dict) -> float:
    return rate(c["edits_me"] + c["edits_them"], c["ref_me"] + c["ref_them"]) or 0.0


def key(o: dict) -> tuple:
    return (o["channel"], o["start"], o["end"])


def removed(a: list[dict], b: list[dict]) -> list[tuple[dict, str]]:
    """Lines of a missing from b, with the rule that plausibly removed each."""
    kept = {key(o) for o in b}
    out, seen = [], set()
    for o in sorted(a, key=lambda o: (o["channel"], o["start"])):
        text = " ".join(score.words(o["text"]))
        if key(o) not in kept:
            if (o["channel"], text) in seen:
                rule = "exact repeat"
            elif any(p in o["text"].lower() for p in score.PHANTOMS):
                rule = "silence phrase"
            else:
                rule = "echo or loop"
            out.append((o, rule))
        seen.add((o["channel"], text))
    return out


def uncovered(call: dict) -> float | None:
    """Seconds of turn audio under no chunk span (upper bound: includes silence before the first word)."""
    if not call["chunks"] or not call["turns"]:
        return None
    total = 0.0
    for t in call["turns"]:
        spans = sorted((c["start"], c["end"]) for c in call["chunks"] if c["channel"] == t["channel"])
        at = t["start"]
        for s, e in spans:
            if e <= at or s >= t["end"]:
                continue
            total += max(0.0, s - at)
            at = max(at, e)
        total += max(0.0, t["end"] - at)
    return total


def main(path_a: str, path_b: str) -> None:
    a, b = (json.loads(Path(p).read_text(encoding="utf-8")) for p in (path_a, path_b))
    for set_name in sorted(set(a) & set(b)):
        names = sorted(set(a[set_name]) & set(b[set_name]))
        ca, cb = ({n: r[set_name][n] for n in names} for r in (a, b))
        ta, tb = totals(ca), totals(cb)
        print(f"[{set_name}] {len(names)} calls")
        for ch in ("me", "them", "all"):
            d = None if ta[ch] is None or tb[ch] is None else (tb[ch] - ta[ch]) * 100
            print(f"  {ch:>4}: A {score.pct(ta[ch])}  B {score.pct(tb[ch])}  diff {'n/a' if d is None else f'{d:+.1f} points'}")
        worse = sum(call_wer(cb[n]) > call_wer(ca[n]) for n in names)
        print(f"  B worse in {worse} of {len(names)} calls")
        for label, calls in (("A", ca), ("B", cb)):
            u = [uncovered(c) for c in calls.values()]
            u = [x for x in u if x is not None]
            if u:
                print(f"  {label}: turn audio under no chunk span: {sum(u):.1f}s (upper bound, includes silence before the first word)")
        for n in names:
            for o, rule in removed(ca[n]["lines"], cb[n]["lines"]):
                print(f"  removed {n} ch{o['channel']} [{o['start']:.1f}-{o['end']:.1f}s] ({rule}): {o['text']}")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2])
