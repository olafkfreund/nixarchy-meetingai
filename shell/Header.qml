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
        IconButton { ts: root.theme.ts; iconText: root.daemon.recording ? root.iStop : root.iRecord; active: root.daemon.recording; tooltipText: root.daemon.recording ? "Stop recording" : "Start recording"; onClicked: { if (root.daemon.recording) root.daemon.send("record.stop", {}); else root.daemon.startRecording(false); } }
        UiText {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.daemon.recording
            textFormat: Text.PlainText
            text: root.elapsed
            color: Commons.Color.popups.text
            font.pixelSize: root.theme.body
            leftPadding: Style.spacing.xs; rightPadding: Style.spacing.xs
        }
        // Brightness follows the live level; a struck-out glyph means muted.
        IconButton { ts: root.theme.ts; visible: root.daemon.recording; iconText: root.daemon.state.mic !== false ? root.iMic : root.iMicOff; opacity: root.daemon.state.mic !== false ? 0.4 + 0.6 * root.daemon.micLevel : 0.5; tooltipText: root.daemon.state.mic !== false ? "Mute microphone" : "Unmute microphone"; onClicked: root.daemon.send("record.toggle_mic", {}) }
        IconButton { ts: root.theme.ts; visible: root.daemon.recording; iconText: root.daemon.state.system !== false ? root.iVolume : root.iVolumeOff; opacity: root.daemon.state.system !== false ? 0.4 + 0.6 * root.daemon.systemLevel : 0.5; tooltipText: root.daemon.state.system !== false ? "Mute system audio" : "Unmute system audio"; onClicked: root.daemon.send("record.toggle_system", {}) }
    }
    RowLayout {
        id: hdrRight
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: root.theme.gap
        UiText {   // which profile's context is in use
            Layout.alignment: Qt.AlignVCenter
            visible: root.profileName !== ""
            textFormat: Text.PlainText
            text: root.profileName
            elide: Text.ElideRight
            Layout.maximumWidth: Style.space(160)
            color: Commons.Color.popups.text
            opacity: 0.7
            font.pixelSize: root.theme.caption
        }
        Rectangle {   // segmented control
            Layout.alignment: Qt.AlignVCenter
            implicitWidth: seg.width + Style.space(4); implicitHeight: seg.height + Style.space(4)
            radius: root.theme.radius
            color: Qt.alpha(Commons.Color.foreground, 0.08)
            Row {
                id: seg
                anchors.centerIn: parent
                IconButton { ts: root.theme.ts; text: "General"; radius: root.theme.radius; active: root.daemon.settings.meeting_mode !== "interview" && !root.team && !root.iv; tooltipText: "General meeting"; onClicked: root.daemon.setSetting("meeting_mode", "general") }
                IconButton { ts: root.theme.ts; text: "Interview"; radius: root.theme.radius; active: root.daemon.settings.meeting_mode === "interview"; tooltipText: "Interview mode"; onClicked: root.daemon.setSetting("meeting_mode", "interview") }
                IconButton { ts: root.theme.ts; text: "Team"; radius: root.theme.radius; active: root.team; tooltipText: "Team meeting: items and series"; onClicked: root.daemon.setSetting("meeting_mode", "team_meeting") }
                IconButton { ts: root.theme.ts; text: "Interviewer"; radius: root.theme.radius; active: root.iv; tooltipText: "Interviewer: question bank and evidence notes for a candidate"; onClicked: root.daemon.setSetting("meeting_mode", "interviewer") }
            }
        }
        IconButton { ts: root.theme.ts; Layout.alignment: Qt.AlignVCenter; iconText: root.iCog; active: root.library; tooltipText: "Settings"; onClicked: root.settingsClicked() }
    }
}
