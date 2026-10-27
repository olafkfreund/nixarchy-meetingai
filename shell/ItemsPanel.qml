pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects (Style.spacing, Color.popups) the linter cannot type
import QtQuick
import qs.Commons
import qs.Commons as Commons

// The card above NOW: Items (team meeting: Last time drawer, "Earlier task?" callout,
// items by kind) or, for the interviewer, the Question bank. The parent sets visibility,
// anchors and height; `want` is the height the content wants.
Rectangle {
    id: root

    required property var daemon
    required property var theme
    required property bool iv
    required property bool lastOpen
    required property var lastTime   // last_time.get reply for the current series, or null

    signal lastOpenToggled()
    signal itemsChanged()   // an earlier item was marked done: reload last_time

    readonly property real iconPx: root.theme.title
    readonly property string iDown: String.fromCodePoint(0xf0140)
    readonly property string iRight: String.fromCodePoint(0xf0142)
    readonly property string iCheck: String.fromCodePoint(0xf012c)
    readonly property string iClose: String.fromCodePoint(0xf0156)
    readonly property string iAction: String.fromCodePoint(0xf0139)
    readonly property string iDecision: String.fromCodePoint(0xf023b)
    readonly property string iQuestion: String.fromCodePoint(0xf02d7)
    readonly property var kinds: [{ kind: "action", title: "Actions", glyph: root.iAction }, { kind: "decision", title: "Decisions", glyph: root.iDecision }, { kind: "question", title: "Questions", glyph: root.iQuestion }]
    readonly property real want: (root.iv ? qbank : icol).implicitHeight + root.theme.padY * 2

    radius: root.theme.radius
    color: Qt.alpha(Commons.Color.accent, 0.12)
    Rectangle { width: Style.space(4); height: parent.height - root.theme.radius; anchors.verticalCenter: parent.verticalCenter; x: Style.spacing.xs; radius: width / 2; color: Commons.Color.accent }
    Flickable {
        x: root.theme.padX; y: root.theme.padY
        width: parent.width - root.theme.padX * 2
        height: parent.height - root.theme.padY * 2
        contentHeight: (root.iv ? qbank : icol).implicitHeight
        clip: true
        QuestionBankPanel {
            id: qbank
            theme: root.theme
            daemon: root.daemon
            visible: root.iv
            width: parent.width
        }
        Column {
            id: icol
            visible: !root.iv
            width: parent.width
            spacing: Style.space(8)
            Row {
                spacing: Style.spacing.md
                UiText { anchors.verticalCenter: parent.verticalCenter; textFormat: Text.PlainText; text: "Items"; color: Commons.Color.popups.text; font.pixelSize: root.theme.caption; font.bold: true; font.capitalization: Font.SmallCaps; font.letterSpacing: 1.5 }
                IconButton { ts: root.theme.ts; visible: root.lastTime && root.lastTime.meeting; iconText: root.lastOpen ? root.iDown : root.iRight; text: "Last time"; tooltipText: "Recap and open items of the previous meeting in this series"; onClicked: root.lastOpenToggled() }
            }
            Column {   // Last time drawer
                visible: root.lastOpen && root.lastTime !== null
                width: parent.width
                spacing: Style.space(4)
                UiText { width: parent.width; textFormat: Text.PlainText; wrapMode: Text.Wrap; text: root.lastTime ? String(root.lastTime.title || "") + " · " + String(root.lastTime.started_at || "") : ""; color: Commons.Color.popups.text; font.pixelSize: root.theme.caption; font.bold: true }
                UiText { width: parent.width; textFormat: Text.MarkdownText; wrapMode: Text.Wrap; text: root.lastTime ? String(root.lastTime.recap || "") : ""; color: Commons.Color.popups.text; font.pixelSize: root.theme.body; lineHeight: root.theme.leading }
                Repeater {
                    model: root.lastTime ? (root.lastTime.items || []) : []
                    UiText { required property var modelData; width: icol.width; textFormat: Text.PlainText; wrapMode: Text.Wrap; text: "- " + modelData.text; color: Commons.Color.popups.text; opacity: 0.8; font.pixelSize: root.theme.body }
                }
            }
            Rectangle {   // Earlier task?
                id: callout
                visible: root.daemon.earlierItem !== null
                width: parent.width
                height: ccol.implicitHeight + root.theme.padY * 2
                radius: root.theme.radius
                color: Qt.alpha(Commons.Color.foreground, 0.10)
                border.width: Style.normalBorderWidth
                border.color: Commons.Color.popups.border
                readonly property var it: root.daemon.earlierItem ? root.daemon.earlierItem.item : ({})
                function dismiss() { root.daemon.earlierItem = null; }
                Column {
                    id: ccol
                    x: root.theme.padX; y: root.theme.padY
                    width: parent.width - root.theme.padX * 2
                    spacing: Style.space(6)
                    UiText { textFormat: Text.PlainText; text: "Earlier task?"; color: Commons.Color.popups.text; font.pixelSize: root.theme.caption; font.bold: true; font.capitalization: Font.SmallCaps; font.letterSpacing: 1.5 }
                    UiText { width: parent.width; textFormat: Text.PlainText; wrapMode: Text.Wrap; text: String(callout.it.text || "") + "  (" + String(callout.it.created_at || "").slice(0, 10) + ")"; color: Commons.Color.popups.text; font.pixelSize: root.theme.body; font.bold: true }
                    UiText { width: parent.width; visible: root.daemon.earlierItem !== null && root.daemon.earlierItem.quote !== ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; text: root.daemon.earlierItem ? "“" + root.daemon.earlierItem.quote + "”" : ""; color: Commons.Color.popups.text; opacity: 0.8; font.italic: true; font.pixelSize: root.theme.body }
                    Row {
                        spacing: Style.spacing.xs
                        IconButton { ts: root.theme.ts; text: "Discussed"; bordered: true; tooltipText: "It came up again; the link is already saved"; onClicked: callout.dismiss() }
                        IconButton { ts: root.theme.ts; iconText: root.iCheck; text: "Done"; bordered: true; tooltipText: "Mark the earlier item done"; onClicked: { root.daemon.send("items.update", { id: callout.it.id, status: "done" }, function() { root.itemsChanged(); }); callout.dismiss(); } }
                        IconButton { ts: root.theme.ts; text: "Not related"; bordered: true; tooltipText: "Remove the link to the earlier item"; onClicked: { if (root.daemon.earlierItem.new_item) root.daemon.updateItem(root.daemon.earlierItem.new_item, { origin_item: "" }); callout.dismiss(); } }
                    }
                }
            }
            UiText {
                visible: root.daemon.items.length === 0
                textFormat: Text.PlainText
                text: "Nothing captured yet"
                color: Commons.Color.popups.text
                opacity: 0.6
                font.italic: true
                font.pixelSize: root.theme.body
            }
            Repeater {
                model: root.kinds
                Column {
                    id: grp
                    required property var modelData
                    readonly property var rows: root.daemon.items.filter(function(i) { return i.kind === grp.modelData.kind; })
                    visible: rows.length > 0
                    width: icol.width
                    spacing: Style.space(4)
                    UiText { textFormat: Text.PlainText; text: grp.modelData.title; color: Commons.Color.popups.text; opacity: 0.7; font.pixelSize: root.theme.caption; font.bold: true }
                    Repeater {
                        model: grp.rows
                        Column {
                            id: cell
                            required property var modelData
                            width: grp.width
                            Item {
                                id: irow
                                readonly property var modelData: cell.modelData
                                readonly property bool open: modelData.status === "open"
                                width: grp.width
                                height: Math.max(root.theme.ctl, itext.implicitHeight + Style.space(4))
                                opacity: open ? 1 : 0.5
                                UiText { id: glyph; anchors.verticalCenter: parent.verticalCenter; textFormat: Text.PlainText; text: grp.modelData.glyph; color: Commons.Color.popups.text; font.pixelSize: root.iconPx }
                                Row {
                                    id: acts
                                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                                    IconButton { ts: root.theme.ts; iconText: root.iCheck; active: irow.modelData.status === "done"; tooltipText: irow.modelData.status === "done" ? "Reopen" : "Mark done"; onClicked: root.daemon.updateItem(irow.modelData.id, { status: irow.modelData.status === "done" ? "open" : "done" }) }
                                    IconButton { ts: root.theme.ts; iconText: root.iClose; active: irow.modelData.status === "dropped"; tooltipText: irow.modelData.status === "dropped" ? "Reopen" : "Drop"; onClicked: root.daemon.updateItem(irow.modelData.id, { status: irow.modelData.status === "dropped" ? "open" : "dropped" }) }
                                }
                                UiText {   // owner, due, source line
                                    id: meta
                                    anchors.right: acts.left; anchors.rightMargin: Style.spacing.md; anchors.verticalCenter: parent.verticalCenter
                                    textFormat: Text.PlainText
                                    text: [irow.modelData.owner, irow.modelData.due, irow.modelData.t0 !== null && irow.modelData.t0 !== undefined ? Math.floor(irow.modelData.t0 / 60) + ":" + ("0" + Math.floor(irow.modelData.t0 % 60)).slice(-2) : ""].filter(function(x) { return x; }).join(" · ")
                                    color: Commons.Color.popups.text
                                    opacity: 0.7
                                    font.pixelSize: root.theme.caption
                                }
                                TextInput {   // inline edit; a draft is shown italic
                                    id: itext
                                    anchors.left: glyph.right; anchors.leftMargin: Style.spacing.md
                                    anchors.right: meta.left; anchors.rightMargin: Style.spacing.md
                                    anchors.verticalCenter: parent.verticalCenter
                                    wrapMode: TextInput.Wrap
                                    font.family: Style.font.family
                                    text: irow.modelData.text
                                    color: Commons.Color.popups.text
                                    selectionColor: Commons.Color.accent
                                    font.pixelSize: root.theme.body
                                    font.italic: irow.modelData.draft === true || irow.modelData.draft === 1
                                    font.strikeout: irow.modelData.status !== "open"
                                    onEditingFinished: { if (text.trim() !== "" && text !== irow.modelData.text) root.daemon.updateItem(irow.modelData.id, { text: text.trim() }); }
                                    onActiveFocusChanged: root.daemon.itemEditing = activeFocus
                                    Keys.onEscapePressed: function(event) { text = irow.modelData.text; focus = false; event.accepted = true; }
                                }
                            }
                            Row {   // a continuation the callout did not show, or that was kept
                                visible: !!cell.modelData.origin_item
                                x: Style.spacing.md + root.iconPx
                                spacing: Style.spacing.xs
                                UiText { width: Math.min(implicitWidth, grp.width - unlink.width - Style.spacing.md * 3 - root.iconPx); elide: Text.ElideRight; textFormat: Text.PlainText; anchors.verticalCenter: parent.verticalCenter; text: "continues: " + String(cell.modelData.origin_text || "an earlier item"); color: Commons.Color.popups.text; opacity: 0.6; font.pixelSize: root.theme.caption }
                                IconButton { id: unlink; ts: root.theme.ts; iconText: root.iClose; tooltipText: "Unlink from the earlier item"; onClicked: root.daemon.updateItem(cell.modelData.id, { origin_item: "" }) }
                            }
                        }
                    }
                }
            }
        }
    }
}
