pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects (Style.spacing, Color.popups) the linter cannot type
import QtQuick
import QtQuick.Layouts
import Quickshell.Io
import qs.Commons as Commons

// First run: what stands between the user and a working overlay, one next step each.
// The daemon says what is missing (state.ready / state.hints); a daemon that is down is step one.
Rectangle {
    id: root

    required property var daemon
    required property var theme

    signal openProviders()
    signal dismissed()

    readonly property var ready: root.daemon.state.ready || ({})
    readonly property var hints: root.daemon.state.hints || ({})
    readonly property bool down: !root.daemon.connected
    readonly property bool needChat: !root.down && root.ready.chat === false
    readonly property bool needSpeech: !root.down && root.ready.speech === false
    readonly property bool needed: root.down || root.needChat || root.needSpeech
    readonly property string startCmd: "systemctl --user start nixarchy-meetingai"
    readonly property string whisperCmd: "systemctl --user status nixarchy-meetingai-whisper"

    visible: root.needed
    height: col.implicitHeight + root.theme.padY * 2
    radius: root.theme.radius
    color: Qt.alpha(Commons.Color.accent, 0.12)
    Accessible.role: Accessible.Pane
    Accessible.name: "Set up Meeting AI"

    property bool startFailed: false
    Process {
        id: starter
        command: ["systemctl", "--user", "start", "nixarchy-meetingai"]
        onExited: function(code) { root.startFailed = code !== 0; }
    }

    component Title: UiText { textFormat: Text.PlainText; color: Commons.Color.popups.text; font.pixelSize: root.theme.body; font.bold: true }
    component Body: UiText { Layout.fillWidth: true; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: Commons.Color.popups.text; opacity: root.theme.dim; font.pixelSize: root.theme.caption }
    Column {
        id: col
        x: root.theme.padX; y: root.theme.padY
        width: parent.width - root.theme.padX * 2
        spacing: root.theme.gap

        RowLayout {
            width: parent.width
            Title { text: "Finish setting up"; Layout.fillWidth: true }
            IconButton { objectName: "setupDismiss"; ts: root.theme.ts; text: "Not now"; tooltipText: "Hide this until the next time the overlay opens"; onClicked: root.dismissed() }
        }
        ColumnLayout {   // 1. the service
            visible: root.down
            width: parent.width
            spacing: root.theme.gap / 2
            Title { text: "Start the service" }
            Body { text: "The Meeting AI daemon is not running. Start it, or run the command in a terminal." }
            CommandRow { theme: root.theme; cmd: root.startCmd }
            RowLayout { IconButton { objectName: "startService"; ts: root.theme.ts; text: "Start the service"; bordered: true; onClicked: { root.startFailed = false; starter.running = true; } } }
            Body { visible: root.startFailed; text: "Error: the service did not start. Run the command above in a terminal to see why." }
        }
        ColumnLayout {   // 2. a provider
            visible: root.needChat
            width: parent.width
            spacing: root.theme.gap / 2
            Title { text: "Add an AI provider" }
            Body { text: String(root.hints.chat || "No AI provider is set up.") }
            RowLayout { IconButton { objectName: "openProviders"; ts: root.theme.ts; text: "Open Providers"; bordered: true; onClicked: root.openProviders() } }
        }
        ColumnLayout {   // 3. speech
            visible: root.needSpeech
            width: parent.width
            spacing: root.theme.gap / 2
            Title { text: "Check speech-to-text" }
            Body { text: String(root.hints.speech || "Speech-to-text is not ready.") }
            CommandRow { theme: root.theme; cmd: root.whisperCmd }
        }
    }
}
