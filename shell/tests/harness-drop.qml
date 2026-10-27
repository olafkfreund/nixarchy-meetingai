import QtQuick
import Quickshell

// For tests/smoke.sh: the overlay against fake-daemon.py --drop. The connection is cut mid-answer,
// the client reconnects, and the transcript is dumped once the answer has finished.
ShellRoot {
    Overlay {
        id: overlay
        Component.onCompleted: overlay.open("{}")
    }
    Timer { interval: 11000; running: true; onTriggered: console.log("DROP " + overlay.dropDump()) }
}
