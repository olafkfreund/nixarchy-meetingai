pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects (Style.spacing, Color.popups) the linter cannot type
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
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

    readonly property real extras: (nowWait.visible ? nowWait.height + nowCol.spacing : 0) + (nowErr.visible ? nowErr.implicitHeight + nowCol.spacing : 0)
    readonly property real nowWant: nowHead.height + (nowQ.visible ? nowQ.implicitHeight : 0) + answerText.height + nowCol.spacing * 2 + root.extras + root.theme.padY * 2

    radius: root.theme.radius
    color: Qt.alpha(Commons.Color.accent, 0.12)
    Rectangle { width: Style.space(4); height: parent.height - root.theme.radius; anchors.verticalCenter: parent.verticalCenter; x: Style.spacing.xs; radius: width / 2; color: Commons.Color.accent }
    Column {
        id: nowCol
        x: root.theme.padX; y: root.theme.padY
        width: parent.width - root.theme.padX * 2
        spacing: Style.space(8)
        Row {   // who and what, on one caption line: Now · Ready · provider model
            id: nowHead
            spacing: root.theme.gap
            UiText { textFormat: Text.PlainText; text: "Now"; color: Commons.Color.popups.text; font.pixelSize: root.theme.caption; font.bold: true; }
            UiText {
                textFormat: Text.PlainText
                text: [root.status, (root.daemon.now.provider + " " + root.daemon.now.model).trim()].filter(function(x) { return x !== ""; }).join("  \u00b7  ")
                color: Commons.Color.popups.text
                opacity: root.theme.dim
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
            mayCut: true
            text: root.daemon.now.asker + " asked: " + root.daemon.now.question
            color: Commons.Color.popups.text
            font.pixelSize: root.theme.body
            font.bold: true
        }
        Rectangle {   // "Writing…": between the question being sent and the first words of the answer
            id: nowWait
            visible: root.daemon.answering && root.daemon.now.body === "" && !root.daemon.now.error
            width: parent.width
            height: Style.space(3)
            radius: height / 2
            color: Qt.alpha(Commons.Color.accent, 0.25)
            clip: true
            Accessible.role: Accessible.ProgressBar
            Accessible.name: "Writing the answer"
            Rectangle {
                id: sweep
                width: parent.width * 0.3; height: parent.height; radius: height / 2
                color: Commons.Color.accent
                NumberAnimation on x { from: -sweep.width; to: nowWait.width; duration: 1200; loops: Animation.Infinite; running: nowWait.visible && !Style.reduceMotion }
            }
        }
        RowLayout {   // a failed answer: the reason and Retry, where the answer would have been
            id: nowErr
            visible: !!root.daemon.now.error
            width: parent.width
            spacing: root.theme.gap
            UiText { Layout.fillWidth: true; Layout.alignment: Qt.AlignVCenter; textFormat: Text.PlainText; wrapMode: Text.Wrap; text: "Error: " + String(root.daemon.now.error || ""); color: Commons.Color.popups.text; font.pixelSize: root.theme.body }
            IconButton { objectName: "retry"; ts: root.theme.ts; Layout.alignment: Qt.AlignVCenter; visible: root.daemon.lastAsk !== null && !root.daemon.now.automatic; text: "Retry"; bordered: true; tooltipText: "Ask the same question again"; onClicked: root.daemon.retry() }
        }
        Flickable {
            id: nowFlick
            width: parent.width
            height: Math.max(0, Math.min(answerText.height, root.height - nowHead.height - (nowQ.visible ? nowQ.implicitHeight : 0) - nowCol.spacing * 2 - root.extras - root.theme.padY * 2))
            contentHeight: answerText.height
            clip: true
            // a body taller than its box shows a scroll bar, so nobody reads half an answer without knowing
            ScrollBar.vertical: ScrollBar { policy: nowFlick.contentHeight > nowFlick.height + 1 ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff }
            // Follow the tail while streaming; a finished answer is read from its top.
            Binding { target: nowFlick; property: "contentY"; value: root.daemon.answering ? Math.max(0, nowFlick.contentHeight - nowFlick.height) : 0; restoreMode: Binding.RestoreNone }
            UiText {
                id: answerText
                width: parent.width - Style.space(10)   // room for the scroll bar
                textFormat: Text.MarkdownText
                wrapMode: Text.Wrap
                text: root.themedLinks(root.daemon.now.body)
                color: Commons.Color.popups.text
                linkColor: Commons.Color.accent
                font.pixelSize: root.theme.nowBody
                lineHeight: root.theme.leading
                onLinkActivated: function(link) { Qt.openUrlExternally(link); }
            }
        }
    }
}
