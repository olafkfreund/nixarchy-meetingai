import QtQuick
import Quickshell

// For tests/smoke.sh: the overlay against fake-daemon.py --reask. It connects while turn 1 streams,
// asks a new question, and turn 1's late delta arrives before answer.started: it must not take NOW.
ShellRoot {
    Overlay {
        id: overlay
        Component.onCompleted: overlay.open("{}")
    }
    Timer { interval: 2000; running: true; onTriggered: overlay.submit("new?", []) }
    Timer { interval: 4000; running: true; onTriggered: console.log("REASK " + overlay.dropDump()) }
}
