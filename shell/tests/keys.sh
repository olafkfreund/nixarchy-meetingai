#!/usr/bin/env bash
# Keyboard test: the composer has the focus when the overlay opens, then Tab (stepped through
# the focus chain, as the key does) walks the header and the Candidate tab. Fails if a stop does
# not move the focus, or if a button the keyboard must reach is never visited.
# Needs a running Wayland session (the overlay is a layer-shell window).
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
tree=${OMARCHY_SHELL_TREE:-/nix/store/gmshfnfmc8wrz4f85f49b3xx1j8a6v0l-nixarchy-omarchy-tree/shell}
work=$(mktemp -d /tmp/nm.XXXX)   # short: unix socket paths cap at 108 bytes
trap 'kill $(jobs -p) 2>/dev/null || true; chmod -R u+w "$work"; rm -rf "$work"' EXIT
mkdir "$work/cfg"
cp -rs "$tree"/* "$work/cfg/"
rm -f "$work/cfg/shell.qml"
for f in "$here"/*.qml; do ln -s "$f" "$work/cfg/$(basename "$f")"; done
ln -s "$here/tests/harness-keys.qml" "$work/cfg/shell.qml"
. "$(dirname "$0")/headless.sh"
export XDG_RUNTIME_DIR="$work/run"
mkdir -m 700 "$XDG_RUNTIME_DIR"
python3 "$here/tests/fake-daemon.py" "$XDG_RUNTIME_DIR/nixarchy-meetingai/daemon.sock" "$work/fake.log" --interviewer &
sleep 0.5
timeout 25 quickshell -p "$work/cfg" > "$work/qs.out" 2> "$work/qs.err" || true
python3 - "$work/qs.out" <<'PY'
import re, sys
keys = [l.split("KEY ", 1)[1].strip() for l in open(sys.argv[1], errors="replace") if "KEY " in l]
bad = []
opened = next((k for k in keys if k.startswith("open ")), "")
if '"composerInput"' not in opened:
    bad.append(f"the composer does not have the focus on open: {opened or 'no probe'}")
stops = [k.split(" ", 1)[1] for k in keys if re.match(r"\d+ ", k)]
if len(stops) < 30:
    bad.append(f"only {len(stops)} Tab stops logged")
for a, b in zip(stops, stops[1:]):
    if a == b:
        bad.append(f"Tab did not move the focus from {a}")
        break
names = set(re.findall(r'"([^"]+)"\)', " ".join(stops)))
for need in ["record", "modePicker", "settings", "tab-candidate", "candidateNew", "attachCv", "attachJob", "generate", "questionText", "questionCompetency", "addQuestion", "deleteCandidate"]:
    if need not in names:
        bad.append(f"Tab never reached '{need}'")
print(f"keys: {len(stops)} stops, {len(names)} named items reached")
for m in bad:
    print("FAIL", m)
sys.exit(1 if bad else 0)
PY
echo "keys ok"
