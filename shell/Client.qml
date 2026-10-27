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
    property var lastAsk: null           // the last typed question, for Retry
    property int staleTurn: -1           // the turn that was streaming when a new question was asked: its late events are dropped
    property string providerNotice: ""   // the daemon's one-time provider notice (e.g. the removed Gemini key); Providers shows it, Service notifies
    property bool libraryOpen: false     // set by the overlay: request failures then go to the Library, not the transcript
    signal libraryError(string message)
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

    // consent_required opens the banner; local_only becomes an error row in the transcript.
    function startRecording(consent) {
        root.send("record.start", consent ? { consent: true } : {}, function(r) {
            var e = String(r.error || "");
            if (r.ok) root.consentPrompt = "";
            else if (e.indexOf("consent_required: ") === 0) root.consentPrompt = e.slice(18);
            else if (e.indexOf("local_only: ") === 0) { root.consentPrompt = ""; root.addRow("error", "", e.slice(12), false); }
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

    // Fixed by programs.nixarchy-meetingai.settings.enforce: shown disabled, never sent.
    function isLocked(key) { return (root.settings.locked || []).indexOf(key) >= 0; }
    function lockedLabel(key, label) { return root.isLocked(key) ? label + " (Set in Nix)" : label; }

    function setSetting(key, value) {
        if (root.isLocked(key)) return;
        var next = Object.assign({}, root.settings);
        next[key] = value;
        root.settings = next;
        root.send("settings.set", { key: key, value: value });
    }

    function ask(text, attachments, screenshot, noContext) {
        root.lastAsk = { text: text, attachments: attachments || [], screenshot: screenshot === true, noContext: noContext === true };
        root.archiveNow();
        root.staleTurn = root.now.turn;
        root.answering = true;
        root.suggestions = [];
        // NOW shows the question and the waiting bar at once; answer.started fills in the rest
        root.now = { turn: -1, question: text, automatic: false, asker: "You", provider: "", model: "", body: "", error: "" };
        root.send("chat.send", { text: text, attachments: root.lastAsk.attachments, screenshot: root.lastAsk.screenshot, no_context: root.lastAsk.noContext }, function(r) { if (!r.ok) root.answerFailed(String(r.error || "request failed")); });
    }
    function retry() { if (root.lastAsk) root.ask(root.lastAsk.text, root.lastAsk.attachments, root.lastAsk.screenshot, root.lastAsk.noContext); }
    // A failed answer stays in NOW, with Retry, instead of becoming a line in the transcript.
    function answerFailed(message) {
        root.answering = false;
        root.now = Object.assign({}, root.now, { error: message !== "" ? message : "The answer failed." });
    }

    // Everything the daemon's current session owns; also what a lost connection leaves behind.
    function resetSession() {
        lines.clear();
        root.staleTurn = -1;
        root.answering = false; root.suggestions = []; root.itemEditing = false; root.pendingItems = null; root.items = []; root.earlierItem = null; root.consentPrompt = "";
        root.micLevel = 0; root.systemLevel = 0;
        root.now = ({ turn: -1, question: "", automatic: false, asker: "", provider: "", model: "", body: "", error: "" });
    }

    // The daemon replays nothing to a client that connects (or reconnects): ask it for the stored
    // transcript and the current answer. now.turn is -2 (unknown) while an answer is still streaming:
    // the next delta says which turn it is. Pages of 500 lines; the cap keeps the newest.
    function rebuild() {
        var page = function(after, first) {
            root.send("transcript.get", { after_line: after, limit: root.maxLines }, function(r) {
                var rows = r.ok && r.data ? r.data.lines : undefined;
                if (!Array.isArray(rows)) return;
                if (first) lines.clear();
                for (var i = 0; i < rows.length; i++) {
                    if (rows[i].who === "system" && rows[i].speaker) root.lastSpeaker = String(rows[i].speaker);
                    root.addRow(String(rows[i].who), String(rows[i].speaker || ""), String(rows[i].text), false);
                }
                if (rows.length >= root.maxLines) page(rows[rows.length - 1].line, false);
            });
        };
        page(0, true);
        root.send("now.get", {}, function(r) {
            if (!r.ok || !r.data || r.data.status === undefined) return;
            var n = r.data, live = n.status === "writing" || n.status === "loading";
            root.answering = live;
            root.suggestions = (n.suggestions || []).slice(0, 3).map(String);
            if (n.answer === "" && n.question === "") return;
            root.now = { turn: live ? -2 : -1, question: String(n.question || ""), automatic: false, asker: "You", provider: String(n.provider || ""), model: String(n.model || ""), body: String(n.answer || ""), error: "" };
        });
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
        // the tail of a turn that was superseded by a new question must not touch the new one
        if (ev.turn !== undefined && ev.turn === root.staleTurn && (ev.event === "answer.delta" || ev.event === "answer.done" || ev.event === "answer.error")) return;
        switch (ev.event) {
        case "settings": root.settings = ev.settings || ({}); break;
        case "session.reset": root.resetSession(); break;
        case "history":
            lines.clear();   // a reconnect replays the history: it replaces the transcript, it does not add to it
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
            if (root.now.turn === -2) root.now = Object.assign({}, root.now, { turn: ev.turn });
            if (root.now.turn !== ev.turn) { root.archiveNow(); root.now = { turn: ev.turn, question: "", automatic: true, asker: root.lastSpeaker || "Them", provider: "", model: "", body: "" }; }
            root.now = Object.assign({}, root.now, { body: root.now.body + (ev.text || ""), provider: String(ev.provider || root.now.provider), model: String(ev.model || root.now.model) });
            break;
        case "answer.done": root.answering = false; if (ev.provider) root.now = Object.assign({}, root.now, { provider: String(ev.provider), model: String(ev.model || root.now.model) }); break;
        case "answer.error": if (ev.turn === undefined || ev.turn === root.now.turn || root.now.turn === -2) root.answerFailed(String(ev.message || ev.error || "")); break;
        case "suggestion": root.suggestions = root.suggestions.concat([String(ev.text)]).slice(0, 3); break;
        case "notice": if (ev.kind === "providers") root.providerNotice = String(ev.text || ""); else root.addRow("notice", "", String(ev.text || ""), false); break;
        case "items.updated": if (ev.meeting === root.state.meeting) root.refreshItems(ev.meeting); break;
        case "earlier_item": if (ev.meeting === root.state.meeting) root.earlierItem = { item: ev.item || ({}), quote: String(ev.quote || ""), line: ev.line, meeting: ev.meeting, new_item: ev.new_item }; break;
        case "questions.updated": if (ev.candidate === root.shownCandidate) root.refreshCand(); break;
        case "notes.updated": root.notesUpdated(ev.meeting); break;
        case "error": root.addRow("error", "", String(ev.message || ""), false); break;
        }
    }

    function failed(msg) {
        root.answering = false;
        if (root.libraryOpen) root.libraryError(String(msg.error || "request failed"));
        else root.addRow("error", "", String(msg.error || "request failed"), false);
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
                root.rebuild();
            } else {
                var p = root.pending;
                root.pending = ({});
                root.state = ({});
                root.resetSession();
                // after the reset, so an answer that was in flight leaves its error (and Retry) in NOW
                for (var id in p) if (typeof p[id] === "function") p[id]({ ok: false, error: "disconnected" });
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
