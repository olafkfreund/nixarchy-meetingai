#!/usr/bin/env bash
# Smoke test against the REAL daemon: starts target/debug/nixarchy-meetingai on
# a throwaway runtime/data/config dir, runs the quickshell harness against its
# socket while real-driver.py checks every reply/event shape. Needs Wayland.
set -euo pipefail

here=$(cd "$(dirname "$0")/.." && pwd)
root=$(cd "$here/.." && pwd)
tree=${OMARCHY_SHELL_TREE:-/nix/store/gmshfnfmc8wrz4f85f49b3xx1j8a6v0l-nixarchy-omarchy-tree/shell}
bin=$root/target/debug/nixarchy-meetingai
(cd "$root" && devenv shell -- cargo build)

work=$(mktemp -d /tmp/nm.XXXX)   # short: unix socket paths cap at 108 bytes
trap 'kill $(jobs -p) 2>/dev/null || true; chmod -R u+w "$work"; rm -rf "$work"' EXIT

mkdir "$work/cfg"
cp -rs "$tree"/* "$work/cfg/"
rm -f "$work/cfg/shell.qml"
for f in "$here"/*.qml; do ln -s "$f" "$work/cfg/$(basename "$f")"; done
ln -s "$here/tests/harness.qml" "$work/cfg/shell.qml"
mkdir "$work/bin"
printf '#!/bin/sh\n:\n' > "$work/bin/notify-send"
chmod +x "$work/bin/notify-send"

export WAYLAND_DISPLAY=${XDG_RUNTIME_DIR:?}/${WAYLAND_DISPLAY:-wayland-1}
export PATH="$work/bin:$PATH"
export XDG_RUNTIME_DIR="$work/run" XDG_DATA_HOME="$work/data" XDG_CONFIG_HOME="$work/conf"
mkdir -m 700 "$XDG_RUNTIME_DIR"
sock=$XDG_RUNTIME_DIR/nixarchy-meetingai/daemon.sock

"$bin" > "$work/daemon.log" 2>&1 &
daemon=$!
for _ in $(seq 50); do [ -S "$sock" ] && break; sleep 0.1; done
[ -S "$sock" ] || { cat "$work/daemon.log"; echo "FAIL: daemon never bound $sock" >&2; exit 1; }

up=0; curl -s -m 2 localhost:11434/api/tags > /dev/null && up=1

quickshell -p "$work/cfg" > "$work/qs.err" 2>&1 &
qs=$!
sleep 2
rc=0
python3 -I "$here/tests/real-driver.py" "$sock" "$up" ${NM_MODEL:+"$NM_MODEL"} || rc=$?
sleep 1
kill "$qs" 2>/dev/null || true; wait "$qs" 2>/dev/null || true

"$bin" ctl quit > /dev/null || true
for _ in $(seq 30); do kill -0 "$daemon" 2>/dev/null || break; sleep 0.1; done
kill -0 "$daemon" 2>/dev/null && { echo "FAIL: daemon did not exit on quit" >&2; exit 1; }

if grep -E 'ERROR|\.qml:[0-9]+' "$work/qs.err"; then echo "FAIL: QML errors above" >&2; exit 1; fi
[ "$rc" = 0 ] || { echo "FAIL: protocol checks (see BAD lines)" >&2; exit 1; }
echo "smoke-real ok"
