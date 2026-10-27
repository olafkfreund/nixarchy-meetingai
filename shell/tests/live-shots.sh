#!/usr/bin/env bash
# Frames of a live meeting against fake-daemon.py --live: partial words, a
# question from the other side, a streamed answer, then suggestions.
# usage: [SCENARIO=live|k8s|team|interviewer] [SCALE=100] live-shots.sh [OUT_DIR]   (needs a Wayland session and grim)
# SCENARIO=k8s plays the captured real answer (fake-daemon.py --k8s); SCALE is the text_scale setting.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
out=${1:-/tmp/meetingai-live}
scenario=${SCENARIO:-live}; [ "$scenario" = error ] && export MEETINGAI_ASK=1; scale=${SCALE:-100}; size=${SIZE:-}   # SCENARIO=r5 SIZE=700x520: resized card, Profiles and Prompts tabs
tree=${OMARCHY_SHELL_TREE:-/nix/store/gmshfnfmc8wrz4f85f49b3xx1j8a6v0l-nixarchy-omarchy-tree/shell}
work=$(mktemp -d)
trap 'kill $(jobs -p) 2>/dev/null || true; chmod -R u+w "$work"; rm -rf "$work"' EXIT
mkdir -p "$out" "$work/cfg"
cp -rs "$tree"/* "$work/cfg/"
rm -f "$work/cfg/shell.qml"
for f in "$here"/*.qml; do ln -s "$f" "$work/cfg/$(basename "$f")"; done
ln -s "$here/tests/geometry.js" "$work/cfg/geometry.js"
ln -s "$here/tests/harness-$([ "$scenario" = r5 ] && echo r5 || echo live).qml" "$work/cfg/shell.qml"
real_run=$XDG_RUNTIME_DIR
export WAYLAND_DISPLAY=${XDG_RUNTIME_DIR:?}/${WAYLAND_DISPLAY:-wayland-1}
export XDG_RUNTIME_DIR="$work/run"
mkdir -m 700 "$XDG_RUNTIME_DIR"
if [ "$scenario" != no-daemon ]; then   # no-daemon: nothing listens, to see the Setup card
    python3 "$here/tests/fake-daemon.py" "$XDG_RUNTIME_DIR/nixarchy-meetingai/daemon.sock" "$work/fake.log" $([ "$scenario" = r5 ] || echo --$scenario) --scale=$scale ${size:+--size=$size} &
fi
sleep 0.5
timeout 30 quickshell -p "$work/cfg" 2> "$work/qs.err" | tee "$work/qs.out" | grep -av "GEO {" &   # quickshell logs (console.log too) to stdout
qs=$!
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
x, y, w, h = map(float, sys.argv[1:5])
cw, ch = min(w - 80, 900), min(h - 80, 760)   # Overlay.qml default card size
if len(sys.argv) > 5 and "x" in sys.argv[5]:
    cw, ch = map(float, sys.argv[5].split("x"))
print("%d,%d %dx%d" % (x + (w - cw) / 2, y + (h - ch) / 2, cw, ch))' $layer $size
}
sleep 3.4; g=$(geom)
# name:seconds to wait before the crop (the harness opens Settings at 16 s)
case $scenario in
    r5) frames="1-resized:1 2-profiles:4.5 3-prompts:4" ;;
    first-run) frames="1-setup:0.5" ;;
    no-daemon) frames="1-no-daemon:0.5" ;;
    error) frames="1-waiting:11.2 2-failed:4" ;;
    team) frames="1-items:3 2-last-time:6 3-settings:4" ;;
    interviewer) frames="1-consent:0.1 2-bank:5.5 3-candidate:9.5 4-new:2 5-attach:2 6-delete:2" ;;   # banner at ~3.5 s, bank after consent at 6 s, Candidate tab after 16 s
    k8s) frames="1-answering:2 2-ready:9 3-settings:4 4-mode:2" ;;
    *) frames="1-question:1 2-answering:2.4 3-suggestions:3 4-you-speaking:2.2 5-asked:2.4 6-settings:4 7-mode:2" ;;
esac
for f in $frames; do
    sleep "${f#*:}"
    grim -g "$g" "$out/crop-${f%:*}.png"
done
wait "$qs" || true
# MEETINGAI_GEOMETRY=1: keep the geometry dumps for tests/check-geometry.py
if [ "${MEETINGAI_GEOMETRY:-}" = 1 ]; then grep -a "GEO {" "$work/qs.out" > "$out/geometry.log" || true; fi
if grep -E 'ERROR' "$work/qs.err"; then cat "$work/qs.err" >&2; exit 1; fi
ls "$out"
