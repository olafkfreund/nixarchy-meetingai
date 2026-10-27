#!/usr/bin/env bash
# Smoke test: run the plugin against fake-daemon.py inside a real quickshell
# for 5 s, fail on any QML error, and check the call-offer logic both ways.
# Needs a running Wayland session (the overlay is a layer-shell window).
set -euo pipefail

here=$(cd "$(dirname "$0")/.." && pwd)
tree=${OMARCHY_SHELL_TREE:-/nix/store/gmshfnfmc8wrz4f85f49b3xx1j8a6v0l-nixarchy-omarchy-tree/shell}
work=$(mktemp -d)
trap 'kill $(jobs -p) 2>/dev/null || true; chmod -R u+w "$work"; rm -rf "$work"' EXIT

# qs.Commons / qs.Ui come from the omarchy shell tree; our files sit beside them.
mkdir "$work/cfg"
cp -rs "$tree"/* "$work/cfg/"
rm -f "$work/cfg/shell.qml"
for f in "$here"/*.qml; do ln -s "$f" "$work/cfg/$(basename "$f")"; done
ln -s "$here/tests/harness.qml" "$work/cfg/shell.qml"

# notify-send stub: records the call, never blocks.
mkdir "$work/bin"
printf '#!/bin/sh\necho "$@" >> "%s/notify.log"\n' "$work" > "$work/bin/notify-send"
chmod +x "$work/bin/notify-send"

# The Wayland socket lives in the real runtime dir; the fake gets its own.
export WAYLAND_DISPLAY=${XDG_RUNTIME_DIR:?}/${WAYLAND_DISPLAY:-wayland-1}
export PATH="$work/bin:$PATH"

run() { # run <label> [--auto]
    local label=$1 auto=${2:-}
    export XDG_RUNTIME_DIR="$work/run-$label"
    mkdir -m 700 "$XDG_RUNTIME_DIR"
    : > "$work/$label.log"
    python3 "$here/tests/fake-daemon.py" "$XDG_RUNTIME_DIR/nixarchy-meetingai/daemon.sock" "$work/$label.log" $auto &
    local fake=$!
    sleep 0.5
    timeout 5 quickshell -p "$work/cfg" 2> "$work/$label.err" || true &
    local qs=$!
    sleep 2.5
    echo '{"active":true}' > "$XDG_RUNTIME_DIR/meeting-mode.json"
    wait "$qs" || true
    kill "$fake" 2>/dev/null || true
    if grep -E 'ERROR|\.qml:[0-9]+' "$work/$label.err"; then
        echo "FAIL ($label): QML errors above" >&2
        exit 1
    fi
    grep -q state.get "$work/$label.log" || { echo "FAIL ($label): never connected to the fake daemon" >&2; exit 1; }
}

run off
run auto --auto

if grep -q record.start "$work/off.log"; then echo "FAIL: record.start sent with auto_record_on_call off" >&2; exit 1; fi
grep -q record.start "$work/auto.log" || { echo "FAIL: record.start not sent with auto_record_on_call on" >&2; exit 1; }
grep -q -- '-A take=' "$work/notify.log" || { echo "FAIL: no Take notes offer with the setting off" >&2; exit 1; }
echo "smoke ok"
