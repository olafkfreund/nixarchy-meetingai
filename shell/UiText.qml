// qmllint disable missing-property
// ^ qs.Commons exposes Style.font as a nested QtObject the linter cannot type
import QtQuick
import qs.Commons

// Text in the shell's font (Style.font.family), so labels, bubbles and answers match qs.Ui controls.
Text {
    font.family: Style.font.family
}
