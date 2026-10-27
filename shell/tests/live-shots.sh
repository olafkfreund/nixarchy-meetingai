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
export WAYLAND_DISPLAY=${XDG_RUNTIME_DIR:?}/${WAYLAND_DISPLAY:-wayland-1}
export XDG_RUNTIME_DIR="$work/run"
mkdir -m 700 "$XDG_RUNTIME_DIR"
python3 "$here/tests/fake-daemon.py" "$XDG_RUNTIME_DIR/nixarchy-meetingai/daemon.sock" "$work/fake.log" --live &
sleep 0.5
timeout 16 quickshell -p "$work/cfg" 2> "$work/qs.err" &
for t in 1-question 2-answering 3-suggestions 4-you-speaking; do
    case $t in 1-*) sleep 3.4 ;; 2-*) sleep 2.4 ;; 3-*) sleep 3.0 ;; 4-*) sleep 2.2 ;; esac
    grim "$out/live-$t.png"
done
wait %2 || true
if grep -E 'ERROR' "$work/qs.err"; then cat "$work/qs.err" >&2; exit 1; fi
ls "$out"
