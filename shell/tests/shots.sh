#!/usr/bin/env bash
# Screenshots against fake-daemon.py: overlay streaming, model dropdown with a
# typed id, Library settings, bar menu. Needs a Wayland session and grim.
# usage: shots.sh [OUT_DIR]
set -euo pipefail

here=$(cd "$(dirname "$0")/.." && pwd)
out=${1:-/tmp/meetingai-shots}
tree=${OMARCHY_SHELL_TREE:-/nix/store/gmshfnfmc8wrz4f85f49b3xx1j8a6v0l-nixarchy-omarchy-tree/shell}
work=$(mktemp -d)
trap 'kill $(jobs -p) 2>/dev/null || true; chmod -R u+w "$work"; rm -rf "$work"' EXIT
mkdir -p "$out" "$work/cfg"
cp -rs "$tree"/* "$work/cfg/"
rm -f "$work/cfg/shell.qml"
for f in "$here"/*.qml; do ln -s "$f" "$work/cfg/$(basename "$f")"; done
ln -s "$here/tests/harness-shots.qml" "$work/cfg/shell.qml"

export WAYLAND_DISPLAY=${XDG_RUNTIME_DIR:?}/${WAYLAND_DISPLAY:-wayland-1}
export XDG_RUNTIME_DIR="$work/run"
mkdir -m 700 "$XDG_RUNTIME_DIR"
python3 "$here/tests/fake-daemon.py" "$XDG_RUNTIME_DIR/nixarchy-meetingai/daemon.sock" "$work/fake.log" &
sleep 0.5
timeout 11 quickshell -p "$work/cfg" 2> "$work/qs.err" &
sleep 1.8; grim "$out/overlay-stream.png"
sleep 1.4; grim "$out/overlay-dropdown.png"
sleep 2.6; grim "$out/library-settings.png"
sleep 3.0; grim "$out/bar-menu.png"
wait %2 || true
cat "$work/qs.err" >&2
if grep -E 'ERROR' "$work/qs.err"; then echo "FAIL: QML errors" >&2; exit 1; fi
ls "$out"
