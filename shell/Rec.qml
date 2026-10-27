// qmllint disable missing-property
// ^ qs.Commons exposes nested QtObjects (Color.popups) the linter cannot type
import QtQuick
import qs.Commons as Commons

// The "recording" red: a vivid #e5484d when it reads against `background` at 3:1 (non-text
// contrast), else the theme's urgent colour, else the foreground. tests/contrast.py mirrors the rule.
QtObject {
    id: root

    property color background: Commons.Color.popups.background
    readonly property color fallback: "#e5484d"
    readonly property color red: root.ratio(root.fallback, root.background) >= 3 ? root.fallback
        : root.ratio(Commons.Color.urgent, root.background) >= 3 ? Commons.Color.urgent : Commons.Color.foreground

    function lum(c) {
        var l = [c.r, c.g, c.b].map(function(x) { return x <= 0.03928 ? x / 12.92 : Math.pow((x + 0.055) / 1.055, 2.4); });
        return 0.2126 * l[0] + 0.7152 * l[1] + 0.0722 * l[2];
    }
    function ratio(a, b) {
        var x = root.lum(a), y = root.lum(b);
        return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
    }
}
