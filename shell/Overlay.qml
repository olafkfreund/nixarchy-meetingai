pragma ComponentBehavior: Bound
// qmllint disable missing-property
import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Wayland
import qs.Commons // qmllint disable import
import qs.Commons as Commons // qmllint disable import

// Meeting AI overlay: transcript, streamed answer, follow-ups, selectors and
// the question box. `omarchy-shell meetingai toggle` opens it.
Item {
    id: root

    property var shell: null
    property var manifest: null
    property bool opened: false
    property bool library: false
    property string previewText: ""
    property var providers: []
    property var models: []
    property var asked: null   // last sent question: { text, chips }

    readonly property string pluginId: String((root.manifest && root.manifest.id) || "nixarchy.meetingai")
    readonly property var paces: ["Fast", "Balanced", "Relaxed"]

    Client { id: daemon }

    function open(payloadJson) {
        root.opened = true;
        daemon.send("providers.list", {}, function(r) { if (r.ok) root.providers = r.data || []; });
        root.loadModels();
    }

    function close() {
        root.opened = false;
        if (root.shell && typeof root.shell.hide === "function") root.shell.hide(root.pluginId);
    }

    function loadModels() {
        var p = daemon.settings.chat_provider;
        if (!p) return;
        daemon.send("models.list", { provider: p }, function(r) { if (r.ok) root.models = r.data || []; });
    }

    function submit(text, chips) {
        var files = chips.filter(function(c) { return c.kind === "file"; }).map(function(c) { return c.path; });
        var inline = chips.filter(function(c) { return c.kind === "text"; }).map(function(c) { return c.text; });
        root.asked = { text: text, chips: chips };
        daemon.ask([text].concat(inline).join("\n\n"), files, false, noContext.checked);
    }

    function openRef(chip) {
        if (chip.kind === "text") root.previewText = chip.text;
        else daemon.send("attachment.preview", { path: chip.path }, function(r) { root.previewText = r.ok ? String((r.data && r.data.text !== undefined) ? r.data.text : r.data) : r.error; });
    }

    Connections {
        target: daemon
        function onConnectedChanged() { if (daemon.connected) root.loadModels(); }
    }

    PanelWindow { // qmllint disable uncreatable-type
        visible: root.opened
        anchors { top: true; bottom: true; left: true; right: true }
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.namespace: "nixarchy-meetingai"
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand

        Rectangle { anchors.fill: parent; color: Commons.Color.menu.scrim }
        MouseArea { anchors.fill: parent; onClicked: root.close() }

        Rectangle {
            id: card
            anchors.centerIn: parent
            width: Math.min(parent.width - Style.space(80), Style.space(900))
            height: Math.min(parent.height - Style.space(80), Style.space(760))
            radius: Style.cornerRadius
            color: Commons.Color.menu.background
            border.color: Commons.Color.menu.border
            focus: true
            Keys.onEscapePressed: root.close()

            MouseArea { anchors.fill: parent }

            Column {
                x: Style.spacing.lg; y: Style.spacing.lg
                width: parent.width - Style.spacing.lg * 2
                height: parent.height - Style.spacing.lg * 2
                spacing: Style.spacing.md

                // Header: recording state and selectors.
                Row {
                    spacing: Style.spacing.xs
                    Pill { label: daemon.recording ? "Stop" : "Record"; on: daemon.recording; onClicked: daemon.send(daemon.recording ? "record.stop" : "record.start", {}) }
                    Pill { label: "Mic"; on: daemon.state.mic === true; onClicked: daemon.send("record.toggle_mic", {}) }
                    Pill { label: "System"; on: daemon.state.system === true; onClicked: daemon.send("record.toggle_system", {}) }
                    Pill { label: "Screenshot"; onClicked: daemon.send("screenshot.capture", { region: true }) }
                    Pill { label: "General"; on: daemon.settings.meeting_mode !== "interview"; onClicked: daemon.setSetting("meeting_mode", "general") }
                    Pill { label: "Interview"; on: daemon.settings.meeting_mode === "interview"; onClicked: daemon.setSetting("meeting_mode", "interview") }
                    Pill { label: "Library"; on: root.library; onClicked: root.library = !root.library }
                    Pill { label: "Close"; onClicked: root.close() }
                }
                Row {
                    spacing: Style.spacing.xs
                    ComboBox {
                        model: root.providers.map(function(p) { return p.name || p.id; })
                        currentIndex: root.providers.findIndex(function(p) { return p.id === daemon.settings.chat_provider; })
                        onActivated: function(i) { daemon.setSetting("chat_provider", root.providers[i].id); root.loadModels(); }
                    }
                    ComboBox {
                        id: modelBox
                        editable: true   // typing filters the list
                        model: root.models.filter(function(m) { return editText === "" || String(m).toLowerCase().indexOf(editText.toLowerCase()) >= 0; })
                        editText: String(daemon.settings.chat_model || "")
                        onAccepted: daemon.setSetting("chat_model", editText)
                        onActivated: daemon.setSetting("chat_model", currentText)
                    }
                    ComboBox {
                        model: root.paces
                        currentIndex: Math.max(0, root.paces.indexOf(daemon.settings.response_pace))
                        onActivated: function(i) { daemon.setSetting("response_pace", root.paces[i]); }
                    }
                    CheckBox {
                        id: noContext
                        text: "No context"
                        contentItem: Text { text: noContext.text; leftPadding: noContext.indicator.width + 4; color: Commons.Color.menu.text; font.pixelSize: Style.font.caption; verticalAlignment: Text.AlignVCenter }
                    }
                }

                Text {
                    visible: !daemon.connected || daemon.lastError !== ""
                    width: parent.width
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    text: daemon.connected ? daemon.lastError : "The nixarchy-meetingai daemon is not running."
                    color: Commons.Color.urgent
                    font.pixelSize: Style.font.caption
                }

                Library {
                    visible: root.library
                    client: daemon
                    width: parent.width
                    height: parent.height - y
                }

                // Transcript: mic and system are visually distinct.
                ListView {
                    id: tlist
                    visible: !root.library
                    width: parent.width
                    height: (parent.height - y) * 0.4
                    clip: true
                    model: daemon.transcript
                    onCountChanged: positionViewAtEnd()
                    delegate: Text {
                        required property string who
                        required property string speaker
                        required property string body
                        required property bool partial
                        width: ListView.view.width
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        horizontalAlignment: who === "mic" ? Text.AlignRight : Text.AlignLeft
                        text: (speaker !== "" ? speaker + ": " : (who === "mic" ? "You: " : "")) + body
                        color: who === "mic" ? Commons.Color.accent : Commons.Color.menu.text
                        opacity: partial ? 0.55 : 1
                        font.italic: partial
                        font.pixelSize: Style.font.body
                    }
                }

                // The sent question; its attachments reopen a preview.
                Row {
                    visible: !root.library && root.asked !== null
                    spacing: Style.spacing.xs
                    Text {
                        textFormat: Text.PlainText
                        text: root.asked ? "Q: " + root.asked.text : ""
                        color: Commons.Color.menu.text
                        font.pixelSize: Style.font.caption
                        elide: Text.ElideRight
                        width: Math.min(implicitWidth, Style.space(500))
                    }
                    Repeater {
                        model: root.asked ? root.asked.chips : []
                        Pill { required property var modelData; label: modelData.label; onClicked: root.openRef(modelData) }
                    }
                }

                ScrollView {
                    visible: !root.library
                    width: parent.width
                    height: (parent.height - y) - composer.height - sugg.height - Style.spacing.md * 2
                    Text {
                        width: card.width - Style.spacing.lg * 2
                        textFormat: Text.MarkdownText
                        wrapMode: Text.Wrap
                        text: daemon.answerError !== "" ? daemon.answerError : daemon.answer
                        color: daemon.answerError !== "" ? Commons.Color.urgent : Commons.Color.menu.text
                        font.pixelSize: Style.font.body
                    }
                }

                Flow {
                    id: sugg
                    visible: !root.library
                    width: parent.width
                    spacing: Style.spacing.xs
                    Repeater {
                        model: daemon.suggestions
                        Pill { required property string modelData; label: modelData; onClicked: root.submit(modelData, []) }
                    }
                }

                Composer {
                    id: composer
                    visible: !root.library
                    width: parent.width
                    onSubmit: function(text, chips) { root.submit(text, chips); }
                    onPreview: function(text) { root.previewText = text; }
                }
            }

            // Scrollable preview of a chip or a sent reference.
            Rectangle {
                visible: root.previewText !== ""
                anchors.fill: parent
                anchors.margins: Style.space(40)
                color: Commons.Color.menu.background
                border.color: Commons.Color.menu.border
                radius: Style.cornerRadius
                ScrollView {
                    anchors.fill: parent
                    anchors.margins: Style.spacing.md
                    Text {
                        width: card.width - Style.space(120)
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        text: root.previewText
                        color: Commons.Color.menu.text
                        font.pixelSize: Style.font.body
                    }
                }
                Pill { anchors.top: parent.top; anchors.right: parent.right; label: "Close"; onClicked: root.previewText = "" }
            }
        }
    }
}
