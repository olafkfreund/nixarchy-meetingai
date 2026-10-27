pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects (Style.spacing, Color.popups) the linter cannot type
import QtQuick
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// Meeting AI bar indicator: a dot and timer while recording. Click opens a
// small menu (New / Restore / Quit); Quit asks for confirmation.
BarWidget {
    id: root
    moduleName: "nixarchy.meetingai"

    property double now: Date.now()
    property bool menuOpen: false
    property bool confirmQuit: false

    readonly property string timer: {
        if (!client.recording || !client.state.since) return "";
        var s = Math.max(0, Math.floor((root.now - client.state.since) / 1000));
        var m = Math.floor(s / 60);
        return m + ":" + (s % 60 < 10 ? "0" : "") + (s % 60);
    }

    implicitWidth: row.implicitWidth
    implicitHeight: button.implicitHeight

    Client { id: client }

    Timer {
        interval: 1000; repeat: true; running: client.recording
        onTriggered: root.now = Date.now()
    }

    function close() { root.menuOpen = false; root.confirmQuit = false; }

    function run(cmd) { if (root.bar) root.bar.run(cmd); }

    function choose(cmd) {
        root.menuOpen = false;
        root.confirmQuit = false;
        if (cmd === "restore") root.run("omarchy-shell meetingai toggle");
        else client.send(cmd, {});
    }

    Row {
        id: row
        height: parent.height
        spacing: Style.spacing.xs

        BarIconButton {
            id: button
            bar: root.bar
            text: "\u{F0A2E}"   // nf-md-microphone_message
            active: client.recording
            dimmed: !client.connected
            tooltipText: !client.connected ? "Meeting AI: daemon not running" : (client.recording ? "Taking notes · " + root.timer : "Meeting AI")
            onPressed: function(b) {
                if (b === Qt.RightButton) root.run("omarchy-shell meetingai toggle");
                else { root.menuOpen = !root.menuOpen; root.confirmQuit = false; }
            }
        }

        Rectangle {
            visible: client.recording
            width: Style.space(8); height: width; radius: width / 2
            anchors.verticalCenter: parent.verticalCenter
            color: Commons.Color.urgent
        }
        Text {
            visible: client.recording
            textFormat: Text.PlainText
            anchors.verticalCenter: parent.verticalCenter
            text: root.timer
            color: Commons.Color.bar.text
            font.pixelSize: Style.font.caption
        }
    }

    PopupCard {
        id: popup
        anchorItem: root
        bar: root.bar
        owner: root
        open: root.menuOpen
        contentWidth: popup.fittedContentWidth(Style.space(180))
        contentHeight: popup.fittedContentHeight(root.confirmQuit ? Style.space(140) : col.implicitHeight)

        Column {
            id: col
            width: parent.width
            spacing: Style.spacing.xs

            Repeater {
                model: [{ label: "New", cmd: "session.new" }, { label: "Restore", cmd: "restore" }, { label: "Quit", cmd: "ask-quit" }]
                Button {
                    required property var modelData
                    width: col.width
                    leftAlign: true
                    text: modelData.label
                    onClicked: {
                        if (modelData.cmd === "ask-quit") root.confirmQuit = true;
                        else root.choose(modelData.cmd);
                    }
                }
            }
        }

        ConfirmDialog {
            anchors.fill: parent
            opened: root.confirmQuit
            z: 10
            message: "Quit the daemon?"
            confirmText: "Quit"
            background: Commons.Color.popups.background
            onCanceled: { root.confirmQuit = false; root.menuOpen = false; }
            onConfirmed: root.choose("quit")
        }
    }
}
