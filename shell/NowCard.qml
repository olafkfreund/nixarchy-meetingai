pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects (Style.spacing, Color.popups) the linter cannot type
import QtQuick
import qs.Commons
import qs.Commons as Commons

// NOW: who asked, the question, status, and the answer larger than the transcript.
// The parent sets visibility, anchors and height; `nowWant` is the height the content wants.
Rectangle {
    id: root

    required property var daemon
    required property var theme
    required property string status
    required property var themedLinks

    readonly property real nowWant: nowHead.height + (nowQ.visible ? nowQ.implicitHeight : 0) + answerText.implicitHeight + nowCol.spacing * 2 + root.theme.padY * 2

    radius: root.theme.radius
    color: Qt.alpha(Commons.Color.accent, 0.12)
    Rectangle { width: Style.space(4); height: parent.height - root.theme.radius; anchors.verticalCenter: parent.verticalCenter; x: Style.spacing.xs; radius: width / 2; color: Commons.Color.accent }
    Column {
        id: nowCol
        x: root.theme.padX; y: root.theme.padY
        width: parent.width - root.theme.padX * 2
        spacing: Style.space(8)
        Row {
            id: nowHead
            spacing: Style.spacing.md
            UiText { textFormat: Text.PlainText; text: "Now"; color: Commons.Color.popups.text; font.pixelSize: root.theme.caption; font.bold: true; font.capitalization: Font.SmallCaps; font.letterSpacing: 1.5 }
            UiText { textFormat: Text.PlainText; text: root.status; color: Commons.Color.popups.text; font.pixelSize: root.theme.caption }
            UiText {
                textFormat: Text.PlainText
                visible: text !== ""
                text: (root.daemon.now.provider + " " + root.daemon.now.model).trim()
                color: Commons.Color.popups.text
                font.pixelSize: root.theme.caption
            }
        }
        UiText {
            id: nowQ
            width: parent.width
            visible: root.daemon.now.question !== ""
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            maximumLineCount: 2
            elide: Text.ElideRight
            text: root.daemon.now.asker + " asked: " + root.daemon.now.question
            color: Commons.Color.popups.text
            font.pixelSize: root.theme.body
            font.bold: true
        }
        Flickable {
            id: nowFlick
            width: parent.width
            height: Math.max(0, Math.min(answerText.implicitHeight, root.height - nowHead.height - (nowQ.visible ? nowQ.implicitHeight : 0) - nowCol.spacing * 2 - root.theme.padY * 2))
            contentHeight: answerText.implicitHeight
            clip: true
            // Follow the tail while streaming; a finished answer is read from its top.
            Binding { target: nowFlick; property: "contentY"; value: root.daemon.answering ? Math.max(0, nowFlick.contentHeight - nowFlick.height) : 0; restoreMode: Binding.RestoreNone }
            UiText {
                id: answerText
                width: parent.width
                textFormat: Text.MarkdownText
                wrapMode: Text.Wrap
                text: root.themedLinks(root.daemon.now.body)
                color: Commons.Color.popups.text
                linkColor: Commons.Color.accent
                font.pixelSize: root.theme.subtitle
                lineHeight: root.theme.leading
                onLinkActivated: function(link) { Qt.openUrlExternally(link); }
            }
        }
    }
}
