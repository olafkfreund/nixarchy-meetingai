pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects the linter cannot type
import QtQuick
import qs.Ui

// qs.Ui ToggleSwitch as a Tab stop: Space or Enter toggles, and the focus shows as the switch's
// cursor ring. As with the switch, the caller owns `checked` and flips it on `toggled()`.
FocusScope {
    id: root

    property bool checked: false
    property string accessibleName: ""
    signal toggled()

    implicitWidth: sw.implicitWidth
    implicitHeight: sw.implicitHeight
    activeFocusOnTab: true
    Accessible.role: Accessible.CheckBox
    Accessible.name: root.accessibleName
    Accessible.checked: root.checked
    Accessible.onToggleAction: root.toggled()
    readonly property string accName: root.accessibleName
    Keys.onSpacePressed: root.toggled()
    Keys.onReturnPressed: root.toggled()
    Keys.onEnterPressed: root.toggled()

    ToggleSwitch {
        id: sw
        anchors.fill: parent
        checked: root.checked
        hasCursor: root.activeFocus
        onToggled: { root.forceActiveFocus(); root.toggled(); }
    }
}
