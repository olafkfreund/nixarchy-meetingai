pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects (Style.spacing, Color.popups) the linter cannot type
import QtQuick
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// Consent: the daemon refused record.start until the candidate is told.
BorderSurface {
    id: root

    required property var daemon
    required property var theme

    readonly property string iRecord: String.fromCodePoint(0xf044b)

    visible: root.daemon.consentPrompt !== ""
    height: ccol2.implicitHeight + root.theme.padY * 2
    radius: root.theme.radius
    color: Qt.alpha(Commons.Color.foreground, 0.10)
    borderSpec: Border.localOrSurfaceSpec("popups", "border", Commons.Color.popups.border, Commons.Color.popups.border, Style.normalBorderWidth)
    Column {
        id: ccol2
        x: root.theme.padX; y: root.theme.padY
        width: parent.width - root.theme.padX * 2
        spacing: Style.space(6)
        UiText { width: parent.width; textFormat: Text.PlainText; wrapMode: Text.Wrap; text: root.daemon.consentPrompt; color: Commons.Color.popups.text; font.pixelSize: root.theme.body; font.bold: true }
        UiText { width: parent.width; textFormat: Text.PlainText; wrapMode: Text.Wrap; text: root.daemon.cand && root.daemon.cand.candidate.local_only ? "Local-only is on: audio and text stay on this machine." : "Recommended: turn on Local-only in the Candidate tab so nothing leaves this machine."; color: Commons.Color.popups.text; opacity: root.theme.dim; font.pixelSize: root.theme.caption }
        Row {
            spacing: Style.spacing.xs
            IconButton { ts: root.theme.ts; iconText: root.iRecord; text: "Recording is announced — start"; bordered: true; tooltipText: "Start recording; the candidate has been told"; onClicked: root.daemon.startRecording(true) }
            IconButton { ts: root.theme.ts; text: "Cancel"; bordered: true; tooltipText: "Do not record"; onClicked: root.daemon.consentPrompt = "" }
        }
    }
}
