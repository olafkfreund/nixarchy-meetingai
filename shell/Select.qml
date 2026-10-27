pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects the linter cannot type
import QtQuick
import QtQuick.Controls
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// A single-select list in the overlay's text size and control height. qs.Ui Dropdown and
// SearchableDropdown hard-code Style.font.body and cannot follow the text-size setting, so
// they would be smaller than the fields beside them. `options`: string[] or {value, label}[].
// `searchable` adds a filter field (long model lists).
Item {
    id: root

    required property var theme
    property string value: ""
    property var options: []
    property string triggerLabel: ""   // shown while `value` is empty
    property bool searchable: false
    property string query: ""
    property string accessibleName: "Choice"
    readonly property string accName: trigger.Accessible.name
    readonly property real fontPx: root.theme.body   // tests/geometry.js compares it with the fields in the same row
    readonly property bool popupOpen: popup.opened
    readonly property string iChevron: String.fromCodePoint(0xf0140)

    signal changed(string value)

    function optionValue(o) { return (o && typeof o === "object") ? String(o.value) : String(o); }
    function optionLabel(o) { return (o && typeof o === "object") ? String(o.label) : String(o); }
    function currentLabel() {
        for (var i = 0; i < root.options.length; i++)
            if (root.optionValue(root.options[i]) === root.value) return root.optionLabel(root.options[i]);
        return root.value !== "" ? root.value : root.triggerLabel;
    }
    readonly property var shown: root.searchable && root.query !== "" ? root.options.filter(function(o) { return root.optionLabel(o).toLowerCase().indexOf(root.query.toLowerCase()) >= 0; }) : root.options
    function pick(i) {
        if (i < 0 || i >= root.shown.length) return;
        root.changed(root.optionValue(root.shown[i]));
        popup.close();
    }
    function key(e) {   // Up/Down/Enter/Escape in the list, from the list or the filter field
        if (e.key === Qt.Key_Escape) { popup.close(); e.accepted = true; }
        else if (e.key === Qt.Key_Down) { list.currentIndex = Math.min(root.shown.length - 1, list.currentIndex + 1); e.accepted = true; }
        else if (e.key === Qt.Key_Up) { list.currentIndex = Math.max(0, list.currentIndex - 1); e.accepted = true; }
        else if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter) { root.pick(list.currentIndex); e.accepted = true; }
    }

    implicitWidth: Math.round(Style.space(240) * root.theme.ts)
    implicitHeight: root.theme.ctl

    BorderSurface {
        id: trigger
        anchors.fill: parent
        radius: Style.cornerRadius
        readonly property bool hot: hover.hovered
        color: Style.controlFill(trigger.activeFocus, trigger.hot, Commons.Color.popups.text, Commons.Color.accent)
        borderSpec: Border.controlSpec(trigger.activeFocus ? "focus" : (trigger.hot ? "hover-cursor" : "normal"), Commons.Color.popups.text, Commons.Color.accent)
        activeFocusOnTab: true
        Accessible.role: Accessible.ComboBox
        Accessible.name: root.accessibleName + ": " + root.currentLabel()
        HoverHandler { id: hover }
        Keys.onPressed: function(e) {
            if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter || e.key === Qt.Key_Space || e.key === Qt.Key_Down) { popup.open(); e.accepted = true; }
        }
        UiText {
            anchors.left: parent.left; anchors.right: chevron.left
            anchors.verticalCenter: parent.verticalCenter
            anchors.leftMargin: trigger.borderLeft + Style.spacing.controlPaddingX
            anchors.rightMargin: Style.spacing.md
            textFormat: Text.PlainText
            text: root.currentLabel()
            elide: Text.ElideRight
            color: Commons.Color.popups.text
            font.pixelSize: root.fontPx
        }
        UiText {
            id: chevron
            anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
            anchors.rightMargin: trigger.borderRight + Style.spacing.controlGap
            textFormat: Text.PlainText
            text: root.iChevron
            color: Qt.darker(Commons.Color.popups.text, 1.2)
            font.pixelSize: root.fontPx
        }
        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: { trigger.forceActiveFocus(); if (popup.opened) popup.close(); else popup.open(); }
        }
    }

    Popup {
        id: popup
        x: 0
        y: root.height + Style.spacing.xxs
        width: root.width
        padding: 1
        focus: true
        onOpened: {
            root.query = "";
            list.currentIndex = Math.max(0, root.shown.findIndex(function(o) { return root.optionValue(o) === root.value; }));
            if (root.searchable) filter.forceActiveFocus(); else list.forceActiveFocus();
        }
        background: BorderSurface {
            color: Commons.Color.popups.background
            borderSpec: Border.localOrSurfaceSpec("popups", "border", Commons.Color.popups.border, Commons.Color.popups.border, Style.normalBorderWidth)
            radius: Style.cornerRadius
        }
        contentItem: Column {
            Input {
                id: filter
                ts: root.theme.ts
                visible: root.searchable
                width: parent.width
                placeholderText: "Search"
                onTextChanged: { root.query = text; list.currentIndex = 0; }
                Keys.priority: Keys.BeforeItem
                Keys.onPressed: function(e) { root.key(e); }
            }
            ListView {
                id: list
                width: parent.width
                height: Math.min(root.shown.length, 8) * root.theme.ctl
                clip: true
                model: root.shown
                currentIndex: -1
                boundsBehavior: Flickable.StopAtBounds
                Keys.onPressed: function(e) { root.key(e); }
                delegate: Rectangle {
                    id: opt
                    required property var modelData
                    required property int index
                    width: list.width
                    height: root.theme.ctl
                    color: opt.index === list.currentIndex ? Style.hoverFillFor(Commons.Color.popups.text, Commons.Color.accent) : "transparent"
                    UiText {
                        anchors.fill: parent
                        anchors.leftMargin: Style.spacing.controlPaddingX; anchors.rightMargin: Style.spacing.controlPaddingX
                        verticalAlignment: Text.AlignVCenter
                        textFormat: Text.PlainText
                        text: root.optionLabel(opt.modelData)
                        elide: Text.ElideRight
                        color: Commons.Color.popups.text
                        font.pixelSize: root.fontPx
                    }
                    MouseArea {
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onPositionChanged: list.currentIndex = opt.index
                        onClicked: root.pick(opt.index)
                    }
                }
            }
        }
    }
}
