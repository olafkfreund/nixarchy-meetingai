pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects (Style.spacing, Color.popups) the linter cannot type
import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Commons as Commons

// Overlay header: one quiet toolbar of icon buttons. Dragging it moves the card.
Item {
    id: root

    required property var daemon
    required property var theme
    required property bool library
    required property string profileName
    required property string elapsed

    signal grabBegan(real gx, real gy)
    signal grabMoved(real gx, real gy)
    signal grabEnded()
    signal resetRequested()
    signal settingsClicked()

    // Nerd Font Material Design glyphs (nf-md-*), monochrome
    readonly property string iRecord: String.fromCodePoint(0xf044b)
    readonly property string iStop: String.fromCodePoint(0xf04db)
    readonly property string iMic: String.fromCodePoint(0xf036c)
    readonly property string iMicOff: String.fromCodePoint(0xf036d)
    readonly property string iVolume: String.fromCodePoint(0xf057e)
    readonly property string iVolumeOff: String.fromCodePoint(0xf0581)
    readonly property int minutes: { var p = root.elapsed.split(":"); return Number(p[0]) * 60 + Number(p[1]); }
    readonly property string iCog: String.fromCodePoint(0xf0493)

    readonly property bool finishing: !root.daemon.recording && root.daemon.state.finishing === true
    readonly property bool live: root.daemon.recording || root.finishing
    property real pulse: 1
    Rec { id: rec }
    // a gentle breath; off with reduced motion (Hyprland animations off, or the setting) and while the overlay is hidden
    readonly property bool pulsing: root.daemon.recording && !Style.reduceMotion && !root.daemon.reduceMotion && root.Window.window !== null && root.Window.window.visible
    SequentialAnimation on pulse {
        loops: Animation.Infinite
        running: root.pulsing
        NumberAnimation { to: 0.6; duration: 900; easing.type: Easing.InOutSine }
        NumberAnimation { to: 1; duration: 900; easing.type: Easing.InOutSine }
    }
    function openPicker() { picker.open(); }
    height: Math.max(hdrLeft.height, hdrRight.height)   // the segmented box is taller than a button; both rows centre on it
    MouseArea {   // drag the header to move the card; double-click resets size and position
        anchors.fill: parent
        cursorShape: pressed ? Qt.ClosedHandCursor : Qt.ArrowCursor
        onPressed: function(m) { var g = mapToItem(null, m.x, m.y); root.grabBegan(g.x, g.y); }
        onPositionChanged: function(m) { if (pressed) { const g = mapToItem(null, m.x, m.y); root.grabMoved(g.x, g.y); } }
        onReleased: root.grabEnded()
        onDoubleClicked: root.resetRequested()
    }
    Row {
        id: hdrLeft
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.spacing.xs
        IconButton {   // record / stop
            id: recBtn
            objectName: "record"
            ts: root.theme.ts
            iconText: root.live ? root.iStop : root.iRecord   // the red dot beside it shows the state
            enabled: !root.finishing
            Accessible.name: root.finishing ? "Finishing the recording" : root.daemon.recording ? "Stop recording, " + root.minutes + " minutes" : "Start recording"
            tooltipText: root.finishing ? "Finishing: writing the notes" : root.daemon.recording ? "Stop recording" : "Start recording"
            onClicked: { if (root.daemon.recording) root.daemon.send("record.stop", {}); else root.daemon.startRecording(false); }
        }
        Rectangle {   // the recording dot: red, pulsing while recording, steady while finishing
            visible: root.live
            anchors.verticalCenter: parent.verticalCenter
            width: Math.round(Style.space(11) * root.theme.ts); height: width; radius: width / 2
            color: rec.red
            opacity: root.pulsing ? root.pulse : 1
        }
        UiText {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.live
            textFormat: Text.PlainText
            text: root.finishing ? "Finishing" : "REC " + root.elapsed
            color: Commons.Color.popups.text   // red text would need 4.5:1; the red stays on the dot
            font.bold: true
            font.pixelSize: root.theme.body
            leftPadding: Style.spacing.xs; rightPadding: Style.spacing.xs
        }
        // Brightness follows the live level; a struck-out glyph means muted.
        IconButton { objectName: "mic"; ts: root.theme.ts; visible: root.live; iconText: root.daemon.state.mic !== false ? root.iMic : root.iMicOff; opacity: root.daemon.state.mic !== false ? 0.4 + 0.6 * root.daemon.micLevel : 0.5; tooltipText: root.daemon.state.mic !== false ? "Mute microphone" : "Unmute microphone"; onClicked: root.daemon.send("record.toggle_mic", {}) }
        IconButton { objectName: "system"; ts: root.theme.ts; visible: root.live; iconText: root.daemon.state.system !== false ? root.iVolume : root.iVolumeOff; opacity: root.daemon.state.system !== false ? 0.4 + 0.6 * root.daemon.systemLevel : 0.5; tooltipText: root.daemon.state.system !== false ? "Mute system audio" : "Unmute system audio"; onClicked: root.daemon.send("record.toggle_system", {}) }
    }
    RowLayout {   // profile name (gives way first), mode picker, settings
        id: hdrRight
        anchors.left: hdrLeft.right; anchors.leftMargin: root.theme.gap
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: root.theme.gap
        Item { Layout.fillWidth: true; Layout.preferredWidth: 0 }
        UiText {   // which profile's context is in use
            Layout.alignment: Qt.AlignVCenter
            Layout.maximumWidth: Style.space(160)
            Layout.minimumWidth: 0
            horizontalAlignment: Text.AlignRight
            // only when there is room: a name cut to "Bac…" tells nothing
            visible: root.profileName !== "" && root.width - hdrLeft.width - picker.implicitWidth - gear.implicitWidth - root.theme.gap * 4 > Math.round(Style.space(110) * root.theme.ts)
            textFormat: Text.PlainText
            text: root.profileName
            elide: Text.ElideRight
            mayCut: true
            color: Commons.Color.popups.text
            opacity: root.theme.dim
            font.pixelSize: root.theme.caption
        }
        ModePicker { id: picker; daemon: root.daemon; theme: root.theme; bounds: root; Layout.alignment: Qt.AlignVCenter }
        IconButton { id: gear; objectName: "settings"; ts: root.theme.ts; Layout.alignment: Qt.AlignVCenter; iconText: root.iCog; active: root.library; tooltipText: "Settings"; onClicked: root.settingsClicked() }
    }
}
