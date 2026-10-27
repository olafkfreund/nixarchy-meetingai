import QtQuick
import Quickshell

// For tests/smoke.sh: the overlay against fake-daemon.py --stale. Turn 2 starts, then the late tail of
// turn 1 arrives; NOW must hold turn 2's text only.
ShellRoot {
    Overlay {
        id: overlay
        Component.onCompleted: overlay.open("{}")
    }
    Timer { interval: 4000; running: true; onTriggered: console.log("STALE " + overlay.dropDump()) }
}
