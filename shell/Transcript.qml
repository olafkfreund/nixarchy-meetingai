pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects (Style.spacing, Color.popups) the linter cannot type
import QtQuick
import qs.Commons
import qs.Commons as Commons

// Conversation top: the transcript toggle and, below it, the transcript with archived
// answers in one virtualized list (capped at 500 rows by Client). The parent anchors
// it to the top and sets `height` to the y of the card below it.
Item {
    id: root

    required property var daemon
    required property real ts
    required property real round
    required property real padX
    required property real padY
    required property real gap
    required property real leading
    required property var themedLinks
    required property bool open
    signal toggled()

    readonly property real barHeight: tbar.height
    readonly property string iDown: String.fromCodePoint(0xf0140)
    readonly property string iRight: String.fromCodePoint(0xf0142)

    Row {   // transcript toggle
        id: tbar
        anchors.top: parent.top
        IconButton { ts: root.ts; iconText: root.open ? root.iDown : root.iRight; text: "Transcript"; tooltipText: root.open ? "Collapse the transcript" : "Expand the transcript"; onClicked: root.toggled() }
    }

    Item {
        visible: root.open
        anchors { top: tbar.bottom; left: parent.left; right: parent.right; bottom: parent.bottom; bottomMargin: Style.spacing.md }
        ListView {
            id: tlist
            anchors { bottom: parent.bottom; left: parent.left; right: parent.right }
            height: Math.min(contentHeight, parent.height)
            clip: true
            spacing: root.gap
            model: root.daemon.transcript
            property bool follow: true
            // Exact bottom; positionViewAtEnd works from estimated delegate heights.
            function toEnd() { if (follow) contentY = originY + Math.max(0, contentHeight - height); }
            onContentHeightChanged: Qt.callLater(toEnd)
            onHeightChanged: Qt.callLater(toEnd)   // the NOW panel grows and shrinks the list
            onMovementEnded: follow = atYEnd
            delegate: Item {
                id: row
                required property string who
                required property string speaker
                required property string body
                required property bool partial
                readonly property bool answer: who === "answer"
                readonly property bool mine: who === "mic" || who === "asked"
                readonly property bool bubble: who === "mic" || who === "system" || who === "asked"
                readonly property string label: who === "asked" ? "You · Asked" : who === "mic" ? "You · Spoke" : (speaker !== "" ? speaker : "Them")
                width: ListView.view.width
                height: answer ? card2.height : bubble ? bub.height : err.implicitHeight
                // Markdown and line height settle after layout; keep the end pinned while following.
                onHeightChanged: Qt.callLater(tlist.toEnd)
                Rectangle {   // flat, borderless bubble
                    id: bub
                    visible: row.bubble
                    x: row.mine ? row.width - width : 0
                    width: Math.min(row.width * 0.8, Math.max(btxt.implicitWidth, blabel.implicitWidth) + root.padX * 2)
                    height: bcol.implicitHeight + root.padY * 2
                    radius: root.round
                    color: row.mine ? Qt.alpha(Commons.Color.accent, 0.22) : Qt.alpha(Commons.Color.foreground, 0.10)
                    Column {
                        id: bcol
                        x: root.padX; y: root.padY
                        width: parent.width - root.padX * 2
                        spacing: Style.space(4)
                        Text { id: blabel; textFormat: Text.PlainText; text: row.label; color: Commons.Color.popups.text; font.pixelSize: Style.font.caption * root.ts; font.bold: true }
                        Text {
                            id: btxt
                            width: parent.width
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                            text: row.body
                            color: Commons.Color.popups.text
                            font.italic: row.partial
                            font.pixelSize: Style.font.body * root.ts
                            lineHeight: root.leading
                        }
                    }
                }
                Rectangle {   // an earlier answer: accent stripe, label, markdown
                    id: card2
                    visible: row.answer
                    width: parent.width
                    height: ansCol.implicitHeight + root.padY * 2
                    radius: root.round
                    color: Qt.alpha(Commons.Color.accent, 0.12)
                    Rectangle { width: Style.space(4); height: parent.height - root.round; anchors.verticalCenter: parent.verticalCenter; x: Style.spacing.xs; radius: width / 2; color: Commons.Color.accent }
                    Column {
                        id: ansCol
                        x: root.padX; y: root.padY
                        width: parent.width - root.padX * 2
                        spacing: Style.space(6)
                        Text { textFormat: Text.PlainText; text: "Suggested answer"; color: Commons.Color.popups.text; font.pixelSize: Style.font.caption * root.ts; font.bold: true; font.capitalization: Font.SmallCaps; font.letterSpacing: 1.5 }
                        Text {
                            width: parent.width
                            textFormat: Text.MarkdownText
                            wrapMode: Text.Wrap
                            text: root.themedLinks(row.body)
                            color: Commons.Color.popups.text
                            linkColor: Commons.Color.accent
                            font.pixelSize: Style.font.body * root.ts
                            lineHeight: root.leading
                            onLinkActivated: function(link) { Qt.openUrlExternally(link); }
                        }
                    }
                }
                Text {
                    id: err
                    visible: !row.answer && !row.bubble
                    width: parent.width
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    text: row.body
                    color: row.who === "error" ? Commons.Color.urgent : Commons.Color.popups.text
                    opacity: row.who === "notice" ? 0.6 : 1
                    font.italic: row.who === "notice"
                    font.pixelSize: (row.who === "notice" ? Style.font.caption : Style.font.bodySmall) * root.ts
                }
            }
        }
    }
}
