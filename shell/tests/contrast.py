#!/usr/bin/env python3
"""WCAG contrast of the overlay's tinted fills, composited like the QML.

usage: contrast.py [--all | colors.toml ...]   (default: the current theme + the light themes flexoki-light, catppuccin-latte, white; --all: every theme)
Fills are Qt.alpha(role, a) over the opaque popups background (popups.background = background,
popups.text = foreground unless a theme's shell.toml overrides them; this reads colors.toml only).
Keep the alphas below in step with shell/Overlay.qml. Exit 1 if any text pair is under 4.5:1
or the recording dot (shell/Rec.qml) is under 3:1 on the header or the bar.
"""
import os
import sys
import tomllib

TREE = "/nix/store/gmshfnfmc8wrz4f85f49b3xx1j8a6v0l-nixarchy-omarchy-tree/themes"
DEFAULT = [os.path.expanduser("~/.local/state/omarchy/current/theme/colors.toml"),
           f"{TREE}/flexoki-light/colors.toml", f"{TREE}/catppuccin-latte/colors.toml", f"{TREE}/white/colors.toml"]
MEETING, YOU, CARD = 0.06, 0.14, 0.12   # Transcript.qml / NowCard.qml: bubbles and the NOW card (archived answers use 0.08)
FALLBACK_RED = "#e5484d"


def rgb(h):
    h = h.lstrip("#")
    return tuple(int(h[i:i + 2], 16) / 255 for i in (0, 2, 4))


def over(fg, a, bg):
    return tuple(a * f + (1 - a) * b for f, b in zip(fg, bg))


def lum(c):
    lin = [x / 12.92 if x <= 0.03928 else ((x + 0.055) / 1.055) ** 2.4 for x in c]
    return 0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2]


def ratio(a, b):
    la, lb = sorted((lum(a), lum(b)), reverse=True)
    return (la + 0.05) / (lb + 0.05)


def record_red(c, bg, fg):
    """Rec.qml: the vivid fallback red if it reads at 3:1 on the background, else the theme's urgent, else the foreground."""
    for cand in (rgb(FALLBACK_RED), rgb(c["urgent"]) if "urgent" in c else rgb("#a55555")):
        if cand is not None and ratio(cand, bg) >= 3:
            return cand
    return fg


def check(path):
    c = tomllib.load(open(path, "rb"))
    bg, fg, ac = rgb(c["background"]), rgb(c["foreground"]), rgb(c["accent"])
    rows = [
        ("text on meeting bubble", fg, over(fg, MEETING, bg)),
        ("text on you bubble", fg, over(ac, YOU, bg)),
        ("text on NOW/answer card", fg, over(ac, CARD, bg)),
        ("text on suggestion hover", fg, over(fg, 0.08, bg)),
        ("badge outline/number (mono)", fg, bg),
        ("level dot (mono, full level)", fg, bg),
        ("link (accent) on card", ac, over(ac, CARD, bg)),
        ("recording dot on header/bar", record_red(c, bg, fg), bg),
    ]
    print(f"{path}  ({c.get('mode', '?')})")
    ok = True
    for name, a, b in rows:
        r = ratio(a, b)
        floor = 3 if name.startswith("recording dot") else 4.5   # a dot is a graphic, not text
        bad = r < floor and not name.startswith("link")   # links also carry underline; reported only
        ok &= not bad
        print(f"  {name:28s} {r:5.2f}:1 {'FAIL' if bad else ''}")
    return ok


if __name__ == "__main__":
    args = sys.argv[1:]
    if args == ["--all"]:
        args = sorted(os.path.join(TREE, d, "colors.toml") for d in os.listdir(TREE) if os.path.exists(os.path.join(TREE, d, "colors.toml")))
    sys.exit(0 if all([check(p) for p in (args or DEFAULT)]) else 1)
