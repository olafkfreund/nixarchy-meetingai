pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects (Style.spacing, Color.popups) the linter cannot type
import QtQuick
import qs.Commons
import qs.Commons as Commons

// Overlay header: one quiet toolbar of icon buttons. Dragging it moves the card.
Item {
    id: root

    required property var daemon
    required property real ts
    required property real round
    required property bool team
    required property bool iv
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
    readonly property string iCog: String.fromCodePoint(0xf0493)

    height: hdrLeft.height
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
        spacing: Style.spacing.xs
        IconButton { ts: root.ts; iconText: root.daemon.recording ? root.iStop : root.iRecord; active: root.daemon.recording; tooltipText: root.daemon.recording ? "Stop recording" : "Start recording"; onClicked: { if (root.daemon.recording) root.daemon.send("record.stop", {}); else root.daemon.startRecording(false); } }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.daemon.recording
            textFormat: Text.PlainText
            text: root.elapsed
            color: Commons.Color.popups.text
            font.pixelSize: Style.font.body * root.ts
            leftPadding: Style.spacing.xs; rightPadding: Style.spacing.xs
        }
        // Brightness follows the live level; a struck-out glyph means muted.
        IconButton { ts: root.ts; visible: root.daemon.recording; iconText: root.daemon.state.mic !== false ? root.iMic : root.iMicOff; opacity: root.daemon.state.mic !== false ? 0.4 + 0.6 * root.daemon.micLevel : 0.5; tooltipText: root.daemon.state.mic !== false ? "Mute microphone" : "Unmute microphone"; onClicked: root.daemon.send("record.toggle_mic", {}) }
        IconButton { ts: root.ts; visible: root.daemon.recording; iconText: root.daemon.state.system !== false ? root.iVolume : root.iVolumeOff; opacity: root.daemon.state.system !== false ? 0.4 + 0.6 * root.daemon.systemLevel : 0.5; tooltipText: root.daemon.state.system !== false ? "Mute system audio" : "Unmute system audio"; onClicked: root.daemon.send("record.toggle_system", {}) }
    }
    Row {
        anchors.right: parent.right
        spacing: Style.spacing.md
        Text {   // which profile's context is in use
            anchors.verticalCenter: parent.verticalCenter
            visible: root.profileName !== ""
            textFormat: Text.PlainText
            text: root.profileName
            elide: Text.ElideRight
            width: Math.min(implicitWidth, Style.space(160))
            color: Commons.Color.popups.text
            opacity: 0.7
            font.pixelSize: Style.font.caption * root.ts
        }
        Rectangle {   // segmented control
            width: seg.width + Style.space(4); height: seg.height + Style.space(4)
            radius: root.round
            color: Qt.alpha(Commons.Color.foreground, 0.08)
            Row {
                id: seg
                anchors.centerIn: parent
                IconButton { ts: root.ts; text: "General"; radius: root.round; active: root.daemon.settings.meeting_mode !== "interview" && !root.team && !root.iv; tooltipText: "General meeting"; onClicked: root.daemon.setSetting("meeting_mode", "general") }
                IconButton { ts: root.ts; text: "Interview"; radius: root.round; active: root.daemon.settings.meeting_mode === "interview"; tooltipText: "Interview mode"; onClicked: root.daemon.setSetting("meeting_mode", "interview") }
                IconButton { ts: root.ts; text: "Team"; radius: root.round; active: root.team; tooltipText: "Team meeting: items and series"; onClicked: root.daemon.setSetting("meeting_mode", "team_meeting") }
                IconButton { ts: root.ts; text: "Interviewer"; radius: root.round; active: root.iv; tooltipText: "Interviewer: question bank and evidence notes for a candidate"; onClicked: root.daemon.setSetting("meeting_mode", "interviewer") }
            }
        }
        IconButton { ts: root.ts; iconText: root.iCog; active: root.library; tooltipText: "Settings"; onClicked: root.settingsClicked() }
    }
}
