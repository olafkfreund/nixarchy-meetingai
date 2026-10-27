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
    property Component settingsHeader: null   // controls shown above the Settings list

    readonly property bool iv: client.settings.meeting_mode === "interviewer"
    readonly property var tabs: root.iv ? ["restore", "profiles", "candidate", "prompts", "providers", "settings"] : ["restore", "profiles", "prompts", "providers", "settings"]
    readonly property var promptKinds: ["answer", "live_answer", "follow_up", "recap"]
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
    function candidateHook(what) { candTab.hook(what); }

    function match(s) { return root.query === "" || String(s).toLowerCase().indexOf(root.query.toLowerCase()) >= 0; }

    function load() {
        root.client.send("session.list", {}, function(r) { if (r.ok) root.sessions = r.data || []; });
        root.reload();
        root.client.send("providers.list", {}, function(r) { if (r.ok) root.providers = r.data || []; });
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

    onTabChanged: root.load()
    onPromptModeChanged: root.loadPrompt()
    onPromptKindChanged: root.loadPrompt()
    Component.onCompleted: root.load()

    Connections {
        target: root.client
        function onNotesUpdated() { root.load(); }
    }

    Column {
        anchors.fill: parent
        spacing: Style.spacing.md

        Row {
            spacing: Style.spacing.xs
            Repeater {
                model: root.tabs
                IconButton { ts: root.theme.ts;
                    required property string modelData
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
                IconButton { ts: root.theme.ts; Layout.alignment: Qt.AlignVCenter; text: "Delete"; onClicked: { root.client.send("session.delete", { id: rrow.modelData.id }); root.load(); } }
            }
        }
        UiText {
            id: notesBox
            visible: root.tab === "restore" && root.notes !== ""
            width: parent.width
            height: visible ? Style.space(160) : 0
            textFormat: Text.MarkdownText
            wrapMode: Text.Wrap
            clip: true
            text: root.notes
            color: Commons.Color.popups.text
            font.pixelSize: root.theme.body
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
            opacity: 0.7
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
                        text: modelData.name + (modelData.selected ? "  (in use)" : "")
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
                        Input { id: nameField; objectName: "profileName"; ts: root.theme.ts; Layout.fillWidth: true; Layout.alignment: Qt.AlignVCenter }
                        IconButton { ts: root.theme.ts; Layout.alignment: Qt.AlignVCenter; text: "Rename"; enabled: root.cur !== null && nameField.text.trim() !== "" && nameField.text !== root.cur.name; opacity: enabled ? 1 : 0.5
                            onClicked: { var n = nameField.text.trim(), o = root.cur.name; root.selName = n; root.client.send("profiles.rename", { name: o, new_name: n }, root.reload); } }
                        IconButton { ts: root.theme.ts; Layout.alignment: Qt.AlignVCenter; text: "Delete"; onClicked: { root.client.send("profiles.delete", { name: root.cur.name }, root.reload); root.selName = ""; } }
                    }
                    UiText { textFormat: Text.PlainText; text: "About me"; color: Commons.Color.popups.text; font.bold: true; font.pixelSize: root.theme.body }
                    Hint { text: "Your role and experience; used in every answer." }
                    Field { id: aboutText; width: parent.width; height: Style.space(64) }
                    UiText { textFormat: Text.PlainText; text: "Meeting brief"; color: Commons.Color.popups.text; font.bold: true; font.pixelSize: root.theme.body }
                    Hint { text: "What this meeting is about (role, company, agenda)." }
                    Field { id: briefText; width: parent.width; height: Style.space(64) }
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
                    root.client.send("profiles.attach", { name: root.cur.name, path: p }, function(r) { if (!r.ok) root.client.lastError = String(r.error); root.reload(); });
                }
            }
        }

        CandidateTab {
            id: candTab
            visible: root.tab === "candidate"
            client: root.client
            theme: root.theme
            width: parent.width; height: parent.height - y
        }

        Column {
            visible: root.tab === "prompts"
            width: parent.width; height: parent.height - y
            spacing: Style.spacing.xs
            Flow {
                width: parent.width
                spacing: Style.spacing.xs
                Repeater {
                    model: ["general", "interview", "team_meeting", "interviewer"]
                    IconButton { ts: root.theme.ts; required property string modelData; text: modelData === "team_meeting" ? "Team" : modelData; active: root.promptMode === modelData; onClicked: root.promptMode = modelData }
                }
                Repeater {
                    model: root.promptKinds
                    IconButton { ts: root.theme.ts; required property string modelData; text: modelData; active: root.promptKind === modelData; onClicked: root.promptKind = modelData }
                }
            }
            Hint { text: root.kindHelp[root.promptKind] || "" }
            ScrollView {
                width: parent.width; height: parent.height - y - promptBtns.height - parent.spacing
                TextArea {
                    id: promptText
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
            visible: root.tab === "providers"
            width: parent.width; height: parent.height - y
            clip: true
            model: root.providers.filter(function(p) { return root.match(p.name || p.id); })
            delegate: RowLayout {
                id: prow
                required property var modelData
                width: ListView.view.width
                height: root.theme.ctl + Style.spacing.xs
                spacing: root.theme.gap
                UiText {
                    Layout.preferredWidth: Math.round(Style.space(220) * root.theme.ts)
                    Layout.fillWidth: !keyField.visible
                    Layout.alignment: Qt.AlignVCenter
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    text: (prow.modelData.name || prow.modelData.id) + (prow.modelData.requires_key === false ? "" : (prow.modelData.has_key ? " · key set" : " · no key"))
                    color: Commons.Color.popups.text
                    font.pixelSize: root.theme.body
                }
                Input {
                    id: keyField
                    objectName: "apiKey"
                    ts: root.theme.ts
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignVCenter
                    visible: prow.modelData.requires_key !== false
                    echoMode: TextInput.Password
                    placeholderText: "API key"
                }
                IconButton { ts: root.theme.ts; Layout.alignment: Qt.AlignVCenter;
                    text: "Set key"
                    visible: keyField.visible
                    enabled: keyField.text !== ""
                    opacity: enabled ? 1 : 0.5
                    onClicked: { root.client.send("settings.set", { key: "api_key", provider: prow.modelData.id, value: keyField.text }); keyField.text = ""; root.load(); }
                }
                IconButton { ts: root.theme.ts; Layout.alignment: Qt.AlignVCenter;
                    text: "Models"
                    onClicked: root.client.send("models.list", { provider: prow.modelData.id }, function(r) { root.modelsText = r.ok ? (r.data || []).join(", ") : r.error; })
                }
            }
            footer: UiText { text: root.modelsText; width: parent.width; wrapMode: Text.Wrap; textFormat: Text.PlainText; color: Commons.Color.popups.text; font.pixelSize: root.theme.caption }
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
                    text: root.labelOverrides[srow.modelData.key] || srow.modelData.label || srow.modelData.key
                    color: Commons.Color.popups.text
                    font.pixelSize: root.theme.body
                }
                Item {
                    Layout.preferredWidth: Math.round(Style.space(220) * root.theme.ts)
                    Layout.preferredHeight: root.theme.ctl
                    Layout.alignment: Qt.AlignVCenter
                    ToggleSwitch {
                        objectName: "settingToggle"
                        visible: typeof srow.modelData.value === "boolean"
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        checked: srow.modelData.value === true
                        onToggled: { root.client.setSetting(srow.modelData.key, !srow.modelData.value); root.search(); }
                    }
                    Input {
                        objectName: "settingField"
                        ts: root.theme.ts
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
