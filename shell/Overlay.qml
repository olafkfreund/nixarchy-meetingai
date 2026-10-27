pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects (Style.spacing, Color.popups) the linter cannot type
import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Wayland
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// Meeting AI overlay: transcript, streamed answer, follow-ups, selectors and
// the question box. `omarchy-shell meetingai toggle` opens it.
Item {
    id: root

    property var shell: null
    property var manifest: null
    property bool opened: false
    property bool library: false
    property string previewText: ""

    // MarkdownText ignores linkColor and paints links web-blue; inline HTML keeps the theme accent.
    function themedLinks(md) {
        var c = String(Commons.Color.accent);
        return String(md).replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, '<a href="$2"><span style="color:' + c + '">$1</span></a>');
    }
    property var providers: []
    property var models: []
    property bool noContext: false
    property real now: Date.now()
    property var asked: null   // last sent question: { text, chips }

    readonly property string pluginId: String((root.manifest && root.manifest.id) || "nixarchy.meetingai")
    // popups.background carries the theme alpha; the overlay must not show what is behind it.
    readonly property color solid: Qt.rgba(Commons.Color.popups.background.r, Commons.Color.popups.background.g, Commons.Color.popups.background.b, 1)
    readonly property string elapsed: {
        var t = Math.max(0, Math.floor((root.now - (daemon.state.since || root.now)) / 1000));
        var p = function(n) { return (n < 10 ? "0" : "") + n; };
        return Math.floor(t / 3600) + ":" + p(Math.floor(t / 60) % 60) + ":" + p(t % 60);
    }
    readonly property real round: Math.max(Style.cornerRadius, 12)   // Hyprland rounding is 6
    readonly property var paces: ["Fast", "Balanced", "Relaxed"]

    Client { id: daemon }
    Timer { running: root.opened && daemon.recording; interval: 1000; repeat: true; triggeredOnStart: true; onTriggered: root.now = Date.now() }

    function open(payloadJson) {
        root.opened = true;
        daemon.send("providers.list", {}, function(r) { if (r.ok) root.providers = r.data || []; });
        root.loadModels();
    }

    // Hooks for tests/shots.sh.
    function openModelPicker() { root.library = true; lib.tab = "settings"; }
    function showLibrary(tab) { root.library = true; lib.tab = tab; }

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
        daemon.ask([text].concat(inline).join("\n\n"), files, false, root.noContext);
    }

    function openRef(chip) {
        if (chip.kind === "text") root.previewText = chip.text;
        else daemon.send("attachment.preview", { path: chip.path }, function(r) { root.previewText = r.ok ? String((r.data && r.data.text !== undefined) ? r.data.text : r.data) : r.error; });
    }

    Connections {
        target: daemon
        function onConnectedChanged() { if (daemon.connected) root.loadModels(); }
        function onSettingsChanged() { if (root.models.length === 0) root.loadModels(); }
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
            radius: root.round
            color: root.solid
            border.color: Commons.Color.popups.border
            border.width: Style.normalBorderWidth
            focus: true
            Keys.onEscapePressed: root.close()

            MouseArea { anchors.fill: parent }

            Column {
                x: Style.spacing.lg; y: Style.spacing.lg
                width: parent.width - Style.spacing.lg * 2
                height: parent.height - Style.spacing.lg * 2
                spacing: Style.spacing.md

                // Header: record, timer, levels, mode and the Settings tab.
                Row {
                    width: parent.width
                    spacing: Style.spacing.md
                    Button { text: daemon.recording ? "Stop" : "Record"; active: daemon.recording; onClicked: daemon.send(daemon.recording ? "record.stop" : "record.start", {}) }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: daemon.recording
                        textFormat: Text.PlainText
                        text: root.elapsed
                        color: Commons.Color.popups.text
                        font.pixelSize: Style.font.body
                    }
                    Row {
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: Style.spacing.xs
                        visible: daemon.recording
                        Repeater {
                            model: [{ label: "Mic", level: daemon.micLevel }, { label: "System", level: daemon.systemLevel }]
                            Row {
                                id: lvl
                                required property var modelData
                                spacing: Style.spacing.xxs
                                Rectangle {   // dot brightens with the level
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: Style.space(10); height: width; radius: width / 2
                                    color: Commons.Color.accent
                                    opacity: 0.25 + 0.75 * lvl.modelData.level
                                }
                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    textFormat: Text.PlainText
                                    text: lvl.modelData.label
                                    color: Commons.Color.popups.text
                                    font.pixelSize: Style.font.caption
                                }
                            }
                        }
                    }
                    Row {
                        spacing: Style.spacing.xs
                        Button { text: "General"; active: daemon.settings.meeting_mode !== "interview"; onClicked: daemon.setSetting("meeting_mode", "general") }
                        Button { text: "Interview"; active: daemon.settings.meeting_mode === "interview"; onClicked: daemon.setSetting("meeting_mode", "interview") }
                    }
                    Button { text: "Settings"; active: root.library; onClicked: { root.library = !root.library; lib.tab = "settings"; } }
                }
                PanelSeparator {}

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
                    id: lib
                    settingsHeader: settingsControls
                    visible: root.library
                    client: daemon
                    width: parent.width
                    height: parent.height - y
                }

                // Transcript and answers in one virtualized list (capped at 500 rows
                // by Client): mic right and accent, system left, answers markdown.
                ListView {
                    id: tlist
                    visible: !root.library
                    width: parent.width
                    height: Math.max(0, parent.height - y - (askedRow.visible ? askedRow.height : 0) - sugg.height - composer.height - Style.spacing.md * 3)
                    clip: true
                    spacing: Style.spacing.sm
                    model: daemon.transcript
                    property bool follow: true
                    onContentHeightChanged: if (follow) Qt.callLater(positionViewAtEnd)
                    onMovementEnded: follow = atYEnd
                    delegate: Item {
                        id: row
                        required property string who
                        required property string speaker
                        required property string body
                        required property bool partial
                        readonly property bool answer: who === "answer"
                        width: ListView.view.width
                        height: answer ? card2.height : txt.implicitHeight
                        Rectangle {   // an answer is its own block: accent stripe, label, markdown
                            id: card2
                            visible: row.answer
                            width: parent.width
                            height: ansCol.implicitHeight + Style.spacing.md * 2
                            radius: root.round
                            color: Qt.alpha(Commons.Color.accent, 0.1)
                            Rectangle { width: Style.space(4); height: parent.height - root.round; anchors.verticalCenter: parent.verticalCenter; x: Style.spacing.xs; radius: width / 2; color: Commons.Color.accent }
                            Column {
                                id: ansCol
                                x: Style.spacing.lg; y: Style.spacing.md
                                width: parent.width - Style.spacing.lg - Style.spacing.md
                                spacing: Style.spacing.xs
                                Text {
                                    textFormat: Text.PlainText
                                    text: "Suggested answer"
                                    color: Commons.Color.accent
                                    font.pixelSize: Style.font.caption
                                    font.bold: true
                                }
                                Text {
                                    width: parent.width
                                    textFormat: Text.MarkdownText
                                    wrapMode: Text.Wrap
                                    text: root.themedLinks(row.body)
                                    color: Commons.Color.popups.text
                                    linkColor: Commons.Color.accent
                                    font.pixelSize: Style.font.body
                                    onLinkActivated: function(link) { Qt.openUrlExternally(link); }
                                }
                            }
                        }
                        Text {
                            id: txt
                            visible: !row.answer
                            width: parent.width
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                            horizontalAlignment: row.who === "mic" ? Text.AlignRight : Text.AlignLeft
                            text: row.who === "error" || row.who === "question" ? row.body
                                : (row.speaker !== "" ? row.speaker + ": " : (row.who === "mic" ? "You: " : "")) + row.body
                            color: row.who === "mic" ? Commons.Color.accent
                                : row.who === "error" ? Commons.Color.urgent
                                : row.who === "system" ? Qt.darker(Commons.Color.popups.text, 1.3) : Commons.Color.popups.text
                            opacity: row.partial ? 0.55 : 1
                            font.italic: row.partial
                            font.pixelSize: Style.font.body
                        }
                    }
                }

                // The sent question; its attachments reopen a preview.
                Row {
                    id: askedRow
                    visible: !root.library && root.asked !== null
                    spacing: Style.spacing.xs
                    Text {
                        textFormat: Text.PlainText
                        text: root.asked ? "Q: " + root.asked.text : ""
                        color: Commons.Color.popups.text
                        font.pixelSize: Style.font.caption
                        elide: Text.ElideRight
                        width: Math.min(implicitWidth, Style.space(500))
                    }
                    Repeater {
                        model: root.asked ? root.asked.chips : []
                        Button { required property var modelData; text: modelData.label; bordered: true; onClicked: root.openRef(modelData) }
                    }
                }

                Flow {
                    id: sugg
                    visible: !root.library
                    width: parent.width
                    spacing: Style.spacing.xs
                    Repeater {
                        model: daemon.suggestions
                        Button { required property string modelData; text: modelData; bordered: true; onClicked: root.submit(modelData, []) }
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

            Component {   // top of the Library's Settings tab
                id: settingsControls
                Flow {
                    width: parent ? parent.width : 0
                    spacing: Style.spacing.md
                    bottomPadding: Style.spacing.md
                    Dropdown {
                        label: "Provider"
                        value: String(daemon.settings.chat_provider || "")
                        options: root.providers.map(function(p) { return { value: p.id, label: p.name || p.id }; })
                        onChanged: function(v) { daemon.setSetting("chat_provider", v); daemon.setSetting("chat_model", ""); root.loadModels(); }
                    }
                    SearchableDropdown {   // typing filters the list
                        id: modelBox
                        label: "Model"
                        value: String(daemon.settings.chat_model || "")
                        options: root.models.map(function(m) { return String(m); })
                        triggerLabel: "default"
                        onChanged: function(v) { daemon.setSetting("chat_model", v); }
                    }
                    Item {
                        width: Style.space(200); height: modelBox.height
                        TextField {   // a model id the list does not have
                            id: customModel
                            anchors.bottom: parent.bottom
                            width: parent.width
                            placeholderText: "or type a model id"
                            onAccepted: { if (text.trim() !== "") daemon.setSetting("chat_model", text.trim()); text = ""; }
                        }
                    }
                    Dropdown {
                        label: "Pace"
                        value: String(daemon.settings.response_pace || "Balanced")
                        options: root.paces
                        onChanged: function(v) { daemon.setSetting("response_pace", v); }
                    }
                    Item {
                        width: ncRow.width; height: modelBox.height
                        Row {
                            id: ncRow
                            anchors.bottom: parent.bottom
                            height: Style.spacing.controlHeight
                            spacing: Style.spacing.xs
                            ToggleSwitch { id: noContext; anchors.verticalCenter: parent.verticalCenter; checked: root.noContext; onToggled: root.noContext = !root.noContext }
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                textFormat: Text.PlainText
                                text: "No context"
                                color: Commons.Color.popups.text
                                font.pixelSize: Style.font.caption
                            }
                        }
                    }
                    Row {
                        spacing: Style.spacing.xs
                        Button { text: "Mic"; active: daemon.state.mic === true; onClicked: daemon.send("record.toggle_mic", {}) }
                        Button { text: "System"; active: daemon.state.system === true; onClicked: daemon.send("record.toggle_system", {}) }
                        Button { text: "Screenshot"; onClicked: daemon.send("screenshot.capture", { region: true }) }
                    }
                }
            }

            // Scrollable preview of a chip or a sent reference.
            Rectangle {
                visible: root.previewText !== ""
                anchors.fill: parent
                anchors.margins: Style.space(40)
                color: root.solid
                border.color: Commons.Color.popups.border
                border.width: Style.normalBorderWidth
                radius: root.round
                ScrollView {
                    anchors.fill: parent
                    anchors.margins: Style.spacing.md
                    Text {
                        width: card.width - Style.space(120)
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        text: root.previewText
                        color: Commons.Color.popups.text
                        font.pixelSize: Style.font.body
                    }
                }
                Button { anchors.top: parent.top; anchors.right: parent.right; text: "Close"; onClicked: root.previewText = "" }
            }
        }
    }
}
