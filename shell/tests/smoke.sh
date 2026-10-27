#!/usr/bin/env bash
# Smoke test: run the plugin against fake-daemon.py inside a real quickshell
# for 5 s, fail on any QML error, and check the call-offer logic both ways.
# Needs a running Wayland session (the overlay is a layer-shell window).
set -euo pipefail

here=$(cd "$(dirname "$0")/.." && pwd)
tree=${OMARCHY_SHELL_TREE:-/nix/store/gmshfnfmc8wrz4f85f49b3xx1j8a6v0l-nixarchy-omarchy-tree/shell}
work=$(mktemp -d /tmp/nm.XXXX)   # short: unix socket paths cap at 108 bytes
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
. "$(dirname "$0")/headless.sh"
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

# --drop: the daemon cuts the connection mid-answer. No QML error, a second state.get (the client
# reconnected), and every transcript row once (history replaces the transcript, it does not add to it).
ln -sf "$here/tests/harness-drop.qml" "$work/cfg/shell.qml"
export XDG_RUNTIME_DIR="$work/run-drop"
mkdir -m 700 "$XDG_RUNTIME_DIR"
python3 "$here/tests/fake-daemon.py" "$XDG_RUNTIME_DIR/nixarchy-meetingai/daemon.sock" "$work/drop.log" --drop &
fake=$!
sleep 0.5
timeout 14 quickshell -p "$work/cfg" > "$work/drop.out" 2> "$work/drop.err" || true
kill "$fake" 2>/dev/null || true
if grep -E 'ERROR|\.qml:[0-9]+' "$work/drop.err" "$work/drop.out"; then echo "FAIL (drop): QML errors above" >&2; exit 1; fi
[ "$(grep -c state.get "$work/drop.log")" -ge 2 ] || { echo "FAIL (drop): the client did not reconnect" >&2; exit 1; }
python3 - "$work/drop.out" <<'PY'
import json, sys
line = next((l for l in open(sys.argv[1], errors="replace") if "DROP {" in l), None)
if line is None:
    sys.exit("FAIL (drop): no transcript dump")
d = json.loads(line[line.index("DROP ") + 5:])
rows = d["rows"]
want = ["asked|And how would you handle a consumer that falls behind for an hour?"]   # the history the daemon replays
if rows != want:
    sys.exit(f"FAIL (drop): the transcript is not the replayed history (stale or duplicated rows): {rows}")
if not d["connected"] or d["answering"] or "Scale out consumers" not in d["now"]:
    sys.exit(f"FAIL (drop): the answer did not finish after the reconnect: {d}")
print(f"drop: reconnected, the transcript is exactly the replayed history")
PY

if grep -q record.start "$work/off.log"; then echo "FAIL: record.start sent with auto_record_on_call off" >&2; exit 1; fi
grep -q record.start "$work/auto.log" || { echo "FAIL: record.start not sent with auto_record_on_call on" >&2; exit 1; }
grep -q -- '-A take=' "$work/notify.log" || { echo "FAIL: no Take notes offer with the setting off" >&2; exit 1; }
echo "smoke ok"
