pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects (Style.spacing, Color.popups) the linter cannot type
import QtQuick
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// Meeting AI bar indicator: a dot and timer while recording. Click opens a
// small menu (New / Restore / Stop Meeting AI); stopping asks for confirmation.
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
    Rec { id: rec; background: Commons.Color.bar.background }
    readonly property bool finishing: !client.recording && client.state.finishing === true
    property real pulse: 1
    readonly property bool pulsing: client.recording && !Style.reduceMotion && !client.reduceMotion
    SequentialAnimation on pulse {
        loops: Animation.Infinite
        running: root.pulsing
        NumberAnimation { to: 0.6; duration: 900; easing.type: Easing.InOutSine }
        NumberAnimation { to: 1; duration: 900; easing.type: Easing.InOutSine }
    }

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
            tooltipText: !client.connected ? "Meeting AI: daemon not running" : (client.recording ? "Taking notes · " + root.timer : root.finishing ? "Meeting AI: finishing the notes" : "Meeting AI")
            onPressed: function(b) {
                if (b === Qt.RightButton) root.run("omarchy-shell meetingai toggle");
                else { root.menuOpen = !root.menuOpen; root.confirmQuit = false; }
            }
        }

        Rectangle {   // red and pulsing while recording; steady while the notes are finished
            id: dot
            visible: client.recording || root.finishing
            width: Style.space(11); height: width; radius: width / 2
            anchors.verticalCenter: parent.verticalCenter
            color: rec.red
            opacity: root.pulsing ? root.pulse : 1
        }
        Text {
            visible: client.recording
            textFormat: Text.PlainText
            anchors.verticalCenter: parent.verticalCenter
            text: root.timer
            color: Commons.Color.bar.text
            font.bold: true
            font.family: Style.font.family
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
                model: [{ label: "New", cmd: "session.new" }, { label: "Restore", cmd: "restore" }, { label: "Stop Meeting AI\u2026", cmd: "ask-quit" }]
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
            message: client.recording ? "Stop Meeting AI? The recording in progress is finished first and its notes are written." : "Stop Meeting AI? The daemon exits; nothing is recorded until it starts again."
            confirmText: "Stop"
            background: Commons.Color.popups.background
            onCanceled: { root.confirmQuit = false; root.menuOpen = false; }
            onConfirmed: root.choose("quit")
        }
    }
}
