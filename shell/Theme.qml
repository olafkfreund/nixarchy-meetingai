// qmllint disable missing-property
// ^ qs.Commons exposes nested QtObjects (Style.spacing, Style.font) the linter cannot type
import QtQuick
import qs.Commons

// One metric for the whole overlay. Overlay creates a single instance (`ts` = the
// text-size setting) and hands it down as `theme`; a plugin folder is not a
// reliable place for a QML singleton, so this is a plain object, not one.
// Every form row is a RowLayout with Layout.preferredHeight: theme.ctl.
QtObject {
    property real ts: 1

    readonly property real ctl: Math.round(Style.spacing.controlHeight * ts)   // same formula as IconButton/Input
    readonly property real gap: Style.spacing.controlGap * ts                  // between controls in a row
    readonly property real padX: Style.space(16)                               // card/bubble padding
    readonly property real padY: Style.space(12)
    readonly property real radius: Math.max(Style.cornerRadius, 12)            // Hyprland rounding is 6
    readonly property real section: Style.space(14)                            // between transcript rows / form sections
    readonly property real leading: 1.3                                        // reading line height
    readonly property string family: Style.font.family
    readonly property real caption: Style.font.caption * ts
    readonly property real bodySmall: Style.font.bodySmall * ts
    readonly property real body: Style.font.body * ts
    readonly property real subtitle: Style.font.subtitle * ts
    readonly property real title: Style.font.title * ts
    readonly property real nowBody: Style.font.body * 1.15 * ts   // the live answer: the biggest text on the card
}
