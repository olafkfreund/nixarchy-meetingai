pragma ComponentBehavior: Bound
// qmllint disable missing-property
import QtQuick
import qs.Commons // qmllint disable import
import qs.Commons as Commons // qmllint disable import
import qs.Ui // qmllint disable import

// Meeting AI bar indicator: a dot and timer while recording. Click opens a
// small menu (New / Restore / Quit); Quit asks for confirmation inline.
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

    // Popup menu below the widget; a plain Rectangle keeps this free of the host's popup plumbing.
    Rectangle {
        visible: root.menuOpen
        y: root.height + Style.spacing.xs
        width: Style.space(180)
        height: col.implicitHeight + Style.spacing.md * 2
        radius: Style.cornerRadius
        color: Commons.Color.menu.background
        border.color: Commons.Color.menu.border
        z: 10

        Column {
            id: col
            x: Style.spacing.md; y: Style.spacing.md
            width: parent.width - Style.spacing.md * 2
            spacing: Style.spacing.xs

            Repeater {
                model: root.confirmQuit
                    ? [{ label: "Quit the daemon?", cmd: "" }, { label: "Yes, quit", cmd: "quit" }, { label: "Cancel", cmd: "cancel" }]
                    : [{ label: "New", cmd: "session.new" }, { label: "Restore", cmd: "restore" }, { label: "Quit", cmd: "ask-quit" }]

                Text {
                    required property var modelData
                    width: col.width
                    textFormat: Text.PlainText
                    text: modelData.label
                    color: modelData.cmd === "" ? Commons.Color.menu.text : (area.containsMouse ? Commons.Color.menu.selectedText : Commons.Color.menu.text)
                    opacity: modelData.cmd === "" ? 0.6 : 1
                    font.pixelSize: Style.font.body
                    MouseArea {
                        id: area
                        anchors.fill: parent
                        hoverEnabled: true
                        enabled: parent.modelData.cmd !== ""
                        onClicked: {
                            var c = parent.modelData.cmd;
                            if (c === "ask-quit") root.confirmQuit = true;
                            else if (c === "cancel") { root.confirmQuit = false; root.menuOpen = false; }
                            else root.choose(c);
                        }
                    }
                }
            }
        }
    }
}
