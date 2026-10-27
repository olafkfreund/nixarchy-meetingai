#!/usr/bin/env bash
# Frames of a live meeting against fake-daemon.py --live: partial words, a
# question from the other side, a streamed answer, then suggestions.
# usage: [SCENARIO=live|k8s|team|interviewer|r5|first-run|no-daemon|error|providers|call] [SCALE=100] [MEETINGAI_GEOMETRY=1] live-shots.sh [OUT_DIR]   (needs a Wayland session and grim)
# SCENARIO=k8s plays the captured real answer (fake-daemon.py --k8s); SCALE is the text_scale setting.
# MEETINGAI_GEOMETRY=1 also runs tests/check-geometry.py on what the harness dumped. SCENARIO=providers switches through every
# provider and asserts the model dropdowns (fake-daemon.py --providers). QML warnings (stdout or stderr) fail the run.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
out=${1:-/tmp/meetingai-live}
scenario=${SCENARIO:-live}; [ "$scenario" = error ] && export MEETINGAI_ASK=1; scale=${SCALE:-100}; size=${SIZE:-}   # SCENARIO=r5 SIZE=700x520: resized card, Profiles and Prompts tabs
tree=${OMARCHY_SHELL_TREE:-/nix/store/gmshfnfmc8wrz4f85f49b3xx1j8a6v0l-nixarchy-omarchy-tree/shell}
work=$(mktemp -d /tmp/nm.XXXX)   # short: unix socket paths cap at 108 bytes
trap 'kill $(jobs -p) 2>/dev/null || true; chmod -R u+w "$work"; rm -rf "$work"' EXIT
mkdir -p "$out" "$work/cfg"
# THEME=flexoki-light (any name under the omarchy themes dir): run with that theme instead of the current one
if [ -n "${THEME:-}" ]; then
    mkdir -p "$work/home/.local/state/omarchy/current"
    ln -s "$(dirname "$tree")/themes/$THEME" "$work/home/.local/state/omarchy/current/theme"
fi
cp -rs "$tree"/* "$work/cfg/"
rm -f "$work/cfg/shell.qml"
for f in "$here"/*.qml; do ln -s "$f" "$work/cfg/$(basename "$f")"; done
ln -s "$here/tests/geometry.js" "$work/cfg/geometry.js"
ln -s "$here/tests/harness-$(case $scenario in r5) echo r5 ;; providers) echo providers ;; *) echo live ;; esac).qml" "$work/cfg/shell.qml"
real_run=$XDG_RUNTIME_DIR
. "$(dirname "$0")/headless.sh"
export XDG_RUNTIME_DIR="$work/run"
mkdir -m 700 "$XDG_RUNTIME_DIR"
if [ "$scenario" != no-daemon ]; then   # no-daemon: nothing listens, to see the Setup card
    python3 "$here/tests/fake-daemon.py" "$XDG_RUNTIME_DIR/nixarchy-meetingai/daemon.sock" "$work/fake.log" $([ "$scenario" = r5 ] || echo --$scenario) --scale=$scale ${size:+--size=$size} &
fi
sleep 0.5
env ${THEME:+HOME="$work/home"} timeout 30 quickshell -p "$work/cfg" 2> "$work/qs.err" | tee "$work/qs.out" | grep -av "GEO {" &   # quickshell logs (console.log too) to stdout
qs=$!
# Capture only the overlay card (it is centred on its full-screen layer on the
# focused monitor): full-desktop frames would include whatever else is open.
geom() {
    local sig layer
    if [ "$HEADLESS" = 1 ]; then layer="0 0 1920 1080"; echo "$layer" > "$work/layer"; else
    sig=$(ls -t "$real_run"/hypr | head -1)
    layer=$(XDG_RUNTIME_DIR="$real_run" HYPRLAND_INSTANCE_SIGNATURE=$sig hyprctl -j layers | python3 -c '
import json, sys
for mon in json.load(sys.stdin).values():
    for lv in mon["levels"].values():
        for l in lv:
            if l["namespace"] == "nixarchy-meetingai":
                print(l["x"], l["y"], l["w"], l["h"])')
    echo "$layer" > "$work/layer"
    fi
    python3 -c '
import sys
x, y, w, h = map(float, sys.argv[1:5])
cw, ch = min(w - 80, 900), min(h - 80, 760)   # Overlay.qml default card size
if len(sys.argv) > 5 and "x" in sys.argv[5]:
    cw, ch = map(float, sys.argv[5].split("x"))
print("%d,%d %dx%d" % (x + (w - cw) / 2, y + (h - ch) / 2, cw, ch))' $layer $size
}
sleep 3.4; g=$(geom)
# name:seconds to wait before the crop (the harness opens Settings at 16 s)
case $scenario in
    r5) frames="1-resized:1 2-profiles:4.5 3-prompts:4" ;;
    first-run) frames="1-setup:0.5" ;;
    no-daemon) frames="1-no-daemon:0.5" ;;
    error) frames="1-waiting:11.2 2-failed:4" ;;
    team) frames="1-items:3 2-last-time:6 3-settings:4" ;;
    interviewer) frames="1-consent:0.1 2-bank:5.5 3-candidate:9.5 4-new:2 5-attach:2 6-delete:2" ;;   # banner at ~3.5 s, bank after consent at 6 s, Candidate tab after 16 s
    providers) frames="1-opencode-error:5.1 2-providers-tab:5.5" ;;   # opencode's list fails at ~8 s; the Providers tab opens at 13 s
    k8s) frames="1-answering:2 2-ready:9 3-settings:4 4-mode:2" ;;
    call) frames="1-chip:0.3 2-maybe:0.6 3-recording:0.6" ;;   # a Zoom call (outlined dot + chip), then a maybe (dim dot), then recording (filled dot + timer)
    *) frames="1-question:1 2-answering:2.4 3-suggestions:3 4-you-speaking:2.2 5-asked:2.4 6-settings:4 7-mode:2 8-bar:0.3" ;;
esac
for f in $frames; do
    sleep "${f#*:}"
    region=$g
    case ${f%:*} in *-bar) read -r lx ly _ < "$work/layer"; region="$lx,$ly 160x36" ;; esac   # the bar widget pill, top left of the monitor
    case ${f%:*} in 1-chip|2-maybe|3-recording) read -r lx ly _ < "$work/layer"; region="$lx,$ly 320x36" ;; esac
    if [ "$scenario" = call ]; then   # the next state for the bar: maybe, then the recording (the fake daemon answers record.start)
        case ${f%:*} in
            2-maybe) python3 -c 'import sys; p = sys.argv[1]; s = open(p).read(); open(p, "w").write(s.replace("\"call\": \"call\"", "\"call\": \"maybe\""))' "$XDG_RUNTIME_DIR/meeting-mode.json" ;;   # in place: the watcher sees it
            3-recording) python3 -c 'import socket,json,os,sys; s=socket.socket(socket.AF_UNIX); s.connect(sys.argv[1]); s.sendall(b"{\"id\":1,\"cmd\":\"record.start\",\"args\":{}}\n"); s.recv(4096)' "$XDG_RUNTIME_DIR/nixarchy-meetingai/daemon.sock" ;;
        esac
        sleep 0.5
    fi
    grim -g "$region" "$out/crop-${f%:*}.png"
done
wait "$qs" || true
# QML problems: an ERROR, a binding loop, a TypeError, a ReferenceError, any file.qml:line (our own GEO/PROV/KEY dumps are not problems)
if grep -aE 'ERROR|\.qml:[0-9]+|Binding loop|TypeError|ReferenceError' "$work/qs.err" "$work/qs.out" | grep -avE 'GEO \{|PROV |KEY |DROP '; then echo "FAIL: QML problems above" >&2; exit 1; fi
if [ "$scenario" = providers ]; then
    python3 - "$work/qs.out" <<'PY'
import json, sys
dumps = [json.loads(l[l.index("PROV ") + 5:]) for l in open(sys.argv[1], errors="replace") if "PROV {" in l]
models = {"claude-sub": ["sonnet", "opus", "haiku"], "agy-sub": ["gemini-3.8-flash-low", "gemini-3.8-pro"], "opencode": [], "codex-sub": ["default", "gpt-5.2-codex", "gpt-5.2"],
          "ollama-cloud": ["gpt-oss:120b", "qwen3-coder:480b"], "ollama": ["qwen3.8:27b", "llama4:8b"]}   # fake-daemon.py MODELS
want = ["ollama", "ollama", "agy-sub", "opencode", "codex-sub", "ollama-cloud", "ollama"]   # claude-sub's step is quiet: its slow reply must not show
bad = []
if [d["at"] for d in dumps] != want:
    bad.append(f"expected dumps for {want}, got {[d['at'] for d in dumps]}")
for d in dumps:
    p, m = d["at"], models[d["at"]]
    for who, key, mk, ek in (("chat", "chat", "models", "error"), ("live", "live", "liveModels", "liveError")):
        if d[key] != p:
            bad.append(f"{p}: the {who} provider is {d[key]}")
        if d[mk] != m:
            bad.append(f"{p}: the {who} dropdown holds {d[mk]}, not {m} (a stale reply, or the list was not loaded)")
        if p == "opencode":
            if "opencode auth login" not in d[ek] or (who == "chat" and "opencode auth login" not in d["fix"]):
                bad.append(f"{p}: a failed list must show the error and the fix: {d[ek]!r} {d['fix']!r}")
        elif d[ek]:
            bad.append(f"{p}: unexpected error {d[ek]!r}")
    for mk, kk in (("models", "model"), ("liveModels", "liveModel")):
        first = models[p][0] if models[p] else ""
        if d[kk] != first:
            bad.append(f"{p}: the model is {d[kk]!r}, not the first entry {first!r}")
for m in bad:
    print("FAIL", m)
print(f"providers: {len(dumps)} switches checked, {len(bad)} problems")
sys.exit(1 if bad else 0)
PY
fi
# MEETINGAI_GEOMETRY=1: the harness dumped every control and text; fail on misalignment, clipping, cut text, overflow
if [ "${MEETINGAI_GEOMETRY:-}" = 1 ]; then
    grep -a "GEO {" "$work/qs.out" > "$out/geometry.log" || true
    python3 "$here/tests/check-geometry.py" "$out/geometry.log"
fi
ls "$out"
