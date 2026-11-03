pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects (Style.spacing, Color.popups) the linter cannot type
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// Restore / Profiles / Prompts / Providers / Settings. One search field asks
// the daemon (settings.search) and also filters the lists on the other tabs.
Item {
    id: root

    required property var client
    required property var theme
    property string tab: "restore"
    property string query: ""
    property var sessions: []
    readonly property var profiles: client.profiles
    property string selName: ""   // the profile being edited
    property var providers: []
    readonly property var labelOverrides: ({ "mcp_allow_record": "Let AI agents start recording" })   // the daemon derives labels from keys
    property var found: []        // settings.search results: { key, label, value }
    property string promptMode: "general"
    property string notes: ""
    property bool notesStale: false   // notes.get: names changed after the notes were written
    property string selectedId: ""
    property string notesPhase: ""
    property string menuFor: ""
    property var lineHits: []         // transcript.search rows for the query
    property string linesDoneFor: ""
    signal closeRequested()
    readonly property real notesDefault: Style.space(160) * theme.ts
    property real notesHeight: notesDefault
    // Unix ms for sorting and showing: a chat's last activity, a meeting's start (SQLite UTC text). 0 if unknown.
    function when(s) {
        if (s.kind === "chat") return Number(s.updated_at) || 0;
        return s.started_at ? (Date.parse(String(s.started_at).replace(" ", "T") + "Z") || 0) : 0;
    }
    readonly property real notesMin: Style.space(48) * theme.ts
    TextMetrics { id: captionLine; font.family: Style.font.family; font.pixelSize: root.theme.caption; text: "42 min · 412 lines" }
    readonly property real rowH: theme.ctl + captionLine.height + Style.spacing.xs   // a two-line meeting row as drawn
    // The notes pane keeps three lines at least and leaves the list two rows; on a card too short for both, the list wins.
    // A binding, so the default on open, a window resize and a late export status line are clamped too. Never reads notesBox.height.
    readonly property real notesMax: restoreList.parent.height - restoreList.y - 2 * root.rowH
        - notesGrip.height - (exportStatusText.visible ? exportStatusText.height + Style.spacing.md : 0) - 2 * Style.spacing.md
    function setNotesHeight(h) { root.notesHeight = Math.max(0, Math.min(Math.max(h, root.notesMin), root.notesMax)); }
    property var notesMeeting: null
    property string exportStatus: ""
    property var exportAfterPick: null
    property string modelsText: ""
    property string modelsFor: ""   // the provider the Models button asked about; a reply for another one is stale
    property var status: ({})        // providers.status rows by id: ready / not installed / not logged in / no key, with the login command
    readonly property var stateLabels: ({ ready: "ready", not_installed: "not installed", not_logged_in: "not logged in", no_key: "no key" })
    property var presets: []        // presets.list rows
    property string provMode: ""    // "" | "presets" | "form"
    property string editId: ""      // custom provider id being edited, or "" when adding from a preset
    property string formPreset: ""
    // The key field shows unless the endpoint, as typed, is a localhost http URL.
    readonly property bool formNeedsKey: !/^http:\/\/(localhost|127\.0\.0\.1|\[::1\])([:\/]|$)/i.test(provEndpoint.text.trim())
    property int formEpoch: 0       // bumped when the form opens or closes; a test reply from an older one is ignored
    property bool testingProvider: false
    property string testMessage: ""
    property Component settingsHeader: null   // controls shown above the Settings list

    readonly property bool iv: client.settings.meeting_mode === "interviewer"
    readonly property var tabs: root.iv ? ["restore", "profiles", "candidate", "prompts", "providers", "settings"] : ["restore", "profiles", "prompts", "providers", "settings"]
    readonly property var promptModes: [{ value: "general", name: "Assist" }, { value: "interview", name: "Interview me" }, { value: "team_meeting", name: "Team" }, { value: "interviewer", name: "Interview someone" }]
    readonly property var promptKinds: [{ value: "answer", name: "Answer" }, { value: "live_answer", name: "Live answer" }, { value: "follow_up", name: "Follow-ups" }, { value: "recap", name: "Recap" }]
    readonly property var kindHelp: ({
        answer: "Used for questions you type: how the model answers you.",
        live_answer: "Used for automatic answers during a call: one sentence to say, then short talking points.",
        follow_up: "Used to suggest the next questions to ask (Ask next).",
        recap: "Used to write the meeting notes when a recording stops."
    })
    readonly property var cur: {
        var m = root.profiles.filter(function(p) { return p.name === root.selName; });
        if (m.length) return m[0];
        m = root.profiles.filter(function(p) { return p.selected; });
        return m.length ? m[0] : (root.profiles.length ? root.profiles[0] : null);
    }
    onCurChanged: { aboutText.text = cur ? String(cur.personal_context || "") : ""; briefText.text = cur ? String(cur.meeting_context || "") : ""; nameField.text = cur ? cur.name : ""; }
    function reload() { root.client.refreshProfiles(); }
    function newProfile() {
        var names = root.profiles.map(function(p) { return p.name; }), n = "New profile", i = 1;
        while (names.indexOf(n) >= 0) n = "New profile " + (++i);
        root.selName = n;
        root.client.send("profiles.save", { name: n, personal_context: "", meeting_context: "" }, function() { root.client.send("profiles.select", { name: n }, root.reload); });
    }
    function saveProfile() {
        if (!cur) return;
        var a = aboutText.text, b = briefText.text;
        root.client.send("profiles.save", { name: cur.name, personal_context: a, meeting_context: b }, root.reload);
        if (cur.selected) { root.client.setSetting("personal_context", a); root.client.setSetting("meeting_context", b); }
    }
    property string promptKind: "answer"
    signal confirmRequested(string message, string label, var yes)
    function candidateHook(what) { candTab.hook(what); }
    function exportMeeting(id) {
        root.notesMeeting = id;
        var folder = String(root.client.settings.export_folder || "").trim();
        if (folder === "") {
            if (root.client.isLocked("export_folder")) {
                root.exportStatus = "Choose an export folder first.";
                return;
            }
            root.exportAfterPick = id;
            exportFolderDialog.open();
            return;
        }
        root.client.send("meetings.export", { meeting: id }, function(r) {
            if (r.ok && r.data && r.data.path) root.exportStatus = "Exported to " + String(r.data.path);
            else root.exportStatus = String(r.error || "Export failed.");
        });
    }
    function setExportFolder(path) {
        var m = root.exportAfterPick;
        root.exportAfterPick = null;
        root.client.setSetting("export_folder", path);
        root.search();
        if (m !== null) root.exportMeeting(m);
    }

    function esc(t) { return String(t).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;"); }
    // StyledText: everything is escaped first, only the match gets a tag
    function marked(text, q) {
        var t = String(text), i = t.toLowerCase().indexOf(q.toLowerCase());
        return i < 0 ? root.esc(t) : root.esc(t.slice(0, i)) + "<b>" + root.esc(t.slice(i, i + q.length)) + "</b>" + root.esc(t.slice(i + q.length));
    }
    function select(s) {
        var id = String(s.id);
        root.selectedId = id;
        root.menuFor = "";
        root.notes = "";
        root.notesStale = false;
        if (s.kind !== "meeting") { root.notesPhase = ""; return; }
        root.notesMeeting = s.id;   // an export.done for the meeting being read shows its status line
        root.notesPhase = "loading";
        root.fetchNotes(s.id);
    }
    // The reply lands only while that meeting is still selected; a live refresh keeps the old text until it does.
    function fetchNotes(meeting) {
        var id = String(meeting);
        root.client.send("notes.get", { meeting: meeting }, function(r) {
            if (root.selectedId !== id) return;
            root.notes = r.ok ? String((r.data && r.data.text !== undefined) ? r.data.text : r.data) : String(r.error || "request failed");
            root.notesStale = r.ok && !!r.data && r.data.stale === true;
            root.notesPhase = !r.ok ? "error" : (root.notes === "" ? "empty" : "ready");
        });
    }
    function capHeadings(md) {
        var lines = String(md).split("\n"), out = [], fence = "";   // the open fence's marker: only the same marker closes it
        for (var i = 0; i < lines.length; i++) {
            var line = lines[i], heading = fence ? null : line.match(/^ {0,3}#{1,6}[ \t]+(.*?)(?:[ \t]+#+)?[ \t]*$/);
            var mark = line.match(/^ {0,3}(```|~~~)/);
            if (mark && (!fence || mark[1] === fence)) { fence = fence ? "" : mark[1]; out.push(line); }
            else if (heading && heading[1].replace(/\*\*/g, "").trim() !== "") out.push("", "**" + heading[1].replace(/\*\*/g, "") + "**", "");
            else out.push(line);
        }
        return out.join("\n");
    }
    function searchLines() {
        var q = root.query.trim();
        if (q.length < 2) { root.lineHits = []; root.linesDoneFor = ""; return; }
        root.linesDoneFor = "";
        root.client.send("transcript.search", { query: q }, function(r) {
            if (root.query.trim() !== q) return;
            root.lineHits = r.ok ? (r.data || []) : [];
            if (r.ok) root.linesDoneFor = q;   // a failed search is not "No results"
        });
    }
    function openHit(h) {
        if (String(h.meeting) === root.client.currentMeeting) { root.closeRequested(); root.client.openLine(String(h.id)); return; }
        if (root.client.recording) return;
        root.client.pendingJump = { meeting: String(h.meeting), id: String(h.id) };
        root.client.send("session.restore", { id: h.meeting }, function(r) {
            if (r.ok) root.closeRequested();
            else { root.client.pendingJump = null; root.error = String(r.error || "request failed"); }
        });
    }

    function match(s) { return root.query === "" || String(s).toLowerCase().indexOf(root.query.toLowerCase()) >= 0; }

    function endpointHost(url) {
        var m = String(url || "").match(/^https?:\/\/([^\/]+)/);
        return m ? m[1] : String(url || "");
    }
    function openPresetList() {
        root.error = "";
        root.provMode = "presets";
        root.client.send("presets.list", {}, function(r) { if (r.ok) root.presets = r.data || []; });
    }
    function openPresetForm(p) {
        root.error = "";
        root.editId = "";
        root.formPreset = String(p.preset || "");
        provName.text = String(p.name || "");
        provEndpoint.text = String(p.endpoint || "");
        provModel.text = String(p.model || "");
        provKey.text = "";
        root.testMessage = "";
        root.testingProvider = false;
        root.provMode = "form";
    }
    function openEditProvider(p) {
        root.error = "";
        root.editId = String(p.id || "");
        root.formPreset = "";
        provName.text = String(p.name || "");
        provEndpoint.text = String(p.endpoint || "");
        provModel.text = String(p.model || "");
        provKey.text = "";
        root.testMessage = "";
        root.testingProvider = false;
        root.provMode = "form";
    }
    function testProvider() {
        root.error = "";
        root.testMessage = "";
        root.testingProvider = true;
        var epoch = root.formEpoch;
        var args = { name: provName.text.trim(), model: provModel.text.trim(), endpoint: provEndpoint.text.trim() };
        if (root.editId !== "") args.id = root.editId;
        else args.preset = root.formPreset;
        if (root.formNeedsKey && provKey.text !== "") args.api_key = provKey.text;
        root.client.send("providers.test", args, function(r) {
            if (epoch !== root.formEpoch) return;
            root.testingProvider = false;
            if (!r.ok) root.testMessage = "Error: " + String(r.error || "request failed");
            else if (r.data && r.data.message !== undefined) root.testMessage = String(r.data.message);
            else root.testMessage = r.data && r.data.ok ? "Connected." : "Connection failed.";
        });
    }
    function saveProvider() {
        root.error = "";
        var args = {
            name: provName.text.trim(),
            model: provModel.text.trim(),
            endpoint: provEndpoint.text.trim()
        };
        if (root.editId !== "") {
            args.id = root.editId;
            if (root.formNeedsKey && provKey.text !== "") args.api_key = provKey.text;
            root.client.send("providers.update", args, function(r) {
                if (!r.ok) { root.error = String(r.error || "request failed"); return; }
                provKey.text = "";
                root.provMode = "";
                root.load();
            });
        } else {
            args.preset = root.formPreset;
            if (root.formNeedsKey && provKey.text !== "") args.api_key = provKey.text;
            root.client.send("providers.add", args, function(r) {
                if (!r.ok) { root.error = String(r.error || "request failed"); return; }
                provKey.text = "";
                root.provMode = "";
                root.load();
            });
        }
    }

    function load() {
        root.client.send("session.list", {}, function(r) { if (r.ok) root.sessions = r.data || []; });
        root.reload();
        root.client.send("providers.list", {}, function(r) { if (r.ok) root.providers = r.data || []; });
        if (root.tab === "providers") {
            root.client.send("providers.status", {}, function(r) { if (r.ok) { var m = {}; (r.data || []).forEach(function(row) { m[row.id] = row; }); root.status = m; } });
            root.client.send("presets.list", {}, function(r) { if (r.ok) root.presets = r.data || []; });
        }
        if (root.tab === "settings") root.client.refreshVoice();
        root.search();
        root.loadPrompt();
        root.client.refreshCandidates();
    }
    function search() {
        root.client.send("settings.search", { query: root.query }, function(r) { if (r.ok) root.found = (r.data || []).filter(function(f) { return String(f.key).indexOf("overlay_") !== 0 && f.key !== "voice_check_paused"; }); });
    }

    function loadPrompt() {
        root.client.send("prompts.get", { mode: root.promptMode, kind: root.promptKind }, function(r) {
            promptText.text = r.ok ? String((r.data && r.data.text !== undefined) ? r.data.text : r.data) : "";
        });
    }

    property string error: ""   // the last failed request made from here, shown under the search field
    onProvModeChanged: { root.formEpoch++; if (root.provMode !== "form") { provKey.text = ""; root.testMessage = ""; root.testingProvider = false; } }
    onTabChanged: { root.error = ""; root.provMode = ""; root.load(); }
    onVisibleChanged: { if (!visible) root.provMode = ""; else root.notesHeight = root.notesDefault; }   // closing drops a typed key; opening resets the notes height
    onPromptModeChanged: root.loadPrompt()
    onPromptKindChanged: root.loadPrompt()
    Component.onCompleted: root.load()

    Timer { id: linesTimer; interval: 300; onTriggered: root.searchLines() }

    Connections {
        target: root.client
        function onLibraryError(message) { root.error = message; }
        function onNotesUpdated(meeting) {
            root.load();
            if (String(meeting) === root.selectedId) root.fetchNotes(meeting);   // in place: no "Loading", the menu stays
        }
        function onExportDone(meeting, path) { if (root.notesMeeting !== null && String(root.notesMeeting) === String(meeting) && path !== "") root.exportStatus = "Exported to " + path; }
        function onProvidersChanged() { root.load(); }
    }

    Column {
        anchors.fill: parent
        spacing: Style.spacing.md

        Flow {   // wraps rather than running past the card edge on a narrow card
            width: parent.width
            spacing: Style.spacing.xs
            Repeater {
                model: root.tabs
                IconButton { ts: root.theme.ts;
                    required property string modelData
                    objectName: "tab-" + modelData
                    text: modelData.charAt(0).toUpperCase() + modelData.slice(1)
                    active: root.tab === modelData
                    onClicked: root.tab = modelData
                }
            }
        }

        PanelSeparator {}

        Input {
            objectName: "search"
            ts: root.theme.ts
            width: parent.width
            placeholderText: root.tab === "restore" ? "Search meetings, chats and transcripts" : "Search settings, chats, profiles"
            onTextChanged: { root.query = text; root.search(); root.linesDoneFor = ""; linesTimer.restart(); }
        }
        UiText {
            objectName: "noResults"
            visible: root.tab === "restore" && root.query.trim().length >= 2 && restoreList.count === 0 && root.linesDoneFor === root.query.trim() && root.lineHits.length === 0
            width: parent.width
            textFormat: Text.PlainText
            text: "No results"
            color: Commons.Color.popups.text
            opacity: root.theme.dim
            font.pixelSize: root.theme.caption
        }
        UiText {   // a failed request from the shown tab
            visible: root.error !== ""
            width: parent.width
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            text: "Error: " + root.error
            color: Commons.Color.popups.text
            font.pixelSize: root.theme.caption
        }

        // Restore: chats and meetings with their notes.
        ListView {
            id: restoreList
            objectName: "restoreList"
            visible: root.tab === "restore"
            width: parent.width; height: Math.max(0, parent.height - y - (linesBox.visible ? linesBox.height + Style.spacing.md : 0) - (exportStatusText.visible ? exportStatusText.height + Style.spacing.md : 0) - (notesGrip.visible ? notesGrip.height + Style.spacing.md : 0) - (notesBox.visible ? notesBox.height + Style.spacing.md : 0))
            clip: true
            model: root.sessions.filter(function(s) { return root.match(s.title); }).sort(function(a, b) { return root.when(b) - root.when(a); })
            delegate: Item {
                id: rrow
                required property var modelData
                readonly property string rowId: String(modelData.id)
                readonly property string dateText: { var t = root.when(modelData); return t ? Qt.formatDateTime(new Date(t), Qt.locale().dateTimeFormat(Locale.ShortFormat)) : ""; }
                readonly property string accName: Accessible.name
                objectName: modelData.kind === "meeting" ? "openNotes" : "openChat"
                width: ListView.view.width
                height: rowColumn.implicitHeight
                activeFocusOnTab: true
                Accessible.role: Accessible.Button
                Accessible.name: (modelData.kind === "meeting" ? "Meeting: " : "Chat: ") + modelData.title + ", " + dateText
                signal clicked()
                onClicked: root.select(modelData)
                Keys.onReturnPressed: clicked()
                Keys.onEnterPressed: clicked()
                Keys.onSpacePressed: clicked()
                Accessible.onPressAction: clicked()
                Rectangle {
                    anchors.fill: parent
                    radius: root.theme.radius
                    color: root.selectedId === rrow.rowId ? Qt.alpha(Commons.Color.accent, 0.18) : "transparent"
                    visible: root.selectedId === rrow.rowId || rrow.activeFocus
                    border.width: rrow.activeFocus ? 2 : 0
                    border.color: Commons.Color.accent
                }
                Rectangle {
                    objectName: "selectedBar"
                    visible: root.selectedId === rrow.rowId
                    x: 0; y: 0; width: 3; height: rrow.height
                    color: Commons.Color.accent
                }
                TextMetrics { id: kindMetrics; font.family: Style.font.family; font.pixelSize: root.theme.caption; text: "Meeting" }
                TextMetrics { id: dateMetrics; font.family: Style.font.family; font.pixelSize: root.theme.caption; text: Qt.formatDateTime(new Date(2026, 11, 28, 23, 59), Qt.locale().dateTimeFormat(Locale.ShortFormat)) }
                Column {
                    id: rowColumn
                    width: parent.width
                    spacing: Style.spacing.xs
                    RowLayout {
                        id: rowFirst
                        width: parent.width
                        height: root.theme.ctl
                        spacing: root.theme.gap
                        UiText {
                            Layout.preferredWidth: kindMetrics.width
                            Layout.alignment: Qt.AlignVCenter
                            textFormat: Text.PlainText
                            text: rrow.modelData.kind === "meeting" ? "Meeting" : "Chat"
                            color: Commons.Color.popups.text
                            opacity: root.theme.dim
                            font.pixelSize: root.theme.caption
                        }
                        UiText {
                            id: rowTitle
                            Layout.fillWidth: true
                            Layout.alignment: Qt.AlignVCenter
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                            mayCut: true
                            text: rrow.modelData.title
                            color: Commons.Color.popups.text
                            font.pixelSize: root.theme.body
                        }
                        UiText {
                            objectName: "sessionWhen"
                            Layout.preferredWidth: dateMetrics.width
                            Layout.minimumWidth: dateMetrics.width
                            Layout.alignment: Qt.AlignVCenter
                            textFormat: Text.PlainText
                            text: rrow.dateText
                            color: Commons.Color.popups.text
                            opacity: root.theme.dim
                            font.pixelSize: root.theme.caption
                        }
                        IconButton { objectName: "restore"; ts: root.theme.ts; Layout.alignment: Qt.AlignVCenter; text: "Restore"; onClicked: root.client.send("session.restore", { id: rrow.modelData.id }) }
                        IconButton { objectName: "rowMore"; ts: root.theme.ts; Layout.alignment: Qt.AlignVCenter; text: "⋯"; Accessible.name: "More actions"; onClicked: root.menuFor = root.menuFor === rrow.rowId ? "" : rrow.rowId }
                    }
                    UiText {
                        visible: rrow.modelData.kind === "meeting" && (Number(rrow.modelData.duration_seconds) > 0 || Number(rrow.modelData.line_count) > 0)
                        textFormat: Text.PlainText
                        text: (Number(rrow.modelData.duration_seconds) > 0 ? (Number(rrow.modelData.duration_seconds) < 60 ? "< 1 min" : Math.floor(Number(rrow.modelData.duration_seconds) / 60) + " min") : "") + (Number(rrow.modelData.duration_seconds) > 0 && Number(rrow.modelData.line_count) > 0 ? " · " : "") + (Number(rrow.modelData.line_count) > 0 ? Number(rrow.modelData.line_count) + " lines" : "")
                        color: Commons.Color.popups.text
                        opacity: root.theme.dim
                        font.pixelSize: root.theme.caption
                    }
                    Row {
                        visible: root.menuFor === rrow.rowId
                        width: parent.width
                        height: visible ? root.theme.ctl : 0
                        spacing: root.theme.gap
                        layoutDirection: Qt.RightToLeft
                        IconButton { objectName: "deleteSession"; ts: root.theme.ts; text: "Delete"; onClicked: { var id = rrow.modelData.id, rid = rrow.rowId, kind = rrow.modelData.kind;   // the row may be rebuilt while the dialog is open
                            root.confirmRequested("Delete \"" + rrow.modelData.title + "\"? " + (kind === "meeting" ? "The transcript, notes and items of this meeting are removed." : "This chat is removed."), "Delete", function() { root.client.send("session.delete", { id: id }); if (root.selectedId === rid) { root.selectedId = ""; root.notes = ""; root.notesStale = false; root.notesPhase = ""; } root.menuFor = ""; root.load(); }); } }
                        IconButton { objectName: "exportNow"; ts: root.theme.ts; text: "Export now"; visible: rrow.modelData.kind === "meeting"; onClicked: root.exportMeeting(rrow.modelData.id) }
                    }
                }
                MouseArea {
                    x: 0; y: 0; width: rowTitle.x + rowTitle.width; height: rowFirst.height
                    cursorShape: Qt.PointingHandCursor
                    onClicked: rrow.clicked()
                }
            }
        }
        Item {   // transcript lines that match the search
            id: linesBox
            objectName: "linesBox"
            visible: root.tab === "restore" && root.lineHits.length > 0
            width: parent.width
            height: visible ? Math.min(linesHead.height + linesList.contentHeight + Style.spacing.xs, Math.max(root.theme.ctl * 3, parent.height * 0.4)) : 0
            UiText { id: linesHead; textFormat: Text.PlainText; text: "Lines"; color: Commons.Color.popups.text; opacity: root.theme.dim; font.pixelSize: root.theme.caption; font.bold: true }
            ListView {
                id: linesList
                objectName: "lineHits"
                anchors { top: linesHead.bottom; topMargin: Style.spacing.xs; left: parent.left; right: parent.right; bottom: parent.bottom }
                clip: true
                spacing: Style.spacing.xs
                model: root.lineHits
                delegate: Item {
                    id: hit
                    objectName: "lineHit" + index
                    required property var modelData
                    required property int index
                    readonly property bool canOpen: String(modelData.meeting) === root.client.currentMeeting || !root.client.recording
                    width: ListView.view.width
                    height: hcol.implicitHeight
                    opacity: canOpen ? 1 : 0.6
                    Accessible.role: Accessible.Button
                    Accessible.name: String(modelData.text)
                    Accessible.onPressAction: { if (hit.canOpen) root.openHit(hit.modelData); }
                    Column {
                        id: hcol
                        width: parent.width
                        UiText {
                            width: parent.width
                            textFormat: Text.StyledText
                            wrapMode: Text.Wrap
                            maximumLineCount: 2
                            elide: Text.ElideRight
                            mayCut: true
                            text: root.esc(root.client.fmtTime(hit.modelData.t0)) + " \u00b7 " + root.esc(hit.modelData.speaker || (hit.modelData.who === "mic" ? "You" : hit.modelData.who === "room" ? "Room" : "Them")) + ": " + root.marked(hit.modelData.text, root.query.trim())
                            color: Commons.Color.popups.text
                            font.pixelSize: root.theme.body
                        }
                        UiText {
                            width: parent.width
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                            mayCut: true
                            text: String(hit.modelData.title || "") + (hit.canOpen ? "" : "  \u00b7  Stop recording to open")
                            color: Commons.Color.popups.text
                            opacity: root.theme.dim
                            font.pixelSize: root.theme.caption
                        }
                    }
                    MouseArea { anchors.fill: parent; enabled: hit.canOpen; cursorShape: Qt.PointingHandCursor; onClicked: root.openHit(hit.modelData) }
                }
            }
        }
        UiText {
            id: exportStatusText
            objectName: "exportStatus"
            visible: root.tab === "restore" && root.exportStatus !== ""
            width: parent.width
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            text: root.exportStatus
            color: Commons.Color.popups.text
            font.pixelSize: root.theme.caption
        }
        Item {   // drag up or down to resize the notes pane
            id: notesGrip
            objectName: "notesGrip"
            visible: notesBox.visible
            width: parent.width; height: 6
            activeFocusOnTab: true
            Accessible.role: Accessible.Slider
            Accessible.name: "Notes pane height"
            Accessible.description: "Up makes the notes pane taller, Down makes it shorter"
            // keyboard and assistive tech: one step is a line of body text
            function step(dir) { root.setNotesHeight(notesBox.height + dir * root.theme.body * 1.5); }
            Keys.onUpPressed: step(1)
            Keys.onDownPressed: step(-1)
            Accessible.onIncreaseAction: step(1)
            Accessible.onDecreaseAction: step(-1)
            Rectangle { anchors.centerIn: parent; width: Style.space(40); height: notesGrip.activeFocus ? 4 : 2; radius: height / 2; color: notesGrip.activeFocus ? Commons.Color.accent : Commons.Color.foreground; opacity: notesGrip.activeFocus ? 1 : root.theme.dim }
            MouseArea {
                anchors.fill: parent
                objectName: "notesDrag"
                cursorShape: Qt.SizeVerCursor
                property real pressY: 0
                property real startH: 0
                // window y, so the grip moving with the pane does not feed back into the drag
                function begin(y) { pressY = y; startH = notesBox.height; }
                function moveTo(y) { root.setNotesHeight(startH - (y - pressY)); }
                onPressed: function(m) { begin(mapToItem(null, m.x, m.y).y); }
                onPositionChanged: function(m) { if (pressed) moveTo(mapToItem(null, m.x, m.y).y); }
            }
        }
        ScrollView {   // long notes scroll
            id: notesBox
            objectName: "notesBox"
            visible: root.tab === "restore" && root.notesPhase !== ""
            width: parent.width
            height: visible ? Math.round(Math.max(0, Math.min(root.notesHeight, root.notesMax))) : 0
            contentWidth: availableWidth
            clip: true
            Column {
                width: notesBox.availableWidth
                spacing: Style.spacing.xs
                Column {
                    objectName: "notesTitle"
                    width: parent.width
                    UiText {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: { var s = root.sessions.find(function(row) { return row.kind === "meeting" && String(row.id) === root.selectedId; }); return s ? String(s.title) : ""; }
                        wrapMode: Text.Wrap   // a long title grows the header instead of being cut
                        color: Commons.Color.popups.text
                        font.pixelSize: root.theme.body
                        font.bold: true
                    }
                    UiText {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: { var s = root.sessions.find(function(row) { return row.kind === "meeting" && String(row.id) === root.selectedId; }); var t = s ? root.when(s) : 0; return t ? Qt.formatDateTime(new Date(t), Qt.locale().dateTimeFormat(Locale.ShortFormat)) : ""; }
                        color: Commons.Color.popups.text
                        opacity: root.theme.dim
                        font.pixelSize: root.theme.caption
                    }
                }
                UiText {
                    objectName: "notesStale"
                    visible: root.notesStale
                    width: parent.width
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    text: "Names changed after these notes were written"
                    color: Commons.Color.popups.text
                    opacity: root.theme.dim
                    font.italic: true
                    font.pixelSize: root.theme.caption
                }
                UiText {
                    objectName: "notesStateText"
                    visible: root.notesPhase !== "ready"
                    width: parent.width
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    text: root.notesPhase === "loading" ? "Loading notes…" : (root.notesPhase === "empty" ? "No notes yet for this meeting." : "Notes failed: " + root.notes)
                    color: Commons.Color.popups.text
                    opacity: root.theme.dim
                    font.pixelSize: root.theme.body
                }
                UiText {
                    visible: root.notesPhase === "ready"
                    width: parent.width
                    textFormat: Text.MarkdownText
                    wrapMode: Text.Wrap
                    text: root.capHeadings(root.notes)
                    color: Commons.Color.popups.text
                    font.pixelSize: root.theme.body
                }
            }
        }

        component Field: TextArea {
            id: fld
            wrapMode: TextEdit.Wrap
            color: Commons.Color.popups.text
            font.family: Style.font.family
            font.pixelSize: root.theme.body
            background: BorderSurface {
                color: Style.controlFill(fld.activeFocus, fld.hovered, Commons.Color.foreground, Commons.Color.accent)
                borderSpec: Border.controlSpec(fld.activeFocus ? "focus" : (fld.hovered ? "hover-cursor" : "normal"), Commons.Color.foreground, Commons.Color.accent)
                radius: Style.cornerRadius
            }
        }
        component Hint: UiText {
            width: parent.width
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: Commons.Color.popups.text
            opacity: root.theme.dim
            font.pixelSize: root.theme.caption
        }

        Row {
            visible: root.tab === "profiles"
            width: parent.width; height: parent.height - y
            spacing: Style.spacing.md
            Column {   // list + New
                width: Math.max(Style.space(150) * root.theme.ts, parent.width * 0.38); height: parent.height
                spacing: Style.spacing.xs
                ListView {
                    width: parent.width; height: Math.min(contentHeight, parent.height - newBtn.height - parent.spacing)   // New sits right under the list
                    clip: true
                    model: root.profiles.filter(function(p) { return root.match(p.name); })
                    delegate: IconButton {
                        required property var modelData
                        ts: root.theme.ts
                        leftAlign: true
                        width: ListView.view.width
                        elidable: true
                        text: (modelData.selected ? "\u25cf  " : "") + modelData.name
                        tooltipText: modelData.selected ? "In use" : ""
                        Accessible.name: modelData.name + (modelData.selected ? ", in use" : "")
                        active: root.cur && root.cur.name === modelData.name
                        onClicked: { root.selName = modelData.name; root.client.send("profiles.select", { name: modelData.name }, root.reload); }
                    }
                }
                IconButton { id: newBtn; ts: root.theme.ts; text: "New"; onClicked: root.newProfile() }
            }
            ScrollView {
                id: editor
                visible: root.cur !== null
                width: parent.width - parent.children[0].width - parent.spacing; height: parent.height
                contentWidth: availableWidth
                clip: true
                Column {
                    width: editor.availableWidth
                    spacing: Style.spacing.xs
                    RowLayout {
                        width: parent.width
                        spacing: root.theme.gap
                        Input { id: nameField; objectName: "profileName"; accessibleName: "Profile name"; ts: root.theme.ts; Layout.fillWidth: true; Layout.alignment: Qt.AlignVCenter }
                        IconButton { ts: root.theme.ts; Layout.alignment: Qt.AlignVCenter; text: "Rename"; enabled: root.cur !== null && nameField.text.trim() !== "" && nameField.text !== root.cur.name; opacity: enabled ? 1 : 0.5
                            onClicked: { var n = nameField.text.trim(), o = root.cur.name; root.selName = n; root.client.send("profiles.rename", { name: o, new_name: n }, root.reload); } }
                        IconButton { objectName: "deleteProfile"; ts: root.theme.ts; Layout.alignment: Qt.AlignVCenter; text: "Delete"; onClicked: { var p = root.cur; root.confirmRequested("Delete the profile \"" + p.name + "\"? Its about-me, meeting brief and " + (p.documents || []).length + " attached document(s) are removed.", "Delete", function() { root.client.send("profiles.delete", { name: p.name }, root.reload); root.selName = ""; }); } }
                    }
                    UiText { textFormat: Text.PlainText; text: "About me"; color: Commons.Color.popups.text; font.bold: true; font.pixelSize: root.theme.body }
                    Hint { text: "Your role and experience; used in every answer." }
                    Field { id: aboutText; Accessible.name: "About me"; width: parent.width; height: Math.round(Style.space(64) * root.theme.ts) }
                    UiText { textFormat: Text.PlainText; text: "Meeting brief"; color: Commons.Color.popups.text; font.bold: true; font.pixelSize: root.theme.body }
                    Hint { text: "What this meeting is about (role, company, agenda)." }
                    Field { id: briefText; Accessible.name: "Meeting brief"; width: parent.width; height: Math.round(Style.space(64) * root.theme.ts) }
                    IconButton { ts: root.theme.ts; text: "Save"; onClicked: root.saveProfile() }
                    UiText { textFormat: Text.PlainText; text: "Documents"; color: Commons.Color.popups.text; font.bold: true; font.pixelSize: root.theme.body; topPadding: Style.spacing.xs }
                    Hint { text: "CV, job description, runbooks (PDF, text, markdown). Newest first in the prompt." }
                    Repeater {
                        model: root.cur ? (root.cur.documents || []) : []
                        RowLayout {
                            id: drow
                            required property var modelData
                            width: parent.width
                            spacing: root.theme.gap
                            UiText {
                                Layout.fillWidth: true
                                Layout.alignment: Qt.AlignVCenter
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                                mayCut: true
                                text: drow.modelData.name + "  ·  " + drow.modelData.chars + " chars"
                                color: Commons.Color.popups.text
                                font.pixelSize: root.theme.body
                            }
                            IconButton { ts: root.theme.ts; Layout.alignment: Qt.AlignVCenter; text: "Remove"; onClicked: root.client.send("profiles.detach", { name: root.cur.name, id: drow.modelData.id }, root.reload) }
                        }
                    }
                    IconButton { ts: root.theme.ts; text: "Attach"; onClicked: attachDialog.open() }
                }
            }
            FileDialog {
                id: attachDialog
                nameFilters: ["Documents (*.pdf *.txt *.md)"]
                onAccepted: {
                    var p = decodeURIComponent(String(selectedFile).replace(/^file:\/\//, ""));
                    root.client.send("profiles.attach", { name: root.cur.name, path: p }, function(r) { if (!r.ok) root.error = String(r.error); root.reload(); });
                }
            }
        }

        CandidateTab {
            id: candTab
            visible: root.tab === "candidate"
            client: root.client
            theme: root.theme
            onConfirmRequested: function(message, label, yes) { root.confirmRequested(message, label, yes); }
            width: parent.width; height: parent.height - y
        }

        Column {
            visible: root.tab === "prompts"
            width: parent.width; height: parent.height - y
            spacing: Style.spacing.xs
            Flow {   // which mode (the names of the mode picker)
                width: parent.width
                spacing: Style.spacing.xs
                Repeater {
                    model: root.promptModes
                    IconButton { ts: root.theme.ts; required property var modelData; text: modelData.name; active: root.promptMode === modelData.value; onClicked: root.promptMode = modelData.value }
                }
            }
            Flow {   // which prompt; a second row, so each row has one active button
                width: parent.width
                spacing: Style.spacing.xs
                Repeater {
                    model: root.promptKinds
                    IconButton { ts: root.theme.ts; required property var modelData; text: modelData.name; active: root.promptKind === modelData.value; onClicked: root.promptKind = modelData.value }
                }
            }
            Hint { text: root.kindHelp[root.promptKind] || "" }
            ScrollView {
                width: parent.width; height: parent.height - y - promptBtns.height - parent.spacing
                TextArea {
                    id: promptText
                    Accessible.name: "Prompt text"
                    wrapMode: TextEdit.Wrap
                    color: Commons.Color.popups.text
                    font.family: Style.font.family
                    font.pixelSize: root.theme.body
                    background: BorderSurface {
                        color: Style.controlFill(promptText.activeFocus, promptText.hovered, Commons.Color.foreground, Commons.Color.accent)
                        borderSpec: Border.controlSpec(promptText.activeFocus ? "focus" : (promptText.hovered ? "hover-cursor" : "normal"), Commons.Color.foreground, Commons.Color.accent)
                        radius: Style.cornerRadius
                    }
                }
            }
            Flow {
                id: promptBtns
                width: parent.width
                spacing: Style.spacing.xs
                IconButton { ts: root.theme.ts; text: "Save"; onClicked: root.client.send("prompts.set", { mode: root.promptMode, kind: root.promptKind, text: promptText.text }) }
                IconButton { ts: root.theme.ts; text: "Reset to default"; onClicked: root.client.send("prompts.reset", { mode: root.promptMode, kind: root.promptKind }, function() { root.loadPrompt(); }) }
            }
        }

        ListView {
            visible: root.tab === "providers" && root.provMode === ""
            width: parent.width; height: parent.height - y
            clip: true
            spacing: Style.spacing.xs
            model: root.providers.filter(function(p) { return !p.custom && root.match(p.name || p.id); })
            header: Column {
                width: ListView.view ? ListView.view.width : parent.width
                spacing: Style.spacing.xs
                bottomPadding: Style.spacing.sm
                Rectangle {   // the daemon's one-time notice (the removed Gemini key)
                    visible: root.client.providerNotice !== ""
                    width: parent.width
                    height: visible ? pn.implicitHeight + root.theme.padY * 2 : 0
                    radius: root.theme.radius
                    color: Qt.alpha(Commons.Color.accent, 0.12)
                    RowLayout {
                        id: pn
                        x: root.theme.padX; y: root.theme.padY
                        width: parent.width - root.theme.padX * 2
                        spacing: root.theme.gap
                        UiText { Layout.fillWidth: true; Layout.alignment: Qt.AlignVCenter; textFormat: Text.PlainText; wrapMode: Text.Wrap; text: root.client.providerNotice; color: Commons.Color.popups.text; font.pixelSize: root.theme.body }
                        IconButton { objectName: "noticeDismiss"; ts: root.theme.ts; Layout.alignment: Qt.AlignVCenter; text: "Got it"; bordered: true; onClicked: root.client.providerNotice = "" }
                    }
                }
                RowLayout {
                    width: parent.width
                    height: root.theme.ctl
                    spacing: root.theme.gap
                    UiText {
                        Layout.fillWidth: true
                        Layout.alignment: Qt.AlignVCenter
                        textFormat: Text.PlainText
                        text: "Your providers"
                        color: Commons.Color.popups.text
                        font.pixelSize: root.theme.body
                        font.bold: true
                    }
                    IconButton {
                        objectName: "addProvider"
                        ts: root.theme.ts
                        Layout.alignment: Qt.AlignVCenter
                        text: "Add provider"
                        bordered: true
                        onClicked: root.openPresetList()
                    }
                }
                Repeater {
                    model: root.providers.filter(function(p) { return p.custom === true && root.match(p.name || p.id); })
                    Column {
                        id: cprow
                        required property var modelData
                        readonly property var usedBy: ["chat_provider", "live_provider", "speech_provider"].filter(function(k) { return String(root.client.settings[k] || "") === cprow.modelData.id; })
                        readonly property bool removeLocked: usedBy.some(function(k) { return root.client.isLocked(k); })
                        width: parent.width
                        spacing: Style.spacing.xs
                        bottomPadding: Style.spacing.xs
                        RowLayout {
                            width: parent.width
                            height: root.theme.ctl
                            spacing: root.theme.gap
                            UiText {
                                objectName: "customProviderName"
                                Layout.fillWidth: true
                                Layout.alignment: Qt.AlignVCenter
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                                mayCut: true
                                text: cprow.modelData.name || cprow.modelData.id
                                color: Commons.Color.popups.text
                                font.pixelSize: root.theme.body
                                font.bold: true
                            }
                            IconButton {
                                objectName: "useProvider"
                                ts: root.theme.ts
                                Layout.alignment: Qt.AlignVCenter
                                text: cprow.modelData.kind === "speech" ? "Use for speech" : "Use for chat"
                                enabled: !root.client.isLocked(cprow.modelData.kind === "speech" ? "speech_provider" : "chat_provider")
                                opacity: enabled ? 1 : 0.5
                                onClicked: root.client.setSetting(cprow.modelData.kind === "speech" ? "speech_provider" : "chat_provider", cprow.modelData.id)
                            }
                            IconButton {
                                objectName: "editProvider"
                                ts: root.theme.ts
                                Layout.alignment: Qt.AlignVCenter
                                text: "Edit"
                                onClicked: root.openEditProvider(cprow.modelData)
                            }
                            IconButton {
                                objectName: "removeProvider"
                                ts: root.theme.ts
                                Layout.alignment: Qt.AlignVCenter
                                text: "Remove"
                                enabled: !cprow.removeLocked
                                opacity: enabled ? 1 : 0.5
                                onClicked: {
                                    var p = cprow.modelData;
                                    root.confirmRequested("Remove \"" + (p.name || p.id) + "\"?", "Remove", function() {
                                        root.client.send("providers.remove", { id: p.id }, function(r) {
                                            if (!r.ok) root.error = String(r.error || "request failed");
                                            root.load();
                                        });
                                    });
                                }
                            }
                        }
                        UiText {
                            objectName: "customProviderMeta"
                            width: parent.width
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                            text: (cprow.modelData.kind === "speech" ? "Speech" : "Chat") + "  ·  " + String(cprow.modelData.model || "") + "  ·  " + root.endpointHost(cprow.modelData.endpoint) + "  ·  " + (cprow.modelData.requires_key === false ? "on this computer" : (cprow.modelData.has_key ? "key set" : "no key")) + (cprow.usedBy.length ? "  ·  in use for " + cprow.usedBy.map(function(k) { return k === "speech_provider" ? "speech" : (k === "live_provider" ? "live answers" : "chat"); }).join(", ") : "") + (cprow.removeLocked ? "  ·  set in Nix, so it cannot be removed" : "")
                            color: Commons.Color.popups.text
                            opacity: root.theme.dim
                            font.pixelSize: root.theme.caption
                        }
                    }
                }
                PanelSeparator { width: parent.width }
            }
            delegate: Column {   // the provider and its Models button; a key row if it takes one; then its status and, when not ready, the fix
                id: prow
                required property var modelData
                readonly property var st: root.status[modelData.id] || null
                width: ListView.view.width
                spacing: Style.spacing.xs
                bottomPadding: Style.spacing.sm
                RowLayout {
                    width: parent.width
                    height: root.theme.ctl
                    spacing: root.theme.gap
                    UiText {
                        Layout.fillWidth: true
                        Layout.alignment: Qt.AlignVCenter
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        text: prow.modelData.name || prow.modelData.id
                        color: Commons.Color.popups.text
                        font.pixelSize: root.theme.body
                        font.bold: true
                    }
                    IconButton { ts: root.theme.ts; Layout.alignment: Qt.AlignVCenter;
                        text: "Models"
                        onClicked: { root.modelsFor = prow.modelData.id; root.client.send("models.list", { provider: prow.modelData.id }, function(r) { if (r.provider !== undefined && r.provider !== root.modelsFor) return; root.modelsText = r.ok ? (r.data || []).join(", ") : "Error: " + r.error; }); }
                    }
                }
                RowLayout {   // the key: only for a provider that takes one (Antigravity, the CLIs and local Ollama have none)
                    visible: prow.modelData.requires_key !== false
                    width: parent.width
                    height: root.theme.ctl
                    spacing: root.theme.gap
                    Input {
                        id: keyField
                        objectName: "apiKey"
                        accessibleName: "API key for " + (prow.modelData.name || prow.modelData.id)
                        ts: root.theme.ts
                        Layout.fillWidth: true
                        Layout.alignment: Qt.AlignVCenter
                        echoMode: TextInput.Password
                        placeholderText: prow.modelData.has_key ? "API key (set; type a new one to replace it)" : "API key"
                    }
                    IconButton { ts: root.theme.ts; Layout.alignment: Qt.AlignVCenter;
                        text: "Set key"
                        enabled: keyField.text !== ""
                        opacity: enabled ? 1 : 0.5
                        onClicked: { root.client.send("settings.set", { key: "api_key", provider: prow.modelData.id, value: keyField.text }); keyField.text = ""; root.load(); }
                    }
                }
                UiText {   // ready / not installed / not logged in / no key
                    width: parent.width
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    text: prow.st ? "Status: " + (root.stateLabels[prow.st.state] || prow.st.state) + (prow.st.state === "ready" || !prow.st.login ? "" : ". To fix, run:") : ""
                    visible: text !== ""
                    color: Commons.Color.popups.text
                    opacity: root.theme.dim
                    font.pixelSize: root.theme.caption
                }
                CommandRow {   // the fix, to copy; a ready provider needs none
                    visible: prow.st !== null && prow.st.state !== "ready" && !!prow.st.login
                    width: parent.width
                    theme: root.theme
                    cmd: prow.st ? String(prow.st.login) : ""
                }
            }
            footer: UiText { text: root.modelsText; width: parent.width; wrapMode: Text.Wrap; textFormat: Text.PlainText; color: Commons.Color.popups.text; font.pixelSize: root.theme.caption }
        }

        ScrollView {
            id: presetScroll
            visible: root.tab === "providers" && root.provMode === "presets"
            width: parent.width; height: parent.height - y
            contentWidth: availableWidth
            clip: true
            Column {
                width: presetScroll.availableWidth
                spacing: Style.spacing.xs
                RowLayout {
                    width: parent.width
                    height: root.theme.ctl
                    spacing: root.theme.gap
                    IconButton {
                        objectName: "presetBack"
                        ts: root.theme.ts
                        Layout.alignment: Qt.AlignVCenter
                        text: "Back"
                        onClicked: { root.error = ""; root.provMode = ""; }
                    }
                    UiText {
                        Layout.fillWidth: true
                        Layout.alignment: Qt.AlignVCenter
                        textFormat: Text.PlainText
                        text: "Choose a preset"
                        color: Commons.Color.popups.text
                        font.pixelSize: root.theme.body
                        font.bold: true
                    }
                }
                UiText {
                    textFormat: Text.PlainText
                    text: "Chat"
                    color: Commons.Color.popups.text
                    font.pixelSize: root.theme.body
                    font.bold: true
                    topPadding: Style.spacing.xs
                }
                Repeater {
                    model: root.presets.filter(function(p) { return p.kind === "chat" && root.match(p.name); })
                    IconButton {
                        required property var modelData
                        objectName: "preset-" + modelData.preset
                        ts: root.theme.ts
                        leftAlign: true
                        width: parent.width
                        elidable: true
                        text: modelData.name + (modelData.local ? "  ·  on this computer" : "") + "  ·  " + modelData.model
                        onClicked: root.openPresetForm(modelData)
                    }
                }
                UiText {
                    textFormat: Text.PlainText
                    text: "Speech"
                    color: Commons.Color.popups.text
                    font.pixelSize: root.theme.body
                    font.bold: true
                    topPadding: Style.spacing.xs
                }
                Repeater {
                    model: root.presets.filter(function(p) { return p.kind === "speech" && root.match(p.name); })
                    IconButton {
                        required property var modelData
                        objectName: "preset-" + modelData.preset
                        ts: root.theme.ts
                        leftAlign: true
                        width: parent.width
                        elidable: true
                        text: modelData.name + (modelData.local ? "  ·  on this computer" : "") + "  ·  " + modelData.model
                        onClicked: root.openPresetForm(modelData)
                    }
                }
            }
        }

        ScrollView {
            id: formScroll
            visible: root.tab === "providers" && root.provMode === "form"
            width: parent.width; height: parent.height - y
            contentWidth: availableWidth
            clip: true
            Column {
                width: formScroll.availableWidth
                spacing: Style.spacing.xs
                RowLayout {
                    width: parent.width
                    height: root.theme.ctl
                    spacing: root.theme.gap
                    IconButton {
                        objectName: "formBack"
                        ts: root.theme.ts
                        Layout.alignment: Qt.AlignVCenter
                        text: "Back"
                        onClicked: { root.error = ""; root.testMessage = ""; root.provMode = root.editId !== "" ? "" : "presets"; }
                    }
                    UiText {
                        Layout.fillWidth: true
                        Layout.alignment: Qt.AlignVCenter
                        textFormat: Text.PlainText
                        text: root.editId !== "" ? "Edit provider" : "Add provider"
                        color: Commons.Color.popups.text
                        font.pixelSize: root.theme.body
                        font.bold: true
                    }
                }
                UiText { textFormat: Text.PlainText; text: "Name"; color: Commons.Color.popups.text; font.bold: true; font.pixelSize: root.theme.body }
                Input {
                    id: provName
                    objectName: "providerName"
                    accessibleName: "Provider name"
                    ts: root.theme.ts
                    width: parent.width
                    placeholderText: "Provider name"
                }
                UiText { textFormat: Text.PlainText; text: "Endpoint"; color: Commons.Color.popups.text; font.bold: true; font.pixelSize: root.theme.body }
                Input {
                    id: provEndpoint
                    objectName: "providerEndpoint"
                    accessibleName: "Endpoint"
                    ts: root.theme.ts
                    width: parent.width
                    placeholderText: "https://..."
                }
                UiText { textFormat: Text.PlainText; text: "Model"; color: Commons.Color.popups.text; font.bold: true; font.pixelSize: root.theme.body }
                Input {
                    id: provModel
                    objectName: "providerModel"
                    accessibleName: "Model"
                    ts: root.theme.ts
                    width: parent.width
                    placeholderText: "Model id"
                }
                Column {
                    visible: root.formNeedsKey
                    width: parent.width
                    spacing: Style.spacing.xs
                    UiText { textFormat: Text.PlainText; text: "API key"; color: Commons.Color.popups.text; font.bold: true; font.pixelSize: root.theme.body }
                    Input {
                        id: provKey
                        objectName: "providerKey"
                        accessibleName: "API key"
                        ts: root.theme.ts
                        width: parent.width
                        echoMode: TextInput.Password
                        placeholderText: root.editId !== "" ? "Leave empty to keep the saved key" : "API key"
                    }
                }
                RowLayout {
                    width: parent.width
                    height: root.theme.ctl
                    spacing: root.theme.gap
                    IconButton {
                        objectName: "testProvider"
                        ts: root.theme.ts
                        Layout.alignment: Qt.AlignVCenter
                        text: root.testingProvider ? "Testing…" : "Test connection"
                        bordered: true
                        enabled: !root.testingProvider
                        opacity: enabled ? 1 : 0.5
                        onClicked: root.testProvider()
                    }
                    IconButton {
                        objectName: "saveProvider"
                        ts: root.theme.ts
                        Layout.alignment: Qt.AlignVCenter
                        text: "Save"
                        bordered: true
                        onClicked: root.saveProvider()
                    }
                    Item { Layout.fillWidth: true }
                }
                UiText {
                    objectName: "testMessage"
                    visible: root.testMessage !== ""
                    width: parent.width
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    text: root.testMessage
                    color: Commons.Color.popups.text
                    font.pixelSize: root.theme.caption
                }
            }
        }

        CallState { id: callState }
        Hint {   // no state file 30 s after start: nothing detects calls
            visible: root.tab === "settings" && callState.missing   // the Column skips it while hidden
            text: "Call detection is off: enable the meeting-mode plugin (omarchy plugin enable nixarchy.meeting-mode --section center)."
        }

        ListView {
            visible: root.tab === "settings"
            width: parent.width; height: parent.height - y
            clip: true
            model: root.found
            header: root.settingsHeader
            delegate: RowLayout {   // label left, control right, both centred on the row
                id: srow
                required property var modelData
                width: ListView.view.width
                height: Math.max(root.theme.ctl, implicitHeight) + Style.spacing.sm
                spacing: root.theme.gap
                UiText {
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignVCenter
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    text: root.client.lockedLabel(srow.modelData.key, root.labelOverrides[srow.modelData.key] || srow.modelData.label || srow.modelData.key)
                    color: Commons.Color.popups.text
                    font.pixelSize: root.theme.body
                }
                Item {
                    Layout.preferredWidth: Math.round(Style.space(220) * root.theme.ts)
                    Layout.preferredHeight: root.theme.ctl
                    Layout.alignment: Qt.AlignVCenter
                    Check {
                        objectName: srow.modelData.key === "export_meetings" ? "exportMeetingsToggle" : "settingToggle"
                        ts: root.theme.ts
                        accessibleName: String(srow.modelData.label || srow.modelData.key)
                        enabled: !root.client.isLocked(srow.modelData.key)
                        visible: typeof srow.modelData.value === "boolean"
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        checked: srow.modelData.value === true
                        onToggled: { root.client.setSetting(srow.modelData.key, !srow.modelData.value); root.search(); }
                    }
                    RowLayout {
                        visible: srow.modelData.key === "export_folder"
                        anchors.fill: parent
                        spacing: root.theme.gap
                        UiText {
                            objectName: "exportFolderField"
                            Layout.fillWidth: true
                            Layout.alignment: Qt.AlignVCenter
                            textFormat: Text.PlainText
                            elide: Text.ElideMiddle
                            mayCut: true
                            text: String(srow.modelData.value || "") || "Not set"
                            color: Commons.Color.popups.text
                            font.pixelSize: root.theme.body
                        }
                        IconButton {
                            objectName: "exportFolderChange"
                            ts: root.theme.ts
                            Layout.alignment: Qt.AlignVCenter
                            text: "Change…"
                            enabled: !root.client.isLocked("export_folder")
                            opacity: enabled ? 1 : 0.5
                            onClicked: { root.exportAfterPick = null; exportFolderDialog.open(); }
                        }
                    }
                    Input {
                        objectName: "settingField"
                        accessibleName: String(srow.modelData.label || srow.modelData.key)
                        ts: root.theme.ts
                        enabled: !root.client.isLocked(srow.modelData.key)
                        visible: typeof srow.modelData.value !== "boolean" && srow.modelData.key !== "export_folder"
                        anchors.fill: parent
                        text: String(srow.modelData.value)
                        onEditingFinished: root.client.setSetting(srow.modelData.key, typeof srow.modelData.value === "number" ? Number(text) : text)
                    }
                }
            }
        }
        FolderDialog {
            id: exportFolderDialog
            onAccepted: {
                var p = decodeURIComponent(String(selectedFolder).replace(/^file:\/\//, ""));
                root.setExportFolder(p);
            }
        }
    }
}
