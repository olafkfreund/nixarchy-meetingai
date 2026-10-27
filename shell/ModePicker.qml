pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects the linter cannot type
import QtQuick
import QtQuick.Controls
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// The meeting mode as one compact button: it shows the current name and opens a list of the
// four modes with a one-line description each. Values stay general | interview | team_meeting | interviewer.
Item {
    id: root

    required property var daemon
    required property var theme
    required property Item bounds   // the popup stays inside it (the header)
    property real popupWidth: 400
    readonly property var modes: [
        { value: "general", name: "Assist", hint: "Answers and notes for any call" },
        { value: "interview", name: "Interview me", hint: "You are being interviewed: answer help" },
        { value: "team_meeting", name: "Team", hint: "Action items, decisions and series" },
        { value: "interviewer", name: "Interview someone", hint: "Question bank and evidence notes" }
    ]
    readonly property string mode: String(root.daemon.settings.meeting_mode || "general")
    readonly property var cur: root.modes.find(function(m) { return m.value === root.mode; }) || root.modes[0]
    readonly property bool popupOpen: popup.opened
    readonly property Item popupContent: popup.contentItem   // tests/geometry.js walks an open popup
    function closePopup() { popup.close(); }

    implicitWidth: btn.implicitWidth
    implicitHeight: btn.implicitHeight

    function open() { popup.open(); }

    IconButton {
        id: btn
        objectName: "modePicker"
        anchors.fill: parent
        ts: root.theme.ts
        text: root.cur.name
        iconText: String.fromCodePoint(0xf0140)
        bordered: true
        tooltipText: "Meeting mode: " + root.cur.hint
        Accessible.name: "Meeting mode: " + root.cur.name
        onClicked: { if (popup.opened) popup.close(); else popup.open(); }
    }

    Popup {
        id: popup
        x: root.width - width
        y: root.height + Style.spacing.xxs
        width: root.popupWidth
        onAboutToShow: root.popupWidth = Math.min(Math.round(Style.space(420) * root.theme.ts), root.mapToItem(root.bounds, root.width, 0).x)
        padding: 1
        focus: true
        onOpened: { list.currentIndex = Math.max(0, root.modes.indexOf(root.cur)); list.forceActiveFocus(); }
        background: BorderSurface {
            color: Commons.Color.popups.background
            borderSpec: Border.localOrSurfaceSpec("popups", "border", Commons.Color.popups.border, Commons.Color.popups.border, Style.normalBorderWidth)
            radius: Style.cornerRadius
        }
        contentItem: ListView {
            id: list
            implicitHeight: contentHeight
            interactive: false
            model: root.modes
            currentIndex: -1
            function choose(i) { if (i < 0) return; root.daemon.setSetting("meeting_mode", root.modes[i].value); popup.close(); }
            Keys.onPressed: function(e) {
                if (e.key === Qt.Key_Down) { currentIndex = Math.min(root.modes.length - 1, currentIndex + 1); e.accepted = true; }
                else if (e.key === Qt.Key_Up) { currentIndex = Math.max(0, currentIndex - 1); e.accepted = true; }
                else if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter || e.key === Qt.Key_Space) { choose(currentIndex); e.accepted = true; }
            }
            delegate: Rectangle {
                id: row
                required property var modelData
                required property int index
                width: list.width
                height: root.theme.ctl + Math.round(root.theme.caption * 1.5)
                color: row.index === list.currentIndex ? Style.hoverFillFor(Commons.Color.popups.text, Commons.Color.accent) : "transparent"
                Accessible.role: Accessible.ListItem
                Accessible.name: row.modelData.name + ": " + row.modelData.hint
                Rectangle { visible: row.modelData.value === root.cur.value; width: Style.space(3); height: parent.height - Style.spacing.lg; anchors.verticalCenter: parent.verticalCenter; color: Commons.Color.accent }
                Column {
                    anchors.left: parent.left; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    anchors.leftMargin: Style.spacing.controlPaddingX; anchors.rightMargin: Style.spacing.controlPaddingX
                    UiText { width: parent.width; textFormat: Text.PlainText; elide: Text.ElideRight; text: row.modelData.name; color: Commons.Color.popups.text; font.pixelSize: root.theme.body; font.bold: true }
                    UiText { width: parent.width; textFormat: Text.PlainText; elide: Text.ElideRight; text: row.modelData.hint; color: Commons.Color.popups.text; opacity: root.theme.dim; font.pixelSize: root.theme.caption }
                }
                MouseArea { anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onPositionChanged: list.currentIndex = row.index; onClicked: list.choose(row.index) }
            }
        }
    }
}
