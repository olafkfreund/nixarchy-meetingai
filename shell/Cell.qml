// qmllint disable missing-property
// ^ qs.Commons exposes nested QtObjects the linter cannot type
import QtQuick
import qs.Commons
import qs.Commons as Commons

// A captioned control in a Flow of settings: the caption line has a fixed height and the
// control slot is one control tall, so every cell in a row lines up whatever it holds.
Column {
    id: root

    required property var theme
    property string label: ""
    default property alias content: slot.data

    spacing: Style.spacing.xs
    UiText {
        width: parent.width
        height: Math.round(Style.font.caption * root.theme.ts * 1.5)
        verticalAlignment: Text.AlignVCenter
        textFormat: Text.PlainText
        text: root.label
        elide: Text.ElideRight
        color: Qt.darker(Commons.Color.popups.text, 1.4)
        font.pixelSize: root.theme.caption
        font.bold: true
    }
    Item { id: slot; width: parent.width; height: root.theme.ctl }
}
