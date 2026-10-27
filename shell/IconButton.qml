// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects the linter cannot type
import QtQuick
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// qs.Ui Button with the overlay's text colour and text scale. `iconText` is a
// Nerd Font Material Design glyph (monochrome); give every icon button a tooltipText.
Button {
    property real ts: 1
    foreground: Commons.Color.popups.text
    fontSize: Style.font.body * ts
    iconSize: Style.font.title * ts
    property bool elidable: false   // a list entry whose label may be cut when the column is narrow
    focusable: true   // Tab reaches every button; Enter/Space press it
    Accessible.role: Accessible.Button
    Accessible.name: text !== "" ? text : tooltipText
    Accessible.onPressAction: clicked()
    readonly property string accName: Accessible.name   // tests/geometry.js: no control without a name
    implicitHeight: Math.round(Style.spacing.controlHeight * ts)   // one height with or without an icon (= Theme.ctl)
}
