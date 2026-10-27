// qmllint disable missing-property
// ^ qs.Commons exposes Style.font as a nested QtObject the linter cannot type
import QtQuick
import qs.Commons

// Text in the shell's font (Style.font.family), so labels, bubbles and answers match qs.Ui controls.
Text {
    property bool mayCut: false   // tests/check-geometry.py fails text that is cut off, unless the design cuts it
    font.family: Style.font.family
    height: Math.max(implicitHeight, contentHeight)   // rich text with a line height paints a few px past its implicit height
}
