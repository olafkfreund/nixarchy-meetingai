pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects (Style.spacing, Color.popups) the linter cannot type
import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Commons as Commons

// Conversation top: the transcript toggle and, below it, the transcript with archived
// answers in one virtualized list (capped at 500 rows by Client). The parent anchors
// it to the top and sets `height` to the y of the card below it.
Item {
    id: root

    required property var daemon
    required property var theme
    required property var themedLinks
    required property bool open
    signal toggled()

    readonly property real barHeight: tbar.height + (hint.visible ? hint.height + Style.spacing.md : 0)   // the toggle, and the hint while there is nothing to list
    readonly property string iDown: String.fromCodePoint(0xf0140)
    readonly property string iRight: String.fromCodePoint(0xf0142)

    Row {   // transcript toggle
        id: tbar
        anchors.top: parent.top
        IconButton { ts: root.theme.ts; iconText: root.open ? root.iDown : root.iRight; text: "Transcript"; tooltipText: root.open ? "Collapse the transcript" : "Expand the transcript"; onClicked: root.toggled() }
    }

    UiText {   // nothing yet: one line under the toggle, and NOW moves up to it
        id: hint
        visible: root.open && root.daemon.transcript.count === 0
        anchors { left: parent.left; right: parent.right; top: tbar.bottom; topMargin: Style.spacing.xs }
        textFormat: Text.PlainText
        text: root.daemon.recording ? "Listening for speech\u2026" : "Press record to start"
        color: Commons.Color.popups.text
        opacity: root.theme.dim
        font.italic: true
        font.pixelSize: root.theme.bodySmall
    }

    Item {
        visible: root.open
        anchors { top: tbar.bottom; left: parent.left; right: parent.right; bottom: parent.bottom; bottomMargin: Style.spacing.md }
        ListView {
            id: tlist
            anchors { bottom: parent.bottom; left: parent.left; right: parent.right }
            // contentHeight is estimated from the delegates in view, which depend on height: read it one tick late
            property real fitH: 0
            height: Math.min(fitH, parent.height)
            clip: true
            spacing: root.theme.section
            model: root.daemon.transcript
            property bool follow: true
            // Exact bottom; positionViewAtEnd works from estimated delegate heights.
            function toEnd() { if (follow) contentY = originY + Math.max(0, contentHeight - height); }
            onContentHeightChanged: Qt.callLater(function() { fitH = contentHeight; toEnd(); })
            onHeightChanged: Qt.callLater(toEnd)   // the NOW panel grows and shrinks the list
            onMovementEnded: follow = atYEnd
            delegate: Item {
                id: row
                required property string who
                required property string speaker
                required property string body
                required property bool partial
                property bool expanded: false
                readonly property bool answer: who === "answer"
                readonly property bool mine: who === "mic" || who === "asked"
                readonly property bool bubble: who === "mic" || who === "system" || who === "room" || who === "asked"
                readonly property string label: who === "asked" ? "You · Asked" : who === "mic" ? "You · Spoke" : who === "room" ? "Room" : (speaker !== "" ? speaker : "Them")
                width: ListView.view.width
                height: answer ? card2.height : bubble ? bub.height : err.implicitHeight
                // Markdown and line height settle after layout; keep the end pinned while following.
                onHeightChanged: Qt.callLater(tlist.toEnd)
                Rectangle {   // flat, borderless bubble
                    id: bub
                    visible: row.bubble
                    x: row.mine ? row.width - width : 0
                    width: Math.min(row.width * 0.8, Math.max(btxt.implicitWidth, blabel.implicitWidth) + root.theme.padX * 1.5)
                    height: bcol.implicitHeight + root.theme.padY * 1.5
                    radius: root.theme.radius
                    color: row.mine ? Qt.alpha(Commons.Color.accent, 0.14) : Qt.alpha(Commons.Color.foreground, 0.06)
                    Column {
                        id: bcol
                        x: root.theme.padX * 0.75; y: root.theme.padY * 0.75
                        width: parent.width - root.theme.padX * 1.5
                        spacing: Style.space(3)
                        UiText { id: blabel; textFormat: Text.PlainText; text: row.label; color: Commons.Color.popups.text; opacity: root.theme.dim; font.pixelSize: root.theme.caption; font.bold: true }
                        UiText {
                            id: btxt
                            width: parent.width
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                            text: row.body
                            color: Commons.Color.popups.text
                            font.italic: row.partial
                            font.pixelSize: root.theme.bodySmall
                            lineHeight: root.theme.leading
                        }
                    }
                }
                Rectangle {   // an earlier answer: accent stripe, label, markdown
                    id: card2
                    visible: row.answer
                    width: parent.width
                    height: ansCol.implicitHeight + root.theme.padY * 1.5
                    radius: root.theme.radius
                    color: Qt.alpha(Commons.Color.accent, 0.08)
                    Rectangle { width: Style.space(4); height: parent.height - root.theme.radius; anchors.verticalCenter: parent.verticalCenter; x: Style.spacing.xs; radius: width / 2; color: Commons.Color.accent }
                    Column {
                        id: ansCol
                        x: root.theme.padX; y: root.theme.padY * 0.75
                        width: parent.width - root.theme.padX * 2
                        spacing: Style.space(4)
                        RowLayout {
                            width: parent.width
                            UiText { Layout.fillWidth: true; textFormat: Text.PlainText; text: "Suggested answer"; color: Commons.Color.popups.text; opacity: root.theme.dim; font.pixelSize: root.theme.caption; font.bold: true; }
                            IconButton { objectName: "answerToggle"; ts: root.theme.ts; visible: row.expanded || ansText.truncated; text: row.expanded ? "Hide" : "Show"; tooltipText: row.expanded ? "Collapse the answer" : "Show the whole answer"; onClicked: row.expanded = !row.expanded }
                        }
                        UiText {
                            id: ansText
                            width: parent.width
                            // earlier answers fold to two plain lines; Show brings the Markdown back
                            textFormat: row.expanded ? Text.MarkdownText : Text.PlainText
                            wrapMode: Text.Wrap
                            maximumLineCount: row.expanded ? 100000 : 2
                            mayCut: !row.expanded   // folded to two lines; Show opens it
                            elide: row.expanded ? Text.ElideNone : Text.ElideRight
                            text: row.expanded ? root.themedLinks(row.body) : String(row.body).replace(/\*+|__|`/g, "").replace(/^#+\s*/gm, "").replace(/^\s*[-*]\s+/gm, "\u2022 ").replace(/\s*\n+\s*/g, "  ")
                            color: Commons.Color.popups.text
                            linkColor: Commons.Color.accent
                            font.pixelSize: root.theme.bodySmall
                            lineHeight: root.theme.leading
                            onLinkActivated: function(link) { Qt.openUrlExternally(link); }
                        }
                    }
                }
                UiText {
                    id: err
                    visible: !row.answer && !row.bubble
                    width: parent.width
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    text: row.who === "error" ? "Error: " + row.body : row.body
                    color: Commons.Color.popups.text
                    opacity: row.who === "notice" ? root.theme.dim : 1
                    font.italic: row.who === "notice"
                    font.pixelSize: (row.who === "notice" ? Style.font.caption : Style.font.bodySmall) * root.theme.ts
                }
            }
        }
    }
}
