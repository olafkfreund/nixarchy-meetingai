// qmllint disable missing-property
// ^ qs.Commons exposes nested QtObjects the linter cannot type
import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Commons as Commons

// A command to copy, and the button that copies it (Setup card, Providers tab).
RowLayout {
    id: root

    required property var theme
    property string cmd

    spacing: root.theme.gap
    TextInput {
        id: field
        Layout.fillWidth: true
        Layout.alignment: Qt.AlignVCenter
        readOnly: true
        selectByMouse: true
        text: root.cmd
        color: Commons.Color.popups.text
        font.family: Style.font.family
        font.pixelSize: root.theme.caption
        Accessible.name: "Command: " + root.cmd
    }
    IconButton { objectName: "copy"; ts: root.theme.ts; Layout.alignment: Qt.AlignVCenter; text: "Copy"; bordered: true; tooltipText: "Copy the command"; onClicked: { field.selectAll(); field.copy(); field.deselect(); } }
}
