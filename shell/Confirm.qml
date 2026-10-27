pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects the linter cannot type
import QtQuick
import qs.Commons
import qs.Commons as Commons

// The overlay's confirmation dialog: covers the whole card, text at the overlay's size.
// (qs.Ui ConfirmDialog cannot follow the text-size setting.) Cancel has the focus first;
// Tab and the arrow keys move between the two buttons, Escape cancels.
FocusScope {
    id: root

    required property var theme
    property bool opened: false
    property string message: ""
    property string confirmText: "Confirm"
    property var yes: null

    function ask(message, confirmText, yes) {
        root.message = message; root.confirmText = confirmText; root.yes = yes;
        root.opened = true;
        cancel.forceActiveFocus();
    }
    function close() { root.opened = false; root.yes = null; }

    visible: opened
    Keys.onPressed: function(e) {
        if (e.key === Qt.Key_Escape) { root.close(); e.accepted = true; }
        else if (e.key === Qt.Key_Tab || e.key === Qt.Key_Backtab || e.key === Qt.Key_Left || e.key === Qt.Key_Right) {
            if (cancel.activeFocus) ok.forceActiveFocus(); else cancel.forceActiveFocus();
            e.accepted = true;
        }
    }

    Rectangle {
        anchors.fill: parent
        radius: root.theme.radius
        color: Qt.alpha(Commons.Color.background, 0.75)
        MouseArea { anchors.fill: parent; onClicked: root.close() }
    }
    Rectangle {
        anchors.centerIn: parent
        width: Math.min(parent.width - Style.space(40), Math.round(Style.space(440) * root.theme.ts))
        height: col.implicitHeight + root.theme.padY * 2
        radius: root.theme.radius
        color: Qt.rgba(Commons.Color.popups.background.r, Commons.Color.popups.background.g, Commons.Color.popups.background.b, 1)
        border.width: Style.normalBorderWidth
        border.color: Commons.Color.accent
        Accessible.role: Accessible.Dialog
        Accessible.name: root.message
        MouseArea { anchors.fill: parent }
        Column {
            id: col
            x: root.theme.padX; y: root.theme.padY
            width: parent.width - root.theme.padX * 2
            spacing: root.theme.section
            UiText { width: parent.width; textFormat: Text.PlainText; wrapMode: Text.Wrap; text: root.message; color: Commons.Color.popups.text; font.pixelSize: root.theme.body; lineHeight: root.theme.leading }
            Row {
                anchors.right: parent.right
                spacing: root.theme.gap
                IconButton { id: cancel; objectName: "confirmCancel"; ts: root.theme.ts; text: "Cancel"; bordered: true; onClicked: root.close() }
                IconButton { id: ok; objectName: "confirmOk"; ts: root.theme.ts; text: root.confirmText; bordered: true; foreground: Commons.Color.urgent; onClicked: { var f = root.yes; root.close(); if (f) f(); } }
            }
        }
    }
}
