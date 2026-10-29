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
    property string modelsText: ""
    property string modelsFor: ""   // the provider the Models button asked about; a reply for another one is stale
    property var status: ({})        // providers.status rows by id: ready / not installed / not logged in / no key, with the login command
    readonly property var stateLabels: ({ ready: "ready", not_installed: "not installed", not_logged_in: "not logged in", no_key: "no key" })
    property var presets: []        // presets.list rows
    property string provMode: ""    // "" | "presets" | "form"
    property string editId: ""      // custom provider id being edited, or "" when adding from a preset
    property string formPreset: ""
    property bool formNeedsKey: true
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
        root.formNeedsKey = p.local !== true && p.needs_key !== false;
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
        root.formNeedsKey = p.requires_key !== false;
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
        var args = { model: provModel.text.trim(), endpoint: provEndpoint.text.trim() };
        if (root.editId !== "") args.id = root.editId;
        else args.preset = root.formPreset;
        if (root.formNeedsKey && provKey.text !== "") args.api_key = provKey.text;
        root.client.send("providers.test", args, function(r) {
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
        root.search();
        root.loadPrompt();
        root.client.refreshCandidates();
    }
    function search() {
        root.client.send("settings.search", { query: root.query }, function(r) { if (r.ok) root.found = (r.data || []).filter(function(f) { return String(f.key).indexOf("overlay_") !== 0; }); });
    }

    function loadPrompt() {
        root.client.send("prompts.get", { mode: root.promptMode, kind: root.promptKind }, function(r) {
            promptText.text = r.ok ? String((r.data && r.data.text !== undefined) ? r.data.text : r.data) : "";
        });
    }

    property string error: ""   // the last failed request made from here, shown under the search field
    onTabChanged: { root.error = ""; root.provMode = ""; root.load(); }
    onPromptModeChanged: root.loadPrompt()
    onPromptKindChanged: root.loadPrompt()
    Component.onCompleted: root.load()

    Connections {
        target: root.client
        function onLibraryError(message) { root.error = message; }
        function onNotesUpdated() { root.load(); }
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
            placeholderText: "Search settings, chats, profiles"
            onTextChanged: { root.query = text; root.search(); }
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
            visible: root.tab === "restore"
            width: parent.width; height: parent.height - y - notesBox.height
            clip: true
            model: root.sessions.filter(function(s) { return root.match(s.title); })
            delegate: RowLayout {
                id: rrow
                required property var modelData
                width: ListView.view.width
                height: root.theme.ctl + Style.spacing.xs
                spacing: root.theme.gap
                UiText {
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignVCenter
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    mayCut: true
                    text: (rrow.modelData.kind === "meeting" ? "Meeting · " : "Chat · ") + rrow.modelData.title
                    color: Commons.Color.popups.text
                    font.pixelSize: root.theme.body
                }
                IconButton { objectName: "restore"; ts: root.theme.ts; Layout.alignment: Qt.AlignVCenter; text: "Restore"; onClicked: root.client.send("session.restore", { id: rrow.modelData.id }) }
                IconButton { ts: root.theme.ts; Layout.alignment: Qt.AlignVCenter;
                    text: "Notes"
                    visible: rrow.modelData.kind === "meeting"
                    onClicked: root.client.send("notes.get", { meeting: rrow.modelData.id }, function(r) { root.notes = r.ok ? String((r.data && r.data.text !== undefined) ? r.data.text : r.data) : r.error; })
                }
                IconButton { objectName: "deleteSession"; ts: root.theme.ts; Layout.alignment: Qt.AlignVCenter; text: "Delete"; onClicked: root.confirmRequested("Delete \"" + rrow.modelData.title + "\"? " + (rrow.modelData.kind === "meeting" ? "The transcript, notes and items of this meeting are removed." : "This chat is removed."), "Delete", function() { root.client.send("session.delete", { id: rrow.modelData.id }); root.load(); }) }
            }
        }
        ScrollView {   // long notes scroll
            id: notesBox
            visible: root.tab === "restore" && root.notes !== ""
            width: parent.width
            height: visible ? Math.round(Style.space(160) * root.theme.ts) : 0
            contentWidth: availableWidth
            clip: true
            UiText {
                width: notesBox.availableWidth
                textFormat: Text.MarkdownText
                wrapMode: Text.Wrap
                text: root.notes
                color: Commons.Color.popups.text
                font.pixelSize: root.theme.body
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
                            text: (cprow.modelData.kind === "speech" ? "Speech" : "Chat") + "  ·  " + String(cprow.modelData.model || "") + "  ·  " + root.endpointHost(cprow.modelData.endpoint) + "  ·  " + (cprow.modelData.requires_key === false ? "on this computer" : (cprow.modelData.has_key ? "key set" : "no key"))
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
                        objectName: "preset-speech-" + modelData.preset
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
                        objectName: "settingToggle"
                        ts: root.theme.ts
                        accessibleName: String(srow.modelData.label || srow.modelData.key)
                        enabled: !root.client.isLocked(srow.modelData.key)
                        visible: typeof srow.modelData.value === "boolean"
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        checked: srow.modelData.value === true
                        onToggled: { root.client.setSetting(srow.modelData.key, !srow.modelData.value); root.search(); }
                    }
                    Input {
                        objectName: "settingField"
                        accessibleName: String(srow.modelData.label || srow.modelData.key)
                        ts: root.theme.ts
                        enabled: !root.client.isLocked(srow.modelData.key)
                        visible: typeof srow.modelData.value !== "boolean"
                        anchors.fill: parent
                        text: String(srow.modelData.value)
                        onEditingFinished: root.client.setSetting(srow.modelData.key, typeof srow.modelData.value === "number" ? Number(text) : text)
                    }
                }
            }
        }
    }
}
