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
    property var profiles: []   // profiles.list rows; refreshProfiles() reloads them
    readonly property bool recording: state.recording === true
    readonly property real textScale: Math.max(0.8, Math.min(1.6, (Number(settings.text_scale) || 100) / 100))   // overlay font multiplier
    readonly property bool reduceMotion: settings.reduce_motion === true
    property alias transcript: lines
    property bool answering: false
    property var suggestions: []   // at most 3, cleared when a new answer starts
    // The live answer (NOW panel). A finished one moves into the transcript when the next starts.
    property var now: ({ turn: -1, question: "", automatic: false, asker: "", provider: "", model: "", body: "" })
    property string lastSpeaker: ""   // last remote speaker, for "who asked"
    property string lastError: ""
    property real micLevel: 0     // 0..1, from the daemon's levels events
    property real systemLevel: 0
    property var items: []         // items.list rows of the current meeting (team mode)
    property var series: []        // series.list rows
    property var earlierItem: null // { item, quote, line, meeting } while the "Earlier task?" callout shows
    property var candidates: []    // candidates.list rows (interviewer mode)
    property var cand: null        // candidate.get of the current candidate: { candidate, documents, questions }
    property string consentPrompt: ""   // set while the daemon asks for "recording is announced"
    signal notesUpdated(var meeting)
    readonly property string currentMeeting: String(state.meeting || "")
    // While an item's text is being edited a refresh waits, so typing is not lost.
    property bool itemEditing: false
    property var pendingItems: null
    function setItems(list) { if (root.itemEditing) root.pendingItems = list; else root.items = list; }
    onItemEditingChanged: { if (!itemEditing && pendingItems !== null) { root.items = pendingItems; pendingItems = null; } }
    onCurrentMeetingChanged: { root.earlierItem = null; root.pendingItems = null; if (currentMeeting === "") root.items = []; else root.refreshItems(currentMeeting); }
    readonly property string currentCandidate: String(state.candidate || "")
    property string viewId: ""      // a candidate opened in the Library for viewing or deletion, without selecting it
    readonly property string shownCandidate: viewId !== "" ? viewId : currentCandidate
    onCurrentCandidateChanged: { root.viewId = ""; root.refreshCand(); }

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
        root.refreshProfiles();
        root.refreshSeries();
        root.refreshCandidates();
    }

    function refreshSeries() {
        root.send("series.list", {}, function(r) { if (r.ok) root.series = r.data || []; });
    }

    function refreshCandidates() {
        root.send("candidates.list", {}, function(r) { if (r.ok) root.candidates = r.data || []; });
    }

    function refreshCand() {
        // Not shownCandidate: inside onCurrentCandidateChanged that binding can still hold the old value.
        var id = root.viewId !== "" ? root.viewId : root.currentCandidate;
        if (id === "") { root.cand = null; return; }
        root.send("candidate.get", { id: id }, function(r) { if (r.ok) root.cand = r.data; else if (id === root.viewId) { root.viewId = ""; root.refreshCand(); } });
    }

    function viewCandidate(id) { root.viewId = id; root.refreshCand(); }

    // Optimistic: the daemon's questions.updated refetch settles it.
    function updateQuestion(id, fields) {
        if (root.cand) root.cand = Object.assign({}, root.cand, { questions: root.cand.questions.map(function(q) { return q.id === id ? Object.assign({}, q, fields) : q; }) });
        root.send("questions.update", Object.assign({ id: id }, fields));
    }

    // consent_required opens the banner; local_only goes to lastError.
    function startRecording(consent) {
        root.send("record.start", consent ? { consent: true } : {}, function(r) {
            var e = String(r.error || "");
            if (r.ok) root.consentPrompt = "";
            else if (e.indexOf("consent_required: ") === 0) root.consentPrompt = e.slice(18);
            else if (e.indexOf("local_only: ") === 0) { root.consentPrompt = ""; root.lastError = e.slice(12); }
            else root.failed(r);
        });
    }

    function refreshItems(meeting) {
        root.send("items.list", { meeting: meeting }, function(r) { if (r.ok) root.setItems(r.data || []); });
    }

    // Optimistic: the daemon's items.updated refetch settles it.
    function updateItem(id, fields) {
        root.setItems((root.pendingItems || root.items).map(function(i) { return i.id === id ? Object.assign({}, i, fields) : i; }));
        root.send("items.update", Object.assign({ id: id }, fields));
    }

    function refreshProfiles() {
        root.send("profiles.list", {}, function(r) { if (r.ok) root.profiles = r.data || []; });
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

    function archiveNow() {
        if (root.now.body !== "") root.addRow("answer", "", root.now.body, false, root.now.turn);
    }

    function pushLine(ev, partial) {
        if (!partial && ev.who === "system" && ev.speaker) root.lastSpeaker = String(ev.speaker);
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
        case "session.reset": lines.clear(); root.answering = false; root.suggestions = []; root.itemEditing = false; root.pendingItems = null; root.items = []; root.earlierItem = null; root.consentPrompt = ""; root.now = ({ turn: -1, question: "", automatic: false, asker: "", provider: "", model: "", body: "" }); break;
        case "history":
            for (var h = 0; h < (ev.rows || []).length; h++) root.addRow(ev.rows[h].who === "question" ? "asked" : String(ev.rows[h].who), "", String(ev.rows[h].text), false);
            break;
        case "state": root.state = Object.assign({}, root.state, ev); if (ev.recording === false) { root.micLevel = 0; root.systemLevel = 0; } break;
        case "levels": root.micLevel = Number(ev.mic) || 0; root.systemLevel = Number(ev.system) || 0; break;
        case "transcript.partial": root.pushLine(ev, true); break;
        case "transcript.final": root.pushLine(ev, false); break;
        case "answer.started":
            root.archiveNow();
            if (ev.automatic !== true && String(ev.question || "") !== "") root.addRow("asked", "", String(ev.question), false);
            root.suggestions = [];
            root.answering = true;
            root.now = { turn: ev.turn, question: String(ev.question || ""), automatic: ev.automatic === true, asker: ev.automatic === true ? (root.lastSpeaker || "Them") : "You", provider: String(ev.provider || ""), model: String(ev.model || ""), body: "" };
            break;
        case "answer.delta":
            root.answering = true;
            if (root.now.turn !== ev.turn) { root.archiveNow(); root.now = { turn: ev.turn, question: "", automatic: true, asker: root.lastSpeaker || "Them", provider: "", model: "", body: "" }; }
            root.now = Object.assign({}, root.now, { body: root.now.body + (ev.text || ""), provider: String(ev.provider || root.now.provider), model: String(ev.model || root.now.model) });
            break;
        case "answer.done": root.answering = false; if (ev.provider) root.now = Object.assign({}, root.now, { provider: String(ev.provider), model: String(ev.model || root.now.model) }); break;
        case "answer.error": root.answering = false; root.addRow("error", "", String(ev.message || ev.error || "answer failed"), false); break;
        case "suggestion": root.suggestions = root.suggestions.concat([String(ev.text)]).slice(0, 3); break;
        case "notice": root.addRow("notice", "", String(ev.text || ""), false); break;
        case "items.updated": if (ev.meeting === root.state.meeting) root.refreshItems(ev.meeting); break;
        case "earlier_item": if (ev.meeting === root.state.meeting) root.earlierItem = { item: ev.item || ({}), quote: String(ev.quote || ""), line: ev.line, meeting: ev.meeting, new_item: ev.new_item }; break;
        case "questions.updated": if (ev.candidate === root.shownCandidate) root.refreshCand(); break;
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
