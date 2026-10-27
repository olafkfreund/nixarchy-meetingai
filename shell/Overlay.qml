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
    // Number on an accent fill: the theme's background/text or white/black, whichever contrasts most (WCAG luminance).
    function lum(c) {
        var f = function(x) { return x <= 0.03928 ? x / 12.92 : Math.pow((x + 0.055) / 1.055, 2.4); };
        return 0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b);
    }
    function ratio(a, b) { var x = lum(a), y = lum(b); return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05); }
    readonly property color onAccent: [Commons.Color.popups.background, Commons.Color.popups.text, "#ffffff", "#000000"].reduce(function(best, c) { return root.ratio(c, Commons.Color.accent) > root.ratio(best, Commons.Color.accent) ? c : best; })
    property var providers: []
    property var models: []
    property bool noContext: false
    property real now: Date.now()
    property bool transcriptOpen: true
    readonly property string status: daemon.answering ? "Writing" : daemon.now.body !== "" ? "Ready" : "Listening"

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
        daemon.ask([text].concat(inline).join("\n\n"), files, false, root.noContext);
    }

    function pickSuggestion(n) {
        var t = daemon.suggestions[n - 1];
        if (t !== undefined) root.submit(t, []);
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

        // While recording the window only takes input over the card; clicks elsewhere reach the apps below.
        mask: Region { item: daemon.recording ? card : full }
        Item { id: full; anchors.fill: parent }
        Rectangle { visible: !daemon.recording; anchors.fill: parent; color: Commons.Color.menu.scrim }
        MouseArea { enabled: !daemon.recording; anchors.fill: parent; onClicked: root.close() }

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
            Keys.onPressed: function(e) {   // 1/2/3 pick a suggestion only when nothing is being typed
                if (!root.library && composer.empty && e.key >= Qt.Key_1 && e.key <= Qt.Key_3 && !(e.modifiers & ~Qt.KeypadModifier)) {
                    root.pickSuggestion(e.key - Qt.Key_0);
                    e.accepted = true;
                }
            }

            MouseArea { anchors.fill: parent }

            Column {
                x: Style.spacing.lg; y: Style.spacing.lg
                width: parent.width - Style.spacing.lg * 2
                height: parent.height - Style.spacing.lg * 2
                spacing: Style.spacing.md

                // Header: record, timer, levels (click to mute), mode and Settings.
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
                        spacing: Style.spacing.lg
                        visible: daemon.recording
                        Repeater {
                            model: [{ label: "Mic", level: daemon.micLevel, on: daemon.state.mic !== false, cmd: "record.toggle_mic" }, { label: "System", level: daemon.systemLevel, on: daemon.state.system !== false, cmd: "record.toggle_system" }]
                            Item {
                                id: lvl
                                required property var modelData
                                width: lvlRow.width; height: lvlRow.height
                                    Row {
                                        id: lvlRow
                                        spacing: Style.spacing.xxs
                                    Rectangle {   // dot brightens with the level; dim when muted
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: Style.space(10); height: width; radius: width / 2
                                        color: Commons.Color.accent
                                        opacity: lvl.modelData.on ? 0.25 + 0.75 * lvl.modelData.level : 0.15
                                    }
                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        textFormat: Text.PlainText
                                        text: lvl.modelData.label
                                        color: Commons.Color.popups.text
                                        font.pixelSize: Style.font.caption
                                font.strikeout: !lvl.modelData.on
                                }
                                }
                                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: daemon.send(lvl.modelData.cmd, {}) }
                            }
                        }
                    }
                    Row {
                        spacing: Style.spacing.xs
                        Button { text: "General"; active: daemon.settings.meeting_mode !== "interview"; onClicked: daemon.setSetting("meeting_mode", "general") }
                        Button { text: "Interview"; active: daemon.settings.meeting_mode === "interview"; onClicked: daemon.setSetting("meeting_mode", "interview") }
                    }
                    Button { text: "⚙"; tooltipText: "Settings"; active: root.library; onClicked: { root.library = !root.library; lib.tab = "settings"; } }
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

                // Conversation: compact transcript (bottom-anchored, collapsible), the
                // pinned NOW panel, ASK NEXT, and the composer.
                Item {
                    id: convo
                    visible: !root.library
                    width: parent.width
                    height: parent.height - y

                    Composer {
                        id: composer
                        anchors.bottom: parent.bottom
                        width: parent.width
                        onSubmit: function(text, chips) { root.submit(text, chips); }
                        onPreview: function(text) { root.previewText = text; }
                        onScreenshot: daemon.send("screenshot.capture", { region: true })
                        onQuick: root.submit("What should I say?", [])
                        onPick: function(n) { root.pickSuggestion(n); }
                    }

                    Column {   // ASK NEXT: up to 3 rows, each with a [1]/[2]/[3] badge
                        id: sugg
                        anchors.bottom: composer.top
                        anchors.bottomMargin: Style.spacing.md
                        width: parent.width
                        Text {
                            visible: daemon.suggestions.length > 0
                            textFormat: Text.PlainText
                            text: "ASK NEXT"
                            color: Commons.Color.popups.text
                            font.pixelSize: Style.font.caption
                            font.bold: true
                        }
                        Repeater {
                            model: daemon.suggestions
                            Rectangle {
                                id: srow
                                required property string modelData
                                required property int index
                                width: sugg.width
                                height: Math.max(Style.spacing.controlHeight, stext.implicitHeight + Style.spacing.md * 2)
                                radius: root.round
                                color: sma.containsMouse ? Style.controlFill(false, true, Commons.Color.foreground, Commons.Color.accent) : "transparent"
                                Rectangle {
                                    id: badge
                                    x: Style.spacing.md; anchors.verticalCenter: parent.verticalCenter
                                    width: Style.space(20); height: width; radius: width / 2
                                    color: Commons.Color.accent
                                    Text {
                                        anchors.centerIn: parent
                                        textFormat: Text.PlainText
                                        text: String(srow.index + 1)
                                        color: root.onAccent
                                        font.pixelSize: Style.font.body
                                        font.bold: true
                                    }
                                }
                                Text {
                                    id: stext
                                    anchors.left: badge.right; anchors.leftMargin: Style.spacing.lg
                                    anchors.right: parent.right; anchors.rightMargin: Style.spacing.md
                                    anchors.verticalCenter: parent.verticalCenter
                                    textFormat: Text.PlainText
                                    wrapMode: Text.Wrap
                                    maximumLineCount: 2
                                    elide: Text.ElideRight
                                    text: srow.modelData
                                    color: Commons.Color.popups.text
                                    font.pixelSize: Style.font.subtitle
                                }
                                MouseArea { id: sma; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.pickSuggestion(srow.index + 1) }
                            }
                        }
                    }

                    // NOW: who asked, the question, status, and the answer larger than the transcript.
                    readonly property real below: composer.height + sugg.height + Style.spacing.md * 2
                    readonly property real avail: height - below - tbar.height
                    Rectangle {
                        id: nowPanel
                        anchors.bottom: sugg.top
                        anchors.bottomMargin: Style.spacing.md
                        width: parent.width
                        height: root.transcriptOpen ? Math.min(nowWant, convo.avail * 0.55) : Math.max(nowWant, convo.avail)
                        readonly property real nowWant: nowHead.height + (nowQ.visible ? nowQ.implicitHeight : 0) + answerText.implicitHeight + nowCol.spacing * 2 + Style.spacing.md * 2
                        radius: root.round
                        color: Qt.alpha(Commons.Color.accent, 0.12)
                        Rectangle { width: Style.space(4); height: parent.height - root.round; anchors.verticalCenter: parent.verticalCenter; x: Style.spacing.xs; radius: width / 2; color: Commons.Color.accent }
                        Column {
                            id: nowCol
                            x: Style.spacing.lg; y: Style.spacing.md
                            width: parent.width - Style.spacing.lg - Style.spacing.md
                            spacing: Style.spacing.xs
                            Row {
                                id: nowHead
                                spacing: Style.spacing.md
                                Text { textFormat: Text.PlainText; text: "NOW"; color: Commons.Color.popups.text; font.pixelSize: Style.font.caption; font.bold: true }
                                Text { textFormat: Text.PlainText; text: root.status; color: Commons.Color.popups.text; font.pixelSize: Style.font.caption }
                                Text {
                                    textFormat: Text.PlainText
                                    visible: text !== ""
                                    text: (daemon.now.provider + " " + daemon.now.model).trim()
                                    color: Commons.Color.popups.text
                                    font.pixelSize: Style.font.caption
                                }
                            }
                            Text {
                                id: nowQ
                                width: parent.width
                                visible: daemon.now.question !== ""
                                textFormat: Text.PlainText
                                wrapMode: Text.Wrap
                                maximumLineCount: 2
                                elide: Text.ElideRight
                                text: daemon.now.asker + " asked: " + daemon.now.question
                                color: Commons.Color.popups.text
                                font.pixelSize: Style.font.body
                                font.bold: true
                            }
                            Flickable {
                                width: parent.width
                                height: Math.max(0, Math.min(answerText.implicitHeight, nowPanel.height - nowHead.height - (nowQ.visible ? nowQ.implicitHeight : 0) - nowCol.spacing * 2 - Style.spacing.md * 2))
                                contentHeight: answerText.implicitHeight
                                clip: true
                                onContentHeightChanged: contentY = Math.max(0, contentHeight - height)
                                Text {
                                    id: answerText
                                    width: parent.width
                                    textFormat: Text.MarkdownText
                                    wrapMode: Text.Wrap
                                    text: root.themedLinks(daemon.now.body)
                                    color: Commons.Color.popups.text
                                    linkColor: Commons.Color.accent
                                    font.pixelSize: Style.font.subtitle
                                    onLinkActivated: function(link) { Qt.openUrlExternally(link); }
                                }
                            }
                        }
                    }

                    Row {   // transcript toggle
                        id: tbar
                        anchors.top: parent.top
                        Button { text: (root.transcriptOpen ? "▾ " : "▸ ") + "Transcript"; onClicked: root.transcriptOpen = !root.transcriptOpen }
                    }

                    // Transcript and archived answers in one virtualized list (capped at 500 rows by Client).
                    Item {
                        visible: root.transcriptOpen
                        anchors { top: tbar.bottom; left: parent.left; right: parent.right; bottom: nowPanel.top; bottomMargin: Style.spacing.md }
                        ListView {
                            id: tlist
                            anchors { bottom: parent.bottom; left: parent.left; right: parent.right }
                            height: Math.min(contentHeight, parent.height)
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
                                readonly property bool mine: who === "mic" || who === "asked"
                                readonly property bool bubble: who === "mic" || who === "system" || who === "asked"
                                readonly property string label: who === "asked" ? "You · Asked" : who === "mic" ? "You · Spoke" : (speaker !== "" ? speaker : "Them")
                                width: ListView.view.width
                                height: answer ? card2.height : bubble ? bub.height : err.implicitHeight
                                Rectangle {   // flat, borderless bubble
                                    id: bub
                                    visible: row.bubble
                                    x: row.mine ? row.width - width : 0
                                    width: Math.min(row.width * 0.8, Math.max(btxt.implicitWidth, blabel.implicitWidth) + Style.spacing.lg * 2)
                                    height: bcol.implicitHeight + Style.spacing.md * 2
                                    radius: root.round
                                    color: row.mine ? Qt.alpha(Commons.Color.accent, 0.22) : Qt.alpha(Commons.Color.foreground, 0.10)
                                    Column {
                                        id: bcol
                                        x: Style.spacing.lg; y: Style.spacing.md
                                        width: parent.width - Style.spacing.lg * 2
                                        Text { id: blabel; textFormat: Text.PlainText; text: row.label; color: Commons.Color.popups.text; font.pixelSize: Style.font.caption; font.bold: true }
                                        Text {
                                            id: btxt
                                            width: parent.width
                                            textFormat: Text.PlainText
                                            wrapMode: Text.Wrap
                                            text: row.body
                                            color: Commons.Color.popups.text
                                            font.italic: row.partial
                                            font.pixelSize: Style.font.bodySmall
                                        }
                                    }
                                }
                                Rectangle {   // an earlier answer: accent stripe, label, markdown
                                    id: card2
                                    visible: row.answer
                                    width: parent.width
                                    height: ansCol.implicitHeight + Style.spacing.md * 2
                                    radius: root.round
                                    color: Qt.alpha(Commons.Color.accent, 0.12)
                                    Rectangle { width: Style.space(4); height: parent.height - root.round; anchors.verticalCenter: parent.verticalCenter; x: Style.spacing.xs; radius: width / 2; color: Commons.Color.accent }
                                    Column {
                                        id: ansCol
                                        x: Style.spacing.lg; y: Style.spacing.md
                                        width: parent.width - Style.spacing.lg - Style.spacing.md
                                        spacing: Style.spacing.xs
                                        Text { textFormat: Text.PlainText; text: "Suggested answer"; color: Commons.Color.popups.text; font.pixelSize: Style.font.caption; font.bold: true }
                                        Text {
                                            width: parent.width
                                            textFormat: Text.MarkdownText
                                            wrapMode: Text.Wrap
                                            text: root.themedLinks(row.body)
                                            color: Commons.Color.popups.text
                                            linkColor: Commons.Color.accent
                                            font.pixelSize: Style.font.bodySmall
                                            onLinkActivated: function(link) { Qt.openUrlExternally(link); }
                                        }
                                    }
                                }
                                Text {
                                    id: err
                                    visible: !row.answer && !row.bubble
                                    width: parent.width
                                    textFormat: Text.PlainText
                                    wrapMode: Text.Wrap
                                    text: row.body
                                    color: row.who === "error" ? Commons.Color.urgent : Commons.Color.popups.text
                                    font.pixelSize: Style.font.bodySmall
                                }
                            }
                        }
                    }
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
