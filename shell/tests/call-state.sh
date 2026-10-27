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
printf '#!/bin/sh\necho "$@" >> "%s/notify.log"\n' "$work" > "$work/bin/notify-send"
chmod +x "$work/bin/notify-send"
. "$(dirname "$0")/headless.sh"
export PATH="$work/bin:$PATH"

# st <call> [pid] [updated-ms]: a state file as meeting-mode writes it; `active` is true throughout (a manual meeting mode)
st() { printf '{"active":true,"reason":"manual","since":1,"updated":%s,"pid":%s,"call":"%s","callApp":"Zoom","callTitle":"Secret roadmap review","callSince":%s,"callDetected":%s}' \
    "${3:-$(date +%s%3N)}" "${2:-$$}" "$1" "$(date +%s%3N)" "$([ "$1" = call ] && echo true || echo false)"; }
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
grep -q 'record.start title=Secret roadmap review' "$work/auto.log" || fail auto "no record.start with the call title on none -> call"
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

# a file whose writer is dead, or an old one with no pid, reads as none
dead() { case $1 in base) wr "$(st none)" ;; go) wr "$(st call 999999999)"; sleep 3; wr "$(st call 0 1000)"; sleep 3 ;; esac; }
run dead --auto dead
no_record dead; no_offer dead

echo "call-state ok"
