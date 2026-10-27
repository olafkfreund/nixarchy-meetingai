pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects (Style.spacing, Color.popups) the linter cannot type
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import qs.Commons
import qs.Commons as Commons

// Question box. Grows to 5 lines then scrolls; Enter sends, Shift+Enter adds
// a line. Code and large pastes become chips; files go as paths.
Item {
    id: root

    required property var theme
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
    property bool team: false      // team meeting mode: show the series picker
    property var seriesList: []    // series.list rows
    property string seriesId: ""
    property bool seriesLocked: false
    property string seriesError: ""    // the last failed series change, shown under the picker
    signal seriesPicked(string id)         // "" = no series
    signal seriesCreated(string name)
    signal pick(int n)   // 1..3 typed into an empty composer: choose that suggestion
    readonly property bool empty: input.text.trim() === "" && root.chips.length === 0

    implicitHeight: col.implicitHeight

    function addChip(chip) { root.chips = root.chips.concat([chip]); }

    function focusInput() { input.forceActiveFocus(); }

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
                    IconButton { ts: root.theme.ts;
                        text: chip.modelData.label
                        bordered: true
                        onClicked: if (chip.modelData.kind === "text") root.preview(chip.modelData.text)
                    }
                    IconButton {
                        ts: root.theme.ts
                        text: "\u00d7"
                        tooltipText: "Remove"
                        Accessible.name: "Remove " + chip.modelData.label
                        onClicked: root.chips = root.chips.filter(function(c, i) { return i !== chip.index; })
                    }
                }
            }
        }

        RowLayout {   // series: only in team mode; "New series..." reveals a name field
            visible: root.team
            width: parent.width
            spacing: root.theme.gap
            Keys.onEscapePressed: function(e) { if (newName.visible) { newName.text = ""; newName.visible = false; e.accepted = true; } else e.accepted = false; }   // cancels the name row first
            UiText { Layout.alignment: Qt.AlignVCenter; textFormat: Text.PlainText; text: "Series"; color: Commons.Color.popups.text; font.pixelSize: root.theme.caption; font.bold: true }
            Select {
                id: seriesBox
                theme: root.theme
                objectName: "seriesPick"
                accessibleName: "Series"
                enabled: !root.seriesLocked
                Layout.preferredWidth: Math.round(Style.space(240) * root.theme.ts)
                Layout.preferredHeight: root.theme.ctl
                Layout.alignment: Qt.AlignVCenter
                value: root.seriesId
                options: [{ value: "", label: "No series" }].concat(root.seriesList.map(function(s) { return { value: String(s.id), label: String(s.name) }; })).concat([{ value: "__new", label: "New series..." }])
                onChanged: function(v) { if (v === "__new") newName.visible = true; else { newName.visible = false; root.seriesPicked(v); } }
            }
            Input {
                id: newName
                objectName: "seriesName"
                ts: root.theme.ts
                visible: false
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignVCenter
                placeholderText: "Series name"
                onAccepted: { if (text.trim() !== "") root.seriesCreated(text.trim()); text = ""; visible = false; }
            }
            Item { Layout.fillWidth: !newName.visible }
        }

        UiText { visible: root.team && root.seriesError !== ""; width: parent.width; textFormat: Text.PlainText; wrapMode: Text.Wrap; text: "Error: " + root.seriesError; color: Commons.Color.popups.text; font.pixelSize: root.theme.caption }

        IconButton {   // quick action, above the field
            ts: root.theme.ts
            iconText: root.iBulb
            objectName: "quick"
            text: "What should I say?"
            tooltipText: "Ask for a suggested reply to the latest question"
            onClicked: root.quick()
        }

        Rectangle {   // one rounded field: attach and screenshot on the left, send on the right
            width: parent.width
            height: Math.max(field.height, root.theme.ctl) + Style.spacing.xs * 2
            radius: root.theme.radius
            color: Style.controlFill(input.activeFocus, input.hovered, Commons.Color.foreground, Commons.Color.accent)
            border.width: Style.normalBorderWidth
            border.color: Qt.alpha(Commons.Color.popups.border, input.activeFocus ? 1 : 0.5)

            IconButton { id: attach; objectName: "attach"; ts: root.theme.ts; x: Style.spacing.xs; anchors.bottom: parent.bottom; anchors.bottomMargin: Style.spacing.xs; iconText: root.iAttach; tooltipText: "Attach a file"; onClicked: files.open() }
            IconButton { id: shot; objectName: "shot"; ts: root.theme.ts; anchors.left: attach.right; anchors.bottom: attach.bottom; iconText: root.iCamera; tooltipText: "Capture a screenshot region"; onClicked: root.screenshot() }
            IconButton { id: send; objectName: "send"; ts: root.theme.ts; anchors.right: parent.right; anchors.rightMargin: Style.spacing.xs; anchors.bottom: attach.bottom; iconText: root.iSend; active: !root.empty; tooltipText: "Send (Enter)"; onClicked: root.send() }

            ScrollView {
                id: field
                anchors.left: shot.right; anchors.right: send.left
                anchors.verticalCenter: parent.verticalCenter
                height: Math.min(input.implicitHeight, fm.height * 5 + Style.spacing.md * 2)

                TextArea {
                    id: input
                    objectName: "composerInput"
                    Accessible.name: "Ask about the meeting"
                    wrapMode: TextEdit.Wrap
                    placeholderText: "Ask about the meeting"
                    color: Commons.Color.popups.text
                    font.family: Style.font.family
                    placeholderTextColor: Qt.alpha(Commons.Color.popups.text, 0.5)
                    font.pixelSize: root.theme.body
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

    FontMetrics { id: fm; font.pixelSize: root.theme.body }

    FileDialog {
        id: files
        onAccepted: {
            var p = decodeURIComponent(String(selectedFile).replace(/^file:\/\//, ""));
            root.addChip({ kind: "file", label: p.split("/").pop(), path: p });
        }
    }
}
