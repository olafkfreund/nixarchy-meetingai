pragma ComponentBehavior: Bound
// qmllint disable missing-property
import QtQuick
import qs.Commons // qmllint disable import
import qs.Commons as Commons // qmllint disable import

// Small text button used across the overlay.
Rectangle {
    id: root
    property string label: ""
    property bool on: false
    signal clicked

    implicitWidth: txt.implicitWidth + Style.spacing.lg * 2
    implicitHeight: txt.implicitHeight + Style.spacing.md * 2
    radius: Style.cornerRadius
    color: root.on || area.containsMouse ? Commons.Color.menu.selectedBackground : "transparent"
    border.color: Commons.Color.menu.border
    border.width: 1

    Text {
        id: txt
        anchors.centerIn: parent
        textFormat: Text.PlainText
        text: root.label
        color: root.on ? Commons.Color.menu.selectedText : Commons.Color.menu.text
        font.pixelSize: Style.font.caption
    }
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        onClicked: root.clicked()
    }
}
