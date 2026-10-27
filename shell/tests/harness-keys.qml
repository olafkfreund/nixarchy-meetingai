import QtQuick
import Quickshell

// For tests/keys.sh: the overlay in interviewer mode with the Candidate tab open; Tab is
// stepped through the focus chain and every stop is logged as "KEY <n> <item>".
ShellRoot {
    id: root
    property int n: 0
    Overlay {
        id: overlay
        Component.onCompleted: overlay.open("{}")
    }
    Timer { interval: 2500; running: true; onTriggered: console.log("KEY open " + overlay.focusProbe(false)) }   // the composer has the focus on open
    Timer { interval: 3000; running: true; onTriggered: overlay.showLibrary("candidate") }
    Timer {
        interval: 120; repeat: true; running: false
        id: stepper
        onTriggered: { root.n++; console.log("KEY " + root.n + " " + overlay.focusProbe(true)); if (root.n >= 150) stepper.stop(); }
    }
    Timer { interval: 5000; running: true; onTriggered: { overlay.focusStart(); stepper.start(); } }
}
