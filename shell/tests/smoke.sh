#!/usr/bin/env bash
# Smoke test: run the plugin against fake-daemon.py inside a real quickshell
# for 9 s, fail on any QML error, and check the call-offer logic both ways.
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
ln -s "$here/tests/geometry.js" "$work/cfg/geometry.js"
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
    timeout 9 quickshell -p "$work/cfg" > "$work/$label.out" 2> "$work/$label.err" || true &
    local qs=$!
    sleep 2.5
    # a manual meeting mode (active, call none) is no call; a call is: the service reacts to `call`
    echo "{\"active\":true,\"call\":\"call\",\"callApp\":\"Zoom\",\"callSince\":1,\"pid\":$$,\"updated\":$(date +%s%3N)}" > "$XDG_RUNTIME_DIR/meeting-mode.json"
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

# --stale: the late tail of an earlier turn (delta, error, done) must not touch the turn that started after it.
ln -sf "$here/tests/harness-stale.qml" "$work/cfg/shell.qml"
export XDG_RUNTIME_DIR="$work/run-stale"
mkdir -m 700 "$XDG_RUNTIME_DIR"
python3 "$here/tests/fake-daemon.py" "$XDG_RUNTIME_DIR/nixarchy-meetingai/daemon.sock" "$work/stale.log" --stale &
fake=$!
sleep 0.5
timeout 7 quickshell -p "$work/cfg" > "$work/stale.out" 2> "$work/stale.err" || true
kill "$fake" 2>/dev/null || true
if grep -aE "$qml_bad" "$work/stale.err" "$work/stale.out"; then echo "FAIL (stale): QML errors above" >&2; exit 1; fi
python3 - "$work/stale.out" <<'PY'
import json, sys
line = next((l for l in open(sys.argv[1], errors="replace") if "STALE {" in l), None)
if line is None:
    sys.exit("FAIL (stale): no dump")
d = json.loads(line[line.index("STALE ") + 6:])
if d["now"] != "NEW" or d["error"] != "":
    sys.exit(f"FAIL (stale): a late reply of an earlier turn reached NOW: {d}")
print("stale: replies of earlier turns are dropped")
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

# --voice: room rows from transcript.get, transcript.relabel and transcript.final, plus voice.* controls in Settings.
ln -sf "$here/tests/harness-voice.qml" "$work/cfg/shell.qml"
export XDG_RUNTIME_DIR="$work/run-voice"
mkdir -m 700 "$XDG_RUNTIME_DIR"
python3 "$here/tests/fake-daemon.py" "$XDG_RUNTIME_DIR/nixarchy-meetingai/daemon.sock" "$work/voice.log" --voice &
fake=$!
sleep 0.5
timeout 7 quickshell -p "$work/cfg" > "$work/voice.out" 2> "$work/voice.err" || true
kill "$fake" 2>/dev/null || true
if grep -aE "$qml_bad" "$work/voice.err" "$work/voice.out" | grep -avE 'GEO \{|VOICE[1-5] '; then echo "FAIL (voice): QML errors above" >&2; exit 1; fi
python3 "$here/tests/check-geometry.py" "$work/voice.out"
python3 - "$work/voice.out" "$work/voice.log" <<'PY'
import json, sys
out = open(sys.argv[1], errors="replace").read().splitlines()
cmds = open(sys.argv[2], errors="replace").read().splitlines()
v = {k: json.loads(next(l[l.index(k + " ") + len(k) + 1:] for l in out if (k + " {") in l)) for k in ("VOICE1", "VOICE2", "VOICE3", "VOICE4", "VOICE5")}
geos = {g["tag"]: g["items"] for g in (json.loads(l[l.index("GEO {") + 4:]) for l in out if "GEO {" in l)}
want_rows = [
    "room|Earlier room remark",
    "mic|My own first line",
    "room|Someone else in the room",
    "room|Direct room line",
]
if v["VOICE1"]["rows"] != want_rows:
    sys.exit(f"FAIL (voice): unexpected transcript rows: {v['VOICE1']['rows']}")
t_items = geos.get("transcript", [])
t_labels = [i["label"] for i in t_items]
if t_labels.count("Room") != 3 or "You · Spoke" not in t_labels:
    sys.exit(f"FAIL (voice): Room / You · Spoke labels missing in transcript: {t_labels}")
room_xs = [i["x"] for i in t_items if i["label"] == "Room"]
you_xs = [i["x"] for i in t_items if i["label"] == "You · Spoke"]
if not room_xs or not you_xs or max(room_xs) >= min(you_xs):
    sys.exit(f"FAIL (voice): Room rows must be left-aligned like Them, not right-aligned like You: room={room_xs} you={you_xs}")
before = {i["n"]: i["label"] for i in geos.get("before-enroll", []) if i.get("n")}
if before.get("voiceStatus") != "Owner voice: not enrolled" or "The quick brown fox" not in before.get("voiceSentence", "") or before.get("voiceRecord") != "Record my voice" or before.get("voicePause") != "Pause check":
    sys.exit(f"FAIL (voice): initial Settings voice controls wrong: {before}")
if not v["VOICE2"]["voiceEnrolling"]:
    sys.exit(f"FAIL (voice): voiceEnrolling not set during voice.enroll: {v['VOICE2']}")
enrolled = {i["n"]: i["label"] for i in geos.get("enrolled", []) if i.get("n")}
if not v["VOICE3"]["voice"]["enrolled"] or v["VOICE3"]["voiceEnrolling"] or enrolled.get("voiceStatus") != "Owner voice: enrolled" or enrolled.get("voiceRecordAgain") != "Record again" or enrolled.get("voiceDelete") != "Delete":
    sys.exit(f"FAIL (voice): enrolled state wrong: {v['VOICE3']} {enrolled}")
paused = {i["n"]: i["label"] for i in geos.get("paused", []) if i.get("n")}
if not v["VOICE4"]["voice"]["paused"] or paused.get("voiceStatus") != "Owner voice: enrolled (check paused)":
    sys.exit(f"FAIL (voice): paused state wrong: {v['VOICE4']} {paused}")
if v["VOICE5"]["voice"]["enrolled"]:
    sys.exit(f"FAIL (voice): still enrolled after voice.delete: {v['VOICE5']}")
for need in ("voice.status", "voice.enroll", "settings.set voice_check_paused", "voice.delete"):
    if need not in cmds:
        sys.exit(f"FAIL (voice): daemon never received {need!r}: {cmds}")
print("voice: room rows (history, relabel, final) and voice controls verified")
PY

if grep -q record.start "$work/off.log"; then echo "FAIL: record.start sent with auto_record_on_call off" >&2; exit 1; fi
grep -q record.start "$work/auto.log" || { echo "FAIL: record.start not sent with auto_record_on_call on" >&2; exit 1; }
grep -q -- '-A take=' "$work/notify.log" || { echo "FAIL: no Take notes offer with the setting off" >&2; exit 1; }
echo "smoke ok"
