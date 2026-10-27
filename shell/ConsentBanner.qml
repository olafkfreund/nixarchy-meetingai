pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects (Style.spacing, Color.popups) the linter cannot type
import QtQuick
import qs.Commons
import qs.Commons as Commons

// Consent: the daemon refused record.start until the candidate is told.
Rectangle {
    id: root

    required property var daemon
    required property real ts
    required property real round
    required property real padX
    required property real padY

    readonly property string iRecord: String.fromCodePoint(0xf044b)

    visible: root.daemon.consentPrompt !== ""
    height: ccol2.implicitHeight + root.padY * 2
    radius: root.round
    color: Qt.alpha(Commons.Color.foreground, 0.10)
    border.width: Style.normalBorderWidth
    border.color: Commons.Color.popups.border
    Column {
        id: ccol2
        x: root.padX; y: root.padY
        width: parent.width - root.padX * 2
        spacing: Style.space(6)
        Text { width: parent.width; textFormat: Text.PlainText; wrapMode: Text.Wrap; text: root.daemon.consentPrompt; color: Commons.Color.popups.text; font.pixelSize: Style.font.body * root.ts; font.bold: true }
        Text { width: parent.width; textFormat: Text.PlainText; wrapMode: Text.Wrap; text: root.daemon.cand && root.daemon.cand.candidate.local_only ? "Local-only is on: audio and text stay on this machine." : "Recommended: turn on Local-only in the Candidate tab so nothing leaves this machine."; color: Commons.Color.popups.text; opacity: 0.8; font.pixelSize: Style.font.caption * root.ts }
        Row {
            spacing: Style.spacing.xs
            IconButton { ts: root.ts; iconText: root.iRecord; text: "Recording is announced — start"; bordered: true; tooltipText: "Start recording; the candidate has been told"; onClicked: root.daemon.startRecording(true) }
            IconButton { ts: root.ts; text: "Cancel"; bordered: true; tooltipText: "Do not record"; onClicked: root.daemon.consentPrompt = "" }
        }
    }
}
