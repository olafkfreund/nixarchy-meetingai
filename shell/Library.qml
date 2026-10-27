pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects (Style.spacing, Color.popups) the linter cannot type
import QtQuick
import QtQuick.Controls
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// Restore / Profiles / Prompts / Providers / Settings. One search field asks
// the daemon (settings.search) and also filters the lists on the other tabs.
Item {
    id: root

    required property var client
    readonly property real ts: client.textScale
    property string tab: "restore"
    property string query: ""
    property var sessions: []
    property var profiles: []
    property var providers: []
    readonly property var labelOverrides: ({ "mcp_allow_record": "Let AI agents start recording" })   // the daemon derives labels from keys
    property var found: []        // settings.search results: { key, label, value }
    property string promptMode: "general"
    property string notes: ""
    property string modelsText: ""
    property Component settingsHeader: null   // controls shown above the Settings list

    readonly property var tabs: ["restore", "profiles", "prompts", "providers", "settings"]
    readonly property var promptKinds: ["answer", "follow_up", "recap"]
    property string promptKind: "answer"

    function match(s) { return root.query === "" || String(s).toLowerCase().indexOf(root.query.toLowerCase()) >= 0; }

    function load() {
        root.client.send("session.list", {}, function(r) { if (r.ok) root.sessions = r.data || []; });
        root.client.send("profiles.list", {}, function(r) { if (r.ok) root.profiles = r.data || []; });
        root.client.send("providers.list", {}, function(r) { if (r.ok) root.providers = r.data || []; });
        root.search();
        root.loadPrompt();
    }

    function search() {
        root.client.send("settings.search", { query: root.query }, function(r) { if (r.ok) root.found = r.data || []; });
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
                IconButton { ts: root.ts;
                    required property string modelData
                    text: modelData.charAt(0).toUpperCase() + modelData.slice(1)
                    active: root.tab === modelData
                    onClicked: root.tab = modelData
                }
            }
        }

        PanelSeparator {}

        TextField {
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
            delegate: Row {
                required property var modelData
                width: ListView.view.width
                spacing: Style.spacing.xs
                Text {
                    width: parent.width - Style.space(200)
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    text: (parent.modelData.kind === "meeting" ? "Meeting · " : "Chat · ") + parent.modelData.title
                    color: Commons.Color.popups.text
                    font.pixelSize: Style.font.body * root.ts
                }
                IconButton { ts: root.ts; text: "Restore"; onClicked: root.client.send("session.restore", { id: parent.modelData.id }) }
                IconButton { ts: root.ts;
                    text: "Notes"
                    visible: parent.modelData.kind === "meeting"
                    onClicked: root.client.send("notes.get", { meeting: parent.modelData.id }, function(r) { root.notes = r.ok ? String((r.data && r.data.text !== undefined) ? r.data.text : r.data) : r.error; })
                }
                IconButton { ts: root.ts; text: "Delete"; onClicked: { root.client.send("session.delete", { id: parent.modelData.id }); root.load(); } }
            }
        }
        Text {
            id: notesBox
            visible: root.tab === "restore" && root.notes !== ""
            width: parent.width
            height: visible ? Style.space(160) : 0
            textFormat: Text.MarkdownText
            wrapMode: Text.Wrap
            clip: true
            text: root.notes
            color: Commons.Color.popups.text
            font.pixelSize: Style.font.body * root.ts
        }

        ListView {
            visible: root.tab === "profiles"
            width: parent.width; height: parent.height - y
            clip: true
            model: root.profiles.filter(function(p) { return root.match(p.name); })
            delegate: Row {
                required property var modelData
                width: ListView.view.width
                spacing: Style.spacing.xs
                Text {
                    width: parent.width - Style.space(100)
                    textFormat: Text.PlainText
                    text: parent.modelData.name
                    color: Commons.Color.popups.text
                    font.pixelSize: Style.font.body * root.ts
                }
                IconButton { ts: root.ts; text: "Delete"; onClicked: { root.client.send("profiles.delete", { name: parent.modelData.name }); root.load(); } }
            }
            footer: Row {
                spacing: Style.spacing.xs
                TextField { id: profileName; placeholderText: "Profile name" }
                IconButton { ts: root.ts; text: "Save current"; onClicked: { root.client.send("profiles.save", { name: profileName.text }); root.load(); } }
            }
        }

        Column {
            visible: root.tab === "prompts"
            width: parent.width; height: parent.height - y
            spacing: Style.spacing.xs
            Row {
                spacing: Style.spacing.xs
                Repeater {
                    model: ["general", "interview"]
                    IconButton { ts: root.ts; required property string modelData; text: modelData; active: root.promptMode === modelData; onClicked: root.promptMode = modelData }
                }
                Repeater {
                    model: root.promptKinds
                    IconButton { ts: root.ts; required property string modelData; text: modelData; active: root.promptKind === modelData; onClicked: root.promptKind = modelData }
                }
            }
            ScrollView {
                width: parent.width; height: parent.height - Style.space(80)
                TextArea {
                    id: promptText
                    wrapMode: TextEdit.Wrap
                    color: Commons.Color.popups.text
                    font.pixelSize: Style.font.body * root.ts
                    background: BorderSurface {
                        color: Style.controlFill(promptText.activeFocus, promptText.hovered, Commons.Color.foreground, Commons.Color.accent)
                        borderSpec: Border.controlSpec(promptText.activeFocus ? "focus" : (promptText.hovered ? "hover-cursor" : "normal"), Commons.Color.foreground, Commons.Color.accent)
                        radius: Style.cornerRadius
                    }
                }
            }
            IconButton { ts: root.ts; text: "Save"; onClicked: root.client.send("prompts.set", { mode: root.promptMode, kind: root.promptKind, text: promptText.text }) }
        }

        ListView {
            visible: root.tab === "providers"
            width: parent.width; height: parent.height - y
            clip: true
            model: root.providers.filter(function(p) { return root.match(p.name || p.id); })
            delegate: Row {
                id: prow
                required property var modelData
                width: ListView.view.width
                spacing: Style.spacing.xs
                Text {
                    width: Style.space(220)
                    textFormat: Text.PlainText
                    text: (prow.modelData.name || prow.modelData.id) + (prow.modelData.requires_key === false ? "" : (prow.modelData.has_key ? " · key set" : " · no key"))
                    color: Commons.Color.popups.text
                    font.pixelSize: Style.font.body * root.ts
                }
                TextField {
                    id: keyField
                    visible: prow.modelData.requires_key !== false
                    echoMode: TextInput.Password
                    placeholderText: "API key"
                }
                IconButton { ts: root.ts;
                    text: "Set key"
                    visible: keyField.visible
                    enabled: keyField.text !== ""
                    opacity: enabled ? 1 : 0.5
                    onClicked: { root.client.send("settings.set", { key: "api_key", provider: prow.modelData.id, value: keyField.text }); keyField.text = ""; root.load(); }
                }
                IconButton { ts: root.ts;
                    text: "Models"
                    onClicked: root.client.send("models.list", { provider: prow.modelData.id }, function(r) { root.modelsText = r.ok ? (r.data || []).join(", ") : r.error; })
                }
            }
            footer: Text { text: root.modelsText; width: parent.width; wrapMode: Text.Wrap; textFormat: Text.PlainText; color: Commons.Color.popups.text; font.pixelSize: Style.font.caption * root.ts }
        }

        ListView {
            visible: root.tab === "settings"
            width: parent.width; height: parent.height - y
            clip: true
            model: root.found
            header: root.settingsHeader
            delegate: Item {   // label left, control right, both centred on the row
                id: srow
                required property var modelData
                width: ListView.view.width
                height: Math.max(label.implicitHeight, ctl.height) + Style.spacing.sm
                Text {
                    id: label
                    anchors.left: parent.left
                    anchors.right: ctl.left
                    anchors.rightMargin: Style.spacing.md
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    text: root.labelOverrides[srow.modelData.key] || srow.modelData.label || srow.modelData.key
                    color: Commons.Color.popups.text
                    font.pixelSize: Style.font.body * root.ts
                }
                Item {
                    id: ctl
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: Style.space(220)
                    height: Style.spacing.controlHeight
                    ToggleSwitch {
                        visible: typeof srow.modelData.value === "boolean"
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        checked: srow.modelData.value === true
                        onToggled: { root.client.setSetting(srow.modelData.key, !srow.modelData.value); root.search(); }
                    }
                    TextField {
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
