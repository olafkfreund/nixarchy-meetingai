import QtQuick
import Quickshell

// Drives the overlay through the states tests/shots.sh photographs.
ShellRoot {
    Overlay {
        id: overlay
        Component.onCompleted: overlay.open("{}")
    }
    Timer { interval: 2000; running: true; onTriggered: overlay.openModelPicker() }
    Timer { interval: 4000; running: true; onTriggered: { overlay.showLibrary("settings"); } }
}
