pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import Quickshell.Io

// Connection to the nixarchy-meetingai daemon: newline-delimited JSON over a
// unix socket. Each QML entry point (service, overlay, bar widget) makes its
// own Client; the daemon accepts many clients and broadcasts events to all.
Item {
    id: root

    readonly property string socketPath: Quickshell.env("XDG_RUNTIME_DIR") + "/nixarchy-meetingai/daemon.sock"
    readonly property bool connected: sock.connected
    property var state: ({})
    property var settings: ({})
    readonly property bool recording: state.recording === true
    readonly property bool reduceMotion: settings.reduce_motion === true
    property alias transcript: lines
    property bool answering: false
    property var suggestions: []
    property string lastError: ""
    property real micLevel: 0     // 0..1, from the daemon's levels events
    property real systemLevel: 0
    signal notesUpdated(var meeting)

    readonly property int maxLines: 500
    property int seq: 0
    property var pending: ({})
    property int delay: 500

    ListModel { id: lines }

    // The reply goes to cb({ok, data, error}); cb may be omitted.
    function send(cmd, args, cb) {
        if (!sock.connected) {
            var down = { ok: false, error: "daemon not running" };
            if (cb) cb(down); else root.failed(down);
            return;
        }
        root.seq += 1;
        root.pending[root.seq] = cb || null;
        sock.write(JSON.stringify({ id: root.seq, cmd: cmd, args: args || {} }) + "\n");
        sock.flush();
    }

    function refresh() {
        root.send("state.get", {}, function(r) { if (r.ok) root.state = r.data || {}; });
        root.send("settings.get", {}, function(r) { if (r.ok) root.settings = r.data || {}; });
    }

    function setSetting(key, value) {
        var next = Object.assign({}, root.settings);
        next[key] = value;
        root.settings = next;
        root.send("settings.set", { key: key, value: value });
    }

    function ask(text, attachments, screenshot, noContext) {
        root.answering = true;
        root.suggestions = [];
        root.send("chat.send", { text: text, attachments: attachments || [], screenshot: screenshot === true, no_context: noContext === true });
    }

    function pushLine(ev, partial) {
        for (var i = lines.count - 1; i >= 0 && i >= lines.count - 4; i--) {
            if (lines.get(i).partial && lines.get(i).who === ev.who) {
                lines.remove(i);
                break;
            }
        }
        root.addRow(String(ev.who || ""), String(ev.speaker || ""), String(ev.text || ""), partial);
    }

    // Answers and errors share the transcript model: one virtualized list.
    function addRow(who, speaker, body, partial, turn) {
        lines.append({ who: who, speaker: speaker, body: body, partial: partial, turn: turn === undefined ? -1 : turn });
        while (lines.count > root.maxLines) lines.remove(0);
    }

    function onEvent(ev) {
        switch (ev.event) {
        case "settings": root.settings = ev.settings || ({}); break;
        case "session.reset": lines.clear(); root.answering = false; root.suggestions = []; break;
        case "history":
            for (var h = 0; h < (ev.rows || []).length; h++) root.addRow(String(ev.rows[h].who), "", String(ev.rows[h].text), false);
            break;
        case "state": root.state = Object.assign({}, root.state, ev); if (ev.recording === false) { root.micLevel = 0; root.systemLevel = 0; } break;
        case "levels": root.micLevel = Number(ev.mic) || 0; root.systemLevel = Number(ev.system) || 0; break;
        case "transcript.partial": root.pushLine(ev, true); break;
        case "transcript.final": root.pushLine(ev, false); break;
        case "answer.delta":
            root.answering = true;
            var at = -1;   // transcript lines may interleave with the stream
            for (var i = lines.count - 1; i >= 0 && at < 0; i--) if (lines.get(i).who === "answer" && lines.get(i).turn === ev.turn) at = i;
            if (at < 0) { root.addRow("answer", "", "", false, ev.turn); at = lines.count - 1; }
            lines.setProperty(at, "body", lines.get(at).body + (ev.text || ""));
            break;
        case "answer.done": root.answering = false; break;
        case "answer.error": root.answering = false; root.addRow("error", "", String(ev.message || ev.error || "answer failed"), false); break;
        case "suggestion": root.suggestions = root.suggestions.concat([String(ev.text)]); break;
        case "notes.updated": root.notesUpdated(ev.meeting); break;
        case "error": root.lastError = String(ev.message || ""); break;
        }
    }

    function failed(msg) {
        root.answering = false;
        root.addRow("error", "", String(msg.error || "request failed"), false);
    }

    function onLine(data) {
        var msg;
        try { msg = JSON.parse(data); } catch (e) { return; }
        if (msg.event !== undefined) { root.onEvent(msg); return; }
        if (!(msg.id in root.pending)) return;
        var cb = root.pending[msg.id];
        delete root.pending[msg.id];
        // A caller with a callback shows its own error; otherwise surface it here.
        if (cb) cb(msg); else if (msg.ok === false) root.failed(msg);
    }

    Socket {
        id: sock
        path: root.socketPath
        parser: SplitParser {
            splitMarker: "\n"
            onRead: function(data) { root.onLine(data); }
        }
        onConnectionStateChanged: {
            if (connected) {
                root.delay = 500;
                retry.stop();
                root.refresh();
            } else {
                var p = root.pending;
                root.pending = ({});
                for (var id in p) p[id]({ ok: false, error: "disconnected" });
                root.state = ({});
                retry.interval = root.delay;
                root.delay = Math.min(5000, root.delay * 2);
                retry.restart();
            }
        }
    }

    Timer {
        id: retry
        onTriggered: {
            sock.connected = false;
            sock.connected = true;
        }
    }

    Component.onCompleted: sock.connected = true
}
