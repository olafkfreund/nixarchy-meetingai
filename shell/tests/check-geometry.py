#!/usr/bin/env python3
"""Check the "GEO {json}" lines tests/geometry.js logged (stdin or a file).

Fails on: a control (button, field, select, toggle, slider) without an Accessible.name; a qs.Ui dropdown (use Select); a button, field or select whose height is not the
shared control height; controls on one visual row whose tops or bottoms (toggles and the slider: centres) differ by more than 1 px; text or buttons wider than their box (clipped);
text that is cut off (elided or past a line limit) where the design does not intend it (UiText.mayCut); text taller than its box; anything sticking out of the card sideways or
downwards; the content of an open Select or mode-picker popup (visited when the harness opens one) sticking out of the window.
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
        if not i["k"] and abs(i["h"] - ctl) > TOL:
            fail(("h", i["t"], i["label"]), f"{i['t']} '{i['label']}' ({i['n']}) is {i['h']:.1f} px tall, control height is {ctl}")
    for i in cs:
        if i.get("acc") == "":
            fail(("acc", i["t"], i["label"], i["n"]), f"{i['t']} '{i['label']}' ({i['n']}) has no Accessible.name")
        if i.get("ui"):
            fail(("ui", i["t"], i["label"]), f"{i['t']} ({i['n']}) is a qs.Ui dropdown: its text ignores the text-size setting; use Select")
    rows = [i for i in cs if not i.get("pop")]   # a popup floats over the rows: it has no row mates
    for a in rows:
        for b in rows:
            if a is b or a["y"] > b["y"] or (a["y"] == b["y"] and a["x"] >= b["x"]):
                continue
            if a["x"] + a["w"] > b["x"] + 0.5 and b["x"] + b["w"] > a["x"] + 0.5:
                continue   # stacked or overlapping horizontally: not the same row
            ov = min(a["y"] + a["h"], b["y"] + b["h"]) - max(a["y"], b["y"])
            if ov < 0.5 * min(a["h"], b["h"]):
                continue
            if a["k"] or b["k"]:
                if abs((a["y"] + a["h"] / 2) - (b["y"] + b["h"] / 2)) > TOL:
                    fail(("row", a["label"], b["label"]), f"row misaligned: '{a['label']}' centre {a['y'] + a['h'] / 2:.1f} vs '{b['label']}' centre {b['y'] + b['h'] / 2:.1f}")
                continue
            if abs(a["fpx"] - b["fpx"]) > 0.5:
                fail(("font", a["label"], b["label"]), f"font sizes differ in one row: '{a['label']}' {a['fpx']:.1f} px vs '{b['label']}' {b['fpx']:.1f} px")
            if abs(a["y"] - b["y"]) > TOL or abs((a["y"] + a["h"]) - (b["y"] + b["h"])) > TOL:
                fail(("row", a["label"], b["label"]), f"row misaligned: '{a['label']}' y={a['y']:.1f}..{a['y']+a['h']:.1f} vs '{b['label']}' y={b['y']:.1f}..{b['y']+b['h']:.1f}")
    for i in its:
        if i.get("clip"):
            fail(("clip", i["t"], i["label"]), f"clipped: {i['t']} '{i['label']}' is wider than its {i['w']:.0f} px box")
        if i.get("cut"):
            fail(("cut", i["t"], i["label"]), f"text is cut off: {i['t']} '{i['label']}' ({i['w']:.0f} px wide)")
        if i.get("vo"):
            fail(("vo", i["t"], i["label"]), f"text taller than its box: {i['t']} '{i['label']}' ({i['h']:.1f} px high, its content {i['ch']:.1f}, implicit {i['ih']:.1f})")
        if i.get("pop"):   # a popup is judged against the window, not the card
            ww, wh, ox, oy = snap["ww"], snap["wh"], snap["ox"], snap["oy"]
            if ww and (ox + i["x"] < -TOL or ox + i["x"] + i["w"] > ww + TOL or oy + i["y"] < -TOL or oy + i["y"] + i["h"] > wh + TOL):
                fail(("popout", i["t"], i["label"]), f"popup outside the window: {i['t']} '{i['label']}' at {ox + i['x']:.0f},{oy + i['y']:.0f} {i['w']:.0f}x{i['h']:.0f}, window {ww:.0f}x{wh:.0f}")
        elif i["x"] < -TOL or i["x"] + i["w"] > cw + TOL or i["y"] < -TOL or i["y"] + i["h"] > snap["h"] + TOL:
            fail(("out", i["t"], i["label"]), f"outside the card: {i['t']} '{i['label']}' x={i['x']:.0f}..{i['x']+i['w']:.0f} y={i['y']:.0f}..{i['y']+i['h']:.0f}, card {cw:.0f}x{snap['h']:.0f}")


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
