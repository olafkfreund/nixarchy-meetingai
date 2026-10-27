#!/usr/bin/env bash
# Frames of a live meeting against fake-daemon.py --live: partial words, a
# question from the other side, a streamed answer, then suggestions.
# usage: live-shots.sh [OUT_DIR]   (needs a Wayland session and grim)
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
out=${1:-/tmp/meetingai-live}
tree=${OMARCHY_SHELL_TREE:-/nix/store/gmshfnfmc8wrz4f85f49b3xx1j8a6v0l-nixarchy-omarchy-tree/shell}
work=$(mktemp -d)
trap 'kill $(jobs -p) 2>/dev/null || true; chmod -R u+w "$work"; rm -rf "$work"' EXIT
mkdir -p "$out" "$work/cfg"
cp -rs "$tree"/* "$work/cfg/"
rm -f "$work/cfg/shell.qml"
for f in "$here"/*.qml; do ln -s "$f" "$work/cfg/$(basename "$f")"; done
ln -s "$here/tests/harness-live.qml" "$work/cfg/shell.qml"
real_run=$XDG_RUNTIME_DIR
export WAYLAND_DISPLAY=${XDG_RUNTIME_DIR:?}/${WAYLAND_DISPLAY:-wayland-1}
export XDG_RUNTIME_DIR="$work/run"
mkdir -m 700 "$XDG_RUNTIME_DIR"
python3 "$here/tests/fake-daemon.py" "$XDG_RUNTIME_DIR/nixarchy-meetingai/daemon.sock" "$work/fake.log" --live &
sleep 0.5
timeout 20 quickshell -p "$work/cfg" 2> "$work/qs.err" &
# Capture only the overlay card (it is centred on its full-screen layer on the
# focused monitor): full-desktop frames would include whatever else is open.
geom() {
    local sig layer
    sig=$(ls -t "$real_run"/hypr | head -1)
    layer=$(XDG_RUNTIME_DIR="$real_run" HYPRLAND_INSTANCE_SIGNATURE=$sig hyprctl -j layers | python3 -c '
import json, sys
for mon in json.load(sys.stdin).values():
    for lv in mon["levels"].values():
        for l in lv:
            if l["namespace"] == "nixarchy-meetingai":
                print(l["x"], l["y"], l["w"], l["h"])')
    python3 -c '
import sys
x, y, w, h = map(float, sys.argv[1:])
cw, ch = min(w - 80, 900), min(h - 80, 760)   # Overlay.qml card size at scale 1
print("%d,%d %dx%d" % (x + (w - cw) / 2, y + (h - ch) / 2, cw, ch))' $layer
}
sleep 3.4; g=$(geom)
for t in 1-question 2-answering 3-suggestions 4-you-speaking 5-asked 6-settings; do
    case $t in 1-*) sleep 1 ;; 2-*) sleep 2.4 ;; 3-*) sleep 3.0 ;; 4-*) sleep 2.2 ;; 5-*) sleep 2.4 ;; 6-*) sleep 4 ;; esac
    grim -g "$g" "$out/crop-$t.png"
done
wait %2 || true
if grep -E 'ERROR' "$work/qs.err"; then cat "$work/qs.err" >&2; exit 1; fi
ls "$out"
