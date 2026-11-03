#!/usr/bin/env bash
# Call-state test: the service reacts to meeting-mode's `call`, never to `active`. Each case runs the service
# (tests/harness-call.qml) against fake-daemon.py in a private headless sway, writes fake
# $XDG_RUNTIME_DIR/meeting-mode.json files, and checks the fake daemon's log and a notify-send stub.
set -euo pipefail
qml_bad='ERROR|\.qml:[0-9]+|Binding loop|TypeError|ReferenceError'
here=$(cd "$(dirname "$0")/.." && pwd)
tree=${OMARCHY_SHELL_TREE:-/nix/store/gmshfnfmc8wrz4f85f49b3xx1j8a6v0l-nixarchy-omarchy-tree/shell}
work=$(mktemp -d /tmp/nm.XXXX)   # short: unix socket paths cap at 108 bytes
trap 'kill $(jobs -p) 2>/dev/null || true; chmod -R u+w "$work"; rm -rf "$work"' EXIT
mkdir "$work/cfg"
cp -rs "$tree"/* "$work/cfg/"
rm -f "$work/cfg/shell.qml"
for f in "$here"/*.qml; do ln -s "$f" "$work/cfg/$(basename "$f")"; done
ln -s "$here/tests/harness-call.qml" "$work/cfg/shell.qml"
mkdir "$work/bin"
printf '#!/bin/sh\necho "$@" >> "%s/notify.log"\n[ -z "$NOTIFY_ANSWER" ] || { sleep "${NOTIFY_DELAY:-0}"; echo "$NOTIFY_ANSWER"; }\n' "$work" > "$work/bin/notify-send"
chmod +x "$work/bin/notify-send"
. "$(dirname "$0")/headless.sh"
export PATH="$work/bin:$PATH"

# st <call> [pid] [updated-ms] [callSince]: a state file as meeting-mode writes it; `active` is true throughout (a manual meeting mode)
st() { printf '{"active":true,"reason":"manual","since":1,"updated":%s,"pid":%s,"call":"%s","callApp":"Zoom","callTitle":"Secret roadmap review","callSince":%s,"callDetected":%s}' \
    "${3:-$(date +%s%3N)}" "${2:-$$}" "$1" "${4:-$(date +%s%3N)}" "$([ "$1" = call ] && echo true || echo false)"; }
wr() { printf '%s' "$1" > "$XDG_RUNTIME_DIR/meeting-mode.json"; }   # in place, so the watcher sees it

# run <label> <fake-daemon flags> <steps function>: the baseline file is written before the service starts
run() {
    local label=$1 flags=$2 steps=$3
    export XDG_RUNTIME_DIR="$work/run-$label"
    mkdir -m 700 "$XDG_RUNTIME_DIR"
    : > "$work/$label.log"; : > "$work/notify.log"
    python3 "$here/tests/fake-daemon.py" "$XDG_RUNTIME_DIR/nixarchy-meetingai/daemon.sock" "$work/$label.log" $flags &
    local fake=$!
    sleep 0.5
    $steps base
    timeout 12 quickshell -p "$work/cfg" > "$work/$label.out" 2> "$work/$label.err" || true &
    local qs=$!
    sleep 3
    $steps go
    wait "$qs" || true
    kill "$fake" 2>/dev/null || true
    cp "$work/notify.log" "$work/$label.notify"
    if grep -aE "$qml_bad" "$work/$label.err" "$work/$label.out"; then echo "FAIL ($label): QML errors above" >&2; exit 1; fi
    grep -q state.get "$work/$label.log" || { echo "FAIL ($label): never connected to the fake daemon" >&2; exit 1; }
}
fail() { echo "FAIL ($1): $2" >&2; exit 1; }
no_record() { ! grep -q 'record\.\(start\|stop\)' "$work/$1.log" || fail "$1" "the daemon was asked to record"; }
# user <cmd>: plays the user, a second client of the fake daemon; reads past the state broadcast to the reply (id 1)
user() { python3 - "$XDG_RUNTIME_DIR/nixarchy-meetingai/daemon.sock" "$1" <<'EOF'
import json, socket, sys
s = socket.socket(socket.AF_UNIX); s.settimeout(3); s.connect(sys.argv[1])
s.sendall((json.dumps({"id": 1, "cmd": sys.argv[2], "args": {}}) + "\n").encode())
for line in s.makefile():
    r = json.loads(line)
    if r.get("id") == 1:
        sys.exit(0 if r.get("ok") else 1)
sys.exit(1)
EOF
}
no_offer() { ! grep -q -- '-A ' "$work/$1.notify" || fail "$1" "an offer was shown"; }

manual() { case $1 in base) wr "$(st none)" ;; go) wr "$(st none)"; sleep 1; printf '{"active":true,"reason":"manual","since":2,"updated":1,"pid":%s}' "$$" > "$XDG_RUNTIME_DIR/meeting-mode.json"; sleep 4 ;; esac; }
run manual --auto manual
no_record manual; no_offer manual

started() { case $1 in base) wr "$(st none)" ;; go) wr "$(st maybe)"; sleep 1.5; wr "$(st call)"; sleep 4 ;; esac; }
run offer "" started
no_record offer
grep -q -- '-A take=Take notes' "$work/offer.notify" || fail offer "no Take notes offer on none -> call"
grep -q "Zoom" "$work/offer.notify" || fail offer "the offer does not name the app"
grep -q "Secret roadmap" "$work/offer.notify" && fail offer "a window title reached a notification"

run auto --auto started
grep -q 'record.start title=Secret roadmap review new' "$work/auto.log" || fail auto "no record.start with the call title on none -> call"
no_offer auto

maybe_only() { case $1 in base) wr "$(st none)" ;; go) wr "$(st maybe)"; sleep 4 ;; esac; }
run maybe --auto maybe_only
no_record maybe; no_offer maybe

# interviewer mode never auto-records (consent stays manual): the offer is shown instead
run interviewer "--interviewer --auto" started
! grep -q 'record\.start' "$work/interviewer.log" || fail interviewer "auto-recorded in interviewer mode"
grep -q -- '-A take=Take notes' "$work/interviewer.notify" || fail interviewer "no offer in interviewer mode"

# the call ends while recording: offer "Stop notes", never stop alone
hangup() { case $1 in base) wr "$(st call)" ;; go) wr "$(st none)"; sleep 4 ;; esac; }
run hangup "--live --auto" hangup
no_record hangup
grep -q -- '-A stop=Stop notes' "$work/hangup.notify" || fail hangup "no Stop notes offer on call -> none"

# the same call ending through "maybe" still offers "Stop notes"
viamaybe() { case $1 in base) wr "$(st call "$$" "" "$cs")" ;; go) wr "$(st maybe "$$" "" "$cs")"; sleep 1.5; wr "$(st none)"; sleep 3 ;; esac; }
cs=$(date +%s%3N)
run viamaybe "--live --auto" viamaybe
no_record viamaybe
grep -q -- '-A stop=Stop notes' "$work/viamaybe.notify" || fail viamaybe "no Stop notes offer on call -> maybe -> none"

# the call ends while the notes it started run: they stop by themselves after the grace period (2 s here), unless kept
# every write passes the same callSince (a missing one reads as a different call); the recording started 2 s before the daemon, after the call
stops() { [ "$(grep -c 'record\.stop' "$work/$1.log")" = "$2" ] || fail "$1" "expected $2 record.stop"; }
cs=$(( $(date +%s%3N) - 10000 ))
autostop() { case $1 in base) wr "$(st call $$ "" "$cs")" ;; go) wr "$(st none $$ "" "$cs")"; sleep 3 ;; esac; }
run autostop "--live --live-since=2000" autostop
stops autostop 1
grep -q -- '-A keep=Keep recording' "$work/autostop.notify" || fail autostop "no Keep recording offer"
grep -q -- '-t 2000' "$work/autostop.notify" || fail autostop "the offer does not expire with the grace period"

maybeend() { case $1 in base) wr "$(st call $$ "" "$cs")" ;; go) wr "$(st maybe $$ "" "$cs")"; sleep 3 ;; esac; }
run maybeend "--live --live-since=2000" maybeend
stops maybeend 1

# a call that returns during the grace period takes the notes over: the first timer must not fire, the second end stops them
cs=$(( $(date +%s%3N) - 60000 ))
takeover() { case $1 in base) wr "$(st call $$ "" "$cs")" ;; go)
    wr "$(st none $$ "" "$cs")"; sleep 0.5
    t2=$(date +%s%3N); wr "$(st call $$ "" "$t2")"; sleep 2.5
    wr "$(st none $$ "" "$t2")"; echo MARK >> "$work/takeover.log"; sleep 3 ;; esac; }
run takeover "--live --live-since=30000" takeover
stops takeover 1
[ "$(grep -n 'record\.stop' "$work/takeover.log" | cut -d: -f1)" -gt "$(grep -n MARK "$work/takeover.log" | cut -d: -f1)" ] || fail takeover "the notes stopped before the second end"

cs=$(( $(date +%s%3N) - 10000 ))
NOTIFY_ANSWER=keep run keep "--live --live-since=2000" autostop
no_record keep
! grep -q -- '-A stop=' "$work/keep.notify" || fail keep "offered Stop notes after Keep recording"

manualnew() { case $1 in base) wr "$(st call $$ "" "$cs")" ;; go) wr "$(st none $$ "" "$cs")"; sleep 0.5; user record.stop; user record.start; sleep 3 ;; esac; }
run manualnew "--live --live-since=2000" manualnew
stops manualnew 1

dead=$(sh -c 'echo $$')   # a pid that has exited
gone() { case $1 in base) wr "$(st call $$ "" "$cs")" ;; go) wr "$(st none "$dead" "" "$cs")"; sleep 3 ;; esac; }
run gone "--live --live-since=2000" gone
no_record gone
grep -q -- '-A stop=Stop notes' "$work/gone.notify" || fail gone "no Stop notes offer when the writer went"

gonepending() { case $1 in base) wr "$(st call $$ "" "$cs")" ;; go) wr "$(st maybe $$ "" "$cs")"; sleep 0.5; wr "$(st maybe "$dead" "" "$cs")"; sleep 3 ;; esac; }
run gonepending "--live --live-since=2000" gonepending
no_record gonepending
[ "$(grep -c -- '-A stop=Stop notes' "$work/gonepending.notify")" = 1 ] || fail gonepending "expected exactly one Stop notes offer"

# an old Stop notes button answered after the user started a new meeting must not stop that one
cs=$(date +%s%3N)
stalestop() { case $1 in base) wr "$(st call $$ "" "$cs")" ;; go) wr "$(st none $$ "" "$cs")"; sleep 0.5; user record.stop; user record.start; sleep 3 ;; esac; }
NOTIFY_ANSWER=stop NOTIFY_DELAY=1.5 run stalestop "--live --auto" stalestop
stops stalestop 1

# settings arrive after the call: auto-record decides once they are there, never an offer
slowset() { case $1 in base) wr "$(st none)" ;; go) wr "$(st call)"; sleep 5 ;; esac; }
run slow "--auto --slow-settings" slowset
grep -q 'record.start title=Secret roadmap review new' "$work/slow.log" || fail slow "no auto-record once the settings arrived"
no_offer slow

# an IPC offer arriving right after an atomic write the watcher may not have seen still offers
race() { case $1 in base) wr "$(st none)" ;; go)
    printf '%s' "$(st call)" > "$XDG_RUNTIME_DIR/mm.tmp"; mv "$XDG_RUNTIME_DIR/mm.tmp" "$XDG_RUNTIME_DIR/meeting-mode.json"
    quickshell ipc -p "$work/cfg" call meetingai offer > /dev/null 2>&1 || true
    sleep 1; cp "$work/notify.log" "$work/race.snap"; sleep 3 ;; esac; }
run race "" race
grep -q -- '-A take=Take notes' "$work/race.snap" || fail race "the IPC offer did not see the new call before the 5 s poll"

# a file whose writer is dead, or an old one with no pid, reads as none
dead() { case $1 in base) wr "$(st none)" ;; go) wr "$(st call 999999999)"; sleep 3; wr "$(st call 0 1000)"; sleep 3 ;; esac; }
run dead --auto dead
no_record dead; no_offer dead

echo "call-state ok"
