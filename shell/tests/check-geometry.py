#!/usr/bin/env python3
"""Check the "GEO {json}" lines tests/geometry.js logged (stdin or a file).

Fails on: a control (button, field, dropdown) whose height is not the shared control height;
controls on one visual row whose tops or bottoms differ by more than 1 px; text or buttons
wider than their box (clipped); anything sticking out of the card sideways.
usage: check-geometry.py geometry.log
"""
import json, sys

TOL = 1.5
bad = {}


def fail(key, msg):
    bad.setdefault(key, msg)


def check(snap):
    ctl, cw = snap["ctl"], snap["w"]
    its = snap["items"]
    cs = [i for i in its if i.get("c")]
    for i in cs:
        if abs(i["h"] - ctl) > TOL:
            fail(("h", i["t"], i["label"]), f"{i['t']} '{i['label']}' ({i['n']}) is {i['h']:.1f} px tall, control height is {ctl}")
    for a in cs:
        for b in cs:
            if a is b or a["y"] > b["y"] or (a["y"] == b["y"] and a["x"] >= b["x"]):
                continue
            if a["x"] + a["w"] > b["x"] + 0.5 and b["x"] + b["w"] > a["x"] + 0.5:
                continue   # stacked or overlapping horizontally: not the same row
            ov = min(a["y"] + a["h"], b["y"] + b["h"]) - max(a["y"], b["y"])
            if ov < 0.5 * min(a["h"], b["h"]):
                continue
            if abs(a["y"] - b["y"]) > TOL or abs((a["y"] + a["h"]) - (b["y"] + b["h"])) > TOL:
                fail(("row", a["label"], b["label"]), f"row misaligned: '{a['label']}' y={a['y']:.1f}..{a['y']+a['h']:.1f} vs '{b['label']}' y={b['y']:.1f}..{b['y']+b['h']:.1f}")
    for i in its:
        if i.get("clip"):
            fail(("clip", i["t"], i["label"]), f"clipped: {i['t']} '{i['label']}' is wider than its {i['w']:.0f} px box")
        if i["x"] < -TOL or i["x"] + i["w"] > cw + TOL:
            fail(("out", i["t"], i["label"]), f"outside the card: {i['t']} '{i['label']}' x={i['x']:.0f}..{i['x']+i['w']:.0f}, card width {cw:.0f}")


n = 0
for line in open(sys.argv[1]) if len(sys.argv) > 1 else sys.stdin:
    k = line.find("GEO {")
    if k < 0:
        continue
    check(json.loads(line[k + 4:]))
    n += 1
if n == 0:
    sys.exit("check-geometry: no GEO lines in the log")
for m in bad.values():
    print("FAIL", m)
print(f"check-geometry: {n} snapshots, {len(bad)} problems")
sys.exit(1 if bad else 0)
