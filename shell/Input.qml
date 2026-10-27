// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects the linter cannot type
import QtQuick
import qs.Commons
import qs.Ui

// qs.Ui TextField at the overlay's text size and the shared control height.
TextField {
    property real ts: 1
    property string accessibleName: ""   // for fields without a placeholder
    Accessible.role: Accessible.EditableText
    Accessible.name: accessibleName !== "" ? accessibleName : placeholderText
    readonly property string accName: accessibleName !== "" ? accessibleName : placeholderText
    font.pixelSize: Style.font.body * ts
    verticalAlignment: TextInput.AlignVCenter
    implicitHeight: Math.round(Style.spacing.controlHeight * ts)   // = Theme.ctl
    verticalPadding: 0
    // a text set from code leaves the cursor at its end and scrolls the start out of view
    onTextChanged: { if (!activeFocus) cursorPosition = 0; }
}
