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

# preset picker: add -> test connection -> save -> use for chat -> edit (password field, empty key) -> remove (confirm dialog)
cat > "$work/harness-preset.qml" <<'QML'
import QtQuick
import Quickshell

ShellRoot {
    id: root
    property int phase: 0
    property var out: ({})

    function find(it, name, depth) {
        if (!it || !it.visible || (depth || 0) > 60) return null;
        if (it.objectName === name) return it;
        var kids = it.children || [];
        for (var i = 0; i < kids.length; i++) { var f = find(kids[i], name, (depth || 0) + 1); if (f) return f; }
        return null;
    }
    function findAny(it, name, depth) {
        if (!it || (depth || 0) > 60) return null;
        if (it.objectName === name) return it;
        var kids = it.children || [];
        for (var i = 0; i < kids.length; i++) { var f = findAny(kids[i], name, (depth || 0) + 1); if (f) return f; }
        return null;
    }

    function findAll(it, name, out, depth) {
        if (!it || !it.visible || (depth || 0) > 60) return out;
        if (it.objectName === name) out.push(it);
        var kids = it.children || [];
        for (var i = 0; i < kids.length; i++) findAll(kids[i], name, out, (depth || 0) + 1);
        return out;
    }

    Overlay {
        id: overlay
        Component.onCompleted: { overlay.open("{}"); overlay.showLibrary("providers"); }
    }

    Timer {
        interval: 450; repeat: true; running: true
        onTriggered: {
            var card = overlay.geometryRoot();
            root.phase += 1;
            if (root.phase === 2) {
                var addBtn = root.find(card, "addProvider");
                if (addBtn) addBtn.clicked();
            } else if (root.phase === 3) {
                var locBtn = root.find(card, "preset-lm-studio");
                var spBtn = root.find(card, "preset-groq-speech");
                root.out.localLabel = locBtn ? String(locBtn.text) : "";
                root.out.hasSpeechPreset = !!spBtn;
                if (locBtn) locBtn.clicked();
            } else if (root.phase === 4) {
                var kLocal = root.find(card, "providerKey");
                root.out.localKeyVisible = kLocal !== null;
                var backBtn = root.find(card, "formBack");
                if (backBtn) backBtn.clicked();
            } else if (root.phase === 5) {
                var oaBtn = root.find(card, "preset-openai");
                if (oaBtn) oaBtn.clicked();
            } else if (root.phase === 6) {
                var nf = root.find(card, "providerName");
                var mf = root.find(card, "providerModel");
                var ef = root.find(card, "providerEndpoint");
                var kf = root.find(card, "providerKey");
                root.out.addPreFill = nf ? (nf.text + "|" + mf.text + "|" + ef.text) : "";
                root.out.addKeyPassword = kf ? (kf.echoMode === TextInput.Password) : false;
                root.out.addKeyEmpty = kf ? (kf.text === "") : false;
                if (nf) nf.text = "My OpenAI";
                if (kf) kf.text = "sk-smoke-secret-key";
                var tBtn = root.find(card, "testProvider");
                if (tBtn) tBtn.clicked();
            } else if (root.phase === 7) {
                var tm = root.find(card, "testMessage");
                root.out.testMsg = tm ? String(tm.text) : "";
                var sBtn = root.find(card, "saveProvider");
                if (sBtn) sBtn.clicked();
            } else if (root.phase === 8) {
                var cn = root.find(card, "customProviderName");
                var cm = root.find(card, "customProviderMeta");
                root.out.savedName = cn ? String(cn.text) : "";
                root.out.savedMeta = cm ? String(cm.text) : "";
                var uBtn = root.find(card, "useProvider");
                if (uBtn) uBtn.clicked();
            } else if (root.phase === 9) {
                root.out.usedChat = overlay.chatProvider;
                root.out.inPicker = overlay.providers.some(function(p) { return p.id === overlay.chatProvider && p.name === "My OpenAI"; });
                var eBtn = root.find(card, "editProvider");
                if (eBtn) eBtn.clicked();
            } else if (root.phase === 10) {
                var ekf = root.find(card, "providerKey");
                var enf = root.find(card, "providerName");
                root.out.editName = enf ? String(enf.text) : "";
                root.out.editKeyPassword = ekf ? (ekf.echoMode === TextInput.Password) : false;
                root.out.editKeyEmpty = ekf ? (ekf.text === "") : false;
                root.out.editKeyPlaceholder = ekf ? String(ekf.placeholderText) : "";
                if (enf) enf.text = "My OpenAI 2";
                var esv = root.find(card, "saveProvider");
                if (esv) esv.clicked();
            } else if (root.phase === 11) {
                var cn2 = root.find(card, "customProviderName");
                root.out.renamed = cn2 ? String(cn2.text) : "";
                var add2 = root.find(card, "addProvider");
                if (add2) add2.clicked();
            } else if (root.phase === 12) {
                var gb = root.find(card, "preset-groq-speech");
                if (gb) gb.clicked();
            } else if (root.phase === 13) {
                var gn = root.find(card, "providerName");
                var gk = root.find(card, "providerKey");
                if (gn) gn.text = "My Groq";
                if (gk) gk.text = "gk-smoke-secret-key";
                var gs = root.find(card, "saveProvider");
                if (gs) gs.clicked();
            } else if (root.phase === 14) {
                root.out.rows = root.findAll(card, "customProviderName", []).map(function(n) { return String(n.text); });
                var uses = root.findAll(card, "useProvider", []);
                root.out.useTexts = uses.map(function(u) { return String(u.text); });
                if (uses.length > 1) uses[1].clicked();
            } else if (root.phase === 15) {
                root.out.metas = root.findAll(card, "customProviderMeta", []).map(function(n) { return String(n.text); });
                var rms = root.findAll(card, "removeProvider", []);
                if (rms.length) rms[0].clicked();
            } else if (root.phase === 16) {
                var okBtn = root.find(card, "confirmOk");
                root.out.confirmShown = !!okBtn;
                if (okBtn) okBtn.clicked();
            } else if (root.phase === 17) {
                root.out.banner = root.find(card, "noticeDismiss") !== null;
                root.out.afterRemoveRows = root.findAll(card, "customProviderName", []).map(function(n) { return String(n.text); });
                root.out.afterRemoveChat = overlay.chatProvider;
                console.log("PRESET " + JSON.stringify(root.out));
                Qt.quit();
            }
        }
    }
}
QML
ln -sf "$work/harness-preset.qml" "$work/cfg/shell.qml"
export XDG_RUNTIME_DIR="$work/run-preset"
mkdir -m 700 "$XDG_RUNTIME_DIR"
python3 "$here/tests/fake-daemon.py" "$XDG_RUNTIME_DIR/nixarchy-meetingai/daemon.sock" "$work/preset.log" &
fake=$!
sleep 0.5
timeout 20 quickshell -p "$work/cfg" > "$work/preset.out" 2> "$work/preset.err" || true
kill "$fake" 2>/dev/null || true
if grep -aE "$qml_bad" "$work/preset.err" "$work/preset.out"; then echo "FAIL (preset): QML errors above" >&2; exit 1; fi
if grep -q "sk-smoke-secret-key\|gk-smoke-secret-key" "$work/preset.log" "$work/preset.out" "$work/preset.err"; then echo "FAIL (preset): API key leaked to log or output" >&2; exit 1; fi
python3 - "$work/preset.out" "$work/preset.log" <<'PY'
import json, sys
line = next((l for l in open(sys.argv[1], errors="replace") if "PRESET {" in l), None)
if line is None:
    sys.exit("FAIL (preset): no PRESET dump")
d = json.loads(line[line.index("PRESET ") + 7:])
log = open(sys.argv[2]).read().splitlines()
bad = []
if "on this computer" not in d.get("localLabel", ""):
    bad.append(f"local preset not marked 'on this computer': {d.get('localLabel')!r}")
if not d.get("hasSpeechPreset"):
    bad.append("speech preset missing from preset picker")
if d.get("localKeyVisible") is not False:
    bad.append("API key field was visible for local preset")
if d.get("addPreFill") != "OpenAI|gpt-4.1-mini|https://api.openai.com/v1/chat/completions":
    bad.append(f"unexpected preset prefill: {d.get('addPreFill')!r}")
if not d.get("addKeyPassword") or not d.get("addKeyEmpty"):
    bad.append(f"add key field must be password and start empty: {d}")
if d.get("testMsg") != "Connection successful.":
    bad.append(f"test connection did not show 'Connection successful.': {d.get('testMsg')!r}")
if d.get("savedName") != "My OpenAI" or not all(w in d.get("savedMeta", "") for w in ("Chat", "gpt-4.1-mini", "api.openai.com", "key set")):
    bad.append(f"saved custom row mismatch: name={d.get('savedName')!r} meta={d.get('savedMeta')!r}")
if not str(d.get("usedChat", "")).startswith("custom-") or not d.get("inPicker"):
    bad.append(f"Use for chat did not select custom provider: {d}")
if d.get("editName") != "My OpenAI" or not d.get("editKeyPassword") or not d.get("editKeyEmpty") or "leave empty to keep the saved key" not in d.get("editKeyPlaceholder", "").lower():
    bad.append(f"edit form key field check failed: {d}")
if d.get("renamed") != "My OpenAI 2":
    bad.append(f"saved edit did not rename the row: {d.get('renamed')!r}")
if d.get("rows") != ["My OpenAI 2", "My Groq"] or d.get("useTexts") != ["Use for chat", "Use for speech"]:
    bad.append(f"speech provider row missing: {d}")
if len(d.get("metas", [])) != 2 or "in use for chat" not in d["metas"][0] or "in use for speech" not in d["metas"][1] or "Speech" not in d["metas"][1]:
    bad.append(f"in-use marks missing: {d.get('metas')}")
if not d.get("confirmShown") or d.get("rows") is None or d.get("afterRemoveRows") != ["My Groq"] or d.get("afterRemoveChat") != "claude-sub":
    bad.append(f"remove flow failed: {d}")
if not d.get("banner"):
    bad.append("removal notice banner not shown")
for need in ("presets.list", "providers.test", "providers.add", "settings.set chat_provider", "settings.set speech_provider", "providers.update", "providers.remove"):
    if need not in log:
        bad.append(f"daemon log missing {need!r}: {log}")
if bad:
    for b in bad:
        print("FAIL (preset):", b, file=sys.stderr)
    sys.exit(1)
print("preset: add -> test -> save -> use for chat -> edit -> remove ok")
PY

# Nix-fixed chat provider that is a custom one: Use and Remove are off, with the reason
cat > "$work/harness-fixed.qml" <<'QML'
import QtQuick
import Quickshell

ShellRoot {
    id: root
    property int phase: 0
    function find(it, name, depth) {
        if (!it || !it.visible || (depth || 0) > 60) return null;
        if (it.objectName === name) return it;
        var kids = it.children || [];
        for (var i = 0; i < kids.length; i++) { var f = find(kids[i], name, (depth || 0) + 1); if (f) return f; }
        return null;
    }
    Overlay {
        id: overlay
        Component.onCompleted: overlay.open("{}")
    }
    Timer {
        interval: 450; repeat: true; running: true
        onTriggered: {
            root.phase += 1;
            if (root.phase === 3) overlay.showLibrary("providers");   // after the connection: the first load needs it
            if (root.phase < 8) return;
            var card = overlay.geometryRoot();
            var u = root.find(card, "useProvider"), r = root.find(card, "removeProvider"), m = root.find(card, "customProviderMeta");
            console.log("FIXED " + JSON.stringify({ use: u ? u.enabled : null, remove: r ? r.enabled : null, meta: m ? String(m.text) : "" }));
            Qt.quit();
        }
    }
}
QML
ln -sf "$work/harness-fixed.qml" "$work/cfg/shell.qml"
export XDG_RUNTIME_DIR="$work/run-fixed"
mkdir -m 700 "$XDG_RUNTIME_DIR"
python3 "$here/tests/fake-daemon.py" "$XDG_RUNTIME_DIR/nixarchy-meetingai/daemon.sock" "$work/fixed.log" --locked-use &
fake=$!
sleep 0.5
timeout 12 quickshell -p "$work/cfg" > "$work/fixed.out" 2> "$work/fixed.err" || true
kill "$fake" 2>/dev/null || true
if grep -aE "$qml_bad" "$work/fixed.err" "$work/fixed.out"; then echo "FAIL (fixed): QML errors above" >&2; exit 1; fi
grep -a 'FIXED {"use":false,"remove":false,"meta":".*set in Nix' "$work/fixed.out" > /dev/null || { echo "FAIL (fixed): Use/Remove not disabled with a reason: $(grep -a FIXED "$work/fixed.out")" >&2; exit 1; }
echo "fixed: a Nix-fixed custom provider cannot be switched or removed"
echo "smoke ok"
