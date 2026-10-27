pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects (Style.spacing, Color.popups) the linter cannot type
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// Question box. Grows to 5 lines then scrolls; Enter sends, Shift+Enter adds
// a line. Code and large pastes become chips; files go as paths.
Item {
    id: root

    property real ts: 1
    // Nerd Font Material Design glyphs (nf-md-*)
    readonly property string iBulb: String.fromCodePoint(0xf0336)
    readonly property string iAttach: String.fromCodePoint(0xf03e2)
    readonly property string iCamera: String.fromCodePoint(0xf0100)
    readonly property string iSend: String.fromCodePoint(0xf048a)
    readonly property int bigPaste: 2000
    property var chips: []   // { kind: "text"|"file", label, text?, path? }
    signal submit(string text, var chips)
    signal preview(string text)
    signal screenshot()
    signal quick()
    signal pick(int n)   // 1..3 typed into an empty composer: choose that suggestion
    readonly property bool empty: input.text.trim() === "" && root.chips.length === 0

    implicitHeight: col.implicitHeight

    function addChip(chip) { root.chips = root.chips.concat([chip]); }

    function send() {
        var t = input.text.trim();
        if (t === "" && root.chips.length === 0) return;
        root.submit(t, root.chips);
        input.text = "";
        root.chips = [];
    }

    Column {
        id: col
        width: parent.width
        spacing: Style.spacing.xs

        Flow {
            width: parent.width
            spacing: Style.spacing.xs
            visible: root.chips.length > 0

            Repeater {
                model: root.chips
                Row {
                    id: chip
                    required property var modelData
                    required property int index
                    IconButton { ts: root.ts;
                        text: chip.modelData.label
                        bordered: true
                        onClicked: if (chip.modelData.kind === "text") root.preview(chip.modelData.text)
                    }
                    PanelActionButton {
                        iconText: "\u00d7"
                        fontSize: Style.font.icon * root.ts
                        onClicked: root.chips = root.chips.filter(function(c, i) { return i !== chip.index; })
                    }
                }
            }
        }

        IconButton {   // quick action, above the field
            ts: root.ts
            iconText: root.iBulb
            text: "What should I say?"
            tooltipText: "Ask for a suggested reply to the latest question"
            onClicked: root.quick()
        }

        Rectangle {   // one rounded field: attach and screenshot on the left, send on the right
            width: parent.width
            height: Math.max(field.height, Style.spacing.controlHeight) + Style.spacing.xs * 2
            radius: Math.max(Style.cornerRadius, 12)
            color: Style.controlFill(input.activeFocus, input.hovered, Commons.Color.foreground, Commons.Color.accent)
            border.width: Style.normalBorderWidth
            border.color: Qt.alpha(Commons.Color.popups.border, input.activeFocus ? 1 : 0.5)

            IconButton { id: attach; ts: root.ts; x: Style.spacing.xs; anchors.bottom: parent.bottom; anchors.bottomMargin: Style.spacing.xs; iconText: root.iAttach; tooltipText: "Attach a file"; onClicked: files.open() }
            IconButton { id: shot; ts: root.ts; anchors.left: attach.right; anchors.bottom: attach.bottom; iconText: root.iCamera; tooltipText: "Capture a screenshot region"; onClicked: root.screenshot() }
            IconButton { id: send; ts: root.ts; anchors.right: parent.right; anchors.rightMargin: Style.spacing.xs; anchors.bottom: attach.bottom; iconText: root.iSend; active: !root.empty; tooltipText: "Send (Enter)"; onClicked: root.send() }

            ScrollView {
                id: field
                anchors.left: shot.right; anchors.right: send.left
                anchors.verticalCenter: parent.verticalCenter
                height: Math.min(input.implicitHeight, fm.height * 5 + Style.spacing.md * 2)

                TextArea {
                    id: input
                    wrapMode: TextEdit.Wrap
                    placeholderText: "Ask about the meeting"
                    color: Commons.Color.popups.text
                    placeholderTextColor: Qt.alpha(Commons.Color.popups.text, 0.5)
                    font.pixelSize: Style.font.body * root.ts
                    background: null
                    Keys.onPressed: function(e) {
                        if (root.empty && e.key >= Qt.Key_1 && e.key <= Qt.Key_3 && !(e.modifiers & ~Qt.KeypadModifier)) {
                            root.pick(e.key - Qt.Key_0);
                            e.accepted = true;
                            return;
                        }
                        if ((e.key === Qt.Key_Return || e.key === Qt.Key_Enter) && !(e.modifiers & Qt.ShiftModifier)) {
                            root.send();
                            e.accepted = true;
                        }
                    }
                    onTextChanged: {
                        if (text.length > root.bigPaste || text.indexOf("```") >= 0) {
                            root.addChip({ kind: "text", label: text.indexOf("```") >= 0 ? "Code" : "Pasted " + text.length + " chars", text: text });
                            text = "";
                        }
                    }
                }
            }
        }
    }

    FontMetrics { id: fm; font.pixelSize: Style.font.body * root.ts }

    FileDialog {
        id: files
        onAccepted: {
            var p = decodeURIComponent(String(selectedFile).replace(/^file:\/\//, ""));
            root.addChip({ kind: "file", label: p.split("/").pop(), path: p });
        }
    }
}
