pragma ComponentBehavior: Bound
// qmllint disable missing-property
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import qs.Commons // qmllint disable import
import qs.Commons as Commons // qmllint disable import
import qs.Ui // qmllint disable import

// Question box. Grows to 5 lines then scrolls; Enter sends, Shift+Enter adds
// a line. Code and large pastes become chips; files go as paths.
Item {
    id: root

    readonly property int bigPaste: 2000
    property var chips: []   // { kind: "text"|"file", label, text?, path? }
    signal submit(string text, var chips)
    signal preview(string text)

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
                    Button {
                        text: chip.modelData.label
                        bordered: true
                        onClicked: if (chip.modelData.kind === "text") root.preview(chip.modelData.text)
                    }
                    PanelActionButton {
                        iconText: "\u00d7"
                        hoverColor: Commons.Color.urgent
                        onClicked: root.chips = root.chips.filter(function(c, i) { return i !== chip.index; })
                    }
                }
            }
        }

        Row {
            width: parent.width
            spacing: Style.spacing.xs

            Button { text: "+"; bordered: true; onClicked: files.open() }

            ScrollView {
                width: parent.width - Style.space(120)
                height: Math.min(input.implicitHeight, fm.height * 5 + Style.spacing.md * 2)

                TextArea {
                    id: input
                    wrapMode: TextEdit.Wrap
                    placeholderText: "Ask about the meeting"
                    color: Commons.Color.menu.text
                    placeholderTextColor: Qt.alpha(Commons.Color.menu.text, 0.5)
                    font.pixelSize: Style.font.body
                    background: BorderSurface {
                        color: Style.controlFill(input.activeFocus, input.hovered, Commons.Color.foreground, Commons.Color.accent)
                        borderSpec: Border.controlSpec(input.activeFocus ? "focus" : (input.hovered ? "hover-cursor" : "normal"), Commons.Color.foreground, Commons.Color.accent)
                        radius: Style.cornerRadius
                    }
                    Keys.onPressed: function(e) {
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

            Button { text: "Send"; active: input.text.trim() !== "" || root.chips.length > 0; bordered: true; onClicked: root.send() }
        }
    }

    FontMetrics { id: fm; font.pixelSize: Style.font.body }

    FileDialog {
        id: files
        onAccepted: {
            var p = decodeURIComponent(String(selectedFile).replace(/^file:\/\//, ""));
            root.addChip({ kind: "file", label: p.split("/").pop(), path: p });
        }
    }
}
