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
printf '#!/bin/sh\necho "$@" >> "%s/notify.log"\n[ -z "$NOTIFY_ANSWER" ] || [ -e "%s/answered" ] || { touch "%s/answered"; sleep "${NOTIFY_DELAY:-0}"; echo "$NOTIFY_ANSWER"; }\n' "$work" "$work" "$work" > "$work/bin/notify-send"
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
    : > "$work/$label.log"; : > "$work/notify.log"; rm -f "$work/answered"   # the stub answers the first notification only
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
s.sendall((json.dumps({"id": 1, "cmd": sys.argv[2], "args": {}, "origin": "shell"}) + "\n").encode())
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

# the call ends while recording without for_call: offer "Stop notes", never stop alone
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

# with for_call, the old Stop notes offer does not show; the daemon's stopping state shows Keep, which sends its gen
stops() { [ "$(grep -c 'record\.stop' "$work/$1.log")" = "$2" ] || fail "$1" "expected $2 record.stop"; }
cs=$(( $(date +%s%3N) - 10000 ))
owned() { case $1 in base) wr "$(st call $$ "" "$cs")" ;; go) wr "$(st none $$ "" "$cs")"; sleep 3 ;; esac; }
NOTIFY_ANSWER=keep run keep "--live --for-call --stopping=call_end:7:30" owned
no_record keep
grep -q -- '-A keep=Keep recording' "$work/keep.notify" || fail keep "no Keep recording offer"
grep -q -- '-t 30000' "$work/keep.notify" || fail keep "the offer does not carry the 30 s timeout"
grep -q 'The call ended\. Notes stop in 30 s\.' "$work/keep.notify" || fail keep "wrong countdown notification body"
grep -q 'record\.keep gen=7' "$work/keep.log" || fail keep "Keep did not send record.keep with the countdown gen"
! grep -q -- '-A stop=' "$work/keep.notify" || fail keep "offered Stop notes when for_call is true"

dead=$(sh -c 'echo $$')   # a pid that has exited
gone() { case $1 in base) wr "$(st call $$ "" "$cs")" ;; go) wr "$(st none "$dead" "" "$cs")"; sleep 3 ;; esac; }
run gone "--live --live-since=2000" gone
no_record gone
grep -q -- '-A stop=Stop notes' "$work/gone.notify" || fail gone "no Stop notes offer when the writer went"

# a call with no callSince owns nothing: older notes get the offer, never a stop
nosince() { case $1 in base) wr "$(st call $$ "" 0)" ;; go) wr "$(st none $$ "" 0)"; sleep 3 ;; esac; }
run nosince "--live --live-since=2000" nosince
no_record nosince
grep -q -- '-A stop=Stop notes' "$work/nosince.notify" || fail nosince "no Stop notes offer for a call without callSince"

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

# A different call while notes are owned (daemon step 1): the fake plays the stop (test.new_call {finish_ms}), saves for finish_ms, then offers the
# handoff token; the shell then starts the new call's notes once, with reason=call. It never starts before the token.
newcall() { python3 -c 'import socket,sys; s=socket.socket(socket.AF_UNIX); s.connect(sys.argv[1]); s.sendall(b"{\"id\":1,\"cmd\":\"test.new_call\",\"args\":{\"finish_ms\":%s},\"origin\":\"shell\"}\n" % sys.argv[2].encode()); s.recv(4096)' "$XDG_RUNTIME_DIR/nixarchy-meetingai/daemon.sock" "$1"; }
starts() { echo $(( $(grep -c 'record\.start' "$work/$1.log") - $(grep -c REFUSED "$work/$1.log") )); }   # accepted starts
once_by_call() { [ "$(starts "$1")" = 1 ] || fail "$1" "expected one accepted record.start, got $(starts "$1")"
    ! grep -q REFUSED "$work/$1.log" || fail "$1" "a start was tried while the old notes were being saved"
    grep -q 'record.start .* reason=call' "$work/$1.log" || fail "$1" "the automatic start does not carry reason=call"; }
cs=$(date +%s%3N)
teams2meet() { case $1 in base) wr "$(st call $$ "" "$cs")" ;; go) newcall 1500; sleep 1; [ "$(grep -c 'record\.start' "$work/handoff.log")" = 0 ] || fail handoff "a start before the old notes were saved"; sleep 4 ;; esac; }
run handoff "--live --auto --for-call" teams2meet
once_by_call handoff

direct() { case $1 in base) wr "$(st call $$ "" "$cs")" ;; go) newcall 1000; wr "$(st call $$ "" "$cs")"; sleep 5 ;; esac; }
run direct "--live --auto --for-call" direct
once_by_call direct

flap() { case $1 in base) wr "$(st call $$ "" "$cs")" ;; go) newcall 1500; sleep 0.3; wr "$(st maybe $$ "" "$cs")"; sleep 0.5; wr "$(st call $$ "" "$cs")"; sleep 4 ;; esac; }
run flap "--live --auto --for-call" flap
once_by_call flap

sameclass() { case $1 in base) wr "$(st call $$ "" "$cs")" ;; go) wr "$(st call $$ "" "$cs")"; sleep 3 ;; esac; }
run sameclass "--live --auto --for-call" sameclass
no_record sameclass

manualstop() { case $1 in base) wr "$(st call $$ "" "$cs")" ;; go) user record.stop; sleep 3 ;; esac; }
run manualstop "--live --auto --for-call" manualstop
[ "$(starts manualstop)" = 0 ] || fail manualstop "a manual stop restarted the notes"

echo "call-state ok"
