#!/usr/bin/env bash
# Smoke test: run the plugin against fake-daemon.py inside a real quickshell
# for 5 s, fail on any QML error, and check the call-offer logic both ways.
# Needs a running Wayland session (the overlay is a layer-shell window).
set -euo pipefail
# QML problems show as warnings on stdout or stderr: a binding loop, a TypeError, a ReferenceError, any file.qml:line
qml_bad='ERROR|\.qml:[0-9]+|Binding loop|TypeError|ReferenceError'

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
    timeout 5 quickshell -p "$work/cfg" > "$work/$label.out" 2> "$work/$label.err" || true &
    local qs=$!
    sleep 2.5
    echo '{"active":true}' > "$XDG_RUNTIME_DIR/meeting-mode.json"
    wait "$qs" || true
    kill "$fake" 2>/dev/null || true
    if grep -aE "$qml_bad" "$work/$label.err" "$work/$label.out"; then
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
if grep -aE "$qml_bad" "$work/drop.err" "$work/drop.out"; then echo "FAIL (drop): QML errors above" >&2; exit 1; fi
[ "$(grep -c state.get "$work/drop.log")" -ge 2 ] || { echo "FAIL (drop): the client did not reconnect" >&2; exit 1; }
python3 - "$work/drop.out" <<'PY'
import json, sys
line = next((l for l in open(sys.argv[1], errors="replace") if "DROP {" in l), None)
if line is None:
    sys.exit("FAIL (drop): no transcript dump")
d = json.loads(line[line.index("DROP ") + 5:])
rows = d["rows"]
# The daemon replays nothing: the client asks transcript.get and now.get, so the three spoken lines come back once
# and NOW holds the whole answer (the chunks sent while it was disconnected too), finished by the deltas after the reconnect.
want = ["system|Thanks for joining. Can you walk us through how you would design the ingestion pipeline?", "mic|Sure. I would start with an event queue in front of the workers so producers never block.", "system|Okay. And how would you handle a consumer that falls behind for an hour?"]
if rows != want:
    sys.exit(f"FAIL (drop): the transcript is not the stored lines, once (stale or duplicated rows): {rows}")
if not d["connected"] or d["answering"] or not d["now"].startswith("**Talking points**") or "Kafka docs" not in d["now"] or d["now"].count("Talking points") != 1:
    sys.exit(f"FAIL (drop): NOW is not the whole answer, once, after the reconnect: {d}")
print("drop: reconnected, the transcript and NOW were rebuilt from transcript.get and now.get")
PY

# --locked: settings fixed by Nix are never sent (live_provider is free and still is; a locked provider keeps its model), and the Settings page loads.
ln -sf "$here/tests/harness-locked.qml" "$work/cfg/shell.qml"
export XDG_RUNTIME_DIR="$work/run-locked"
mkdir -m 700 "$XDG_RUNTIME_DIR"
python3 "$here/tests/fake-daemon.py" "$XDG_RUNTIME_DIR/nixarchy-meetingai/daemon.sock" "$work/locked.log" --locked &
fake=$!
sleep 0.5
timeout 6 quickshell -p "$work/cfg" > "$work/locked.out" 2> "$work/locked.err" || true
kill "$fake" 2>/dev/null || true
if grep -aE "$qml_bad" "$work/locked.err" "$work/locked.out"; then echo "FAIL (locked): QML errors above" >&2; exit 1; fi
grep -q "settings.set live_provider" "$work/locked.log" || { echo "FAIL (locked): the free setting was not sent" >&2; exit 1; }
if grep -qE "settings.set (chat_provider|chat_model|meeting_mode|text_scale)" "$work/locked.log"; then echo "FAIL (locked): a setting fixed by Nix was sent" >&2; exit 1; fi
echo "locked: Nix-fixed settings are never sent"

if grep -q record.start "$work/off.log"; then echo "FAIL: record.start sent with auto_record_on_call off" >&2; exit 1; fi
grep -q record.start "$work/auto.log" || { echo "FAIL: record.start not sent with auto_record_on_call on" >&2; exit 1; }
grep -q -- '-A take=' "$work/notify.log" || { echo "FAIL: no Take notes offer with the setting off" >&2; exit 1; }
echo "smoke ok"
