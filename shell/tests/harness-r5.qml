import QtQuick
import Quickshell

// Round 5 frames for tests/live-shots.sh (SCENARIO=r5): resized card, Profiles tab, Prompts tab.
ShellRoot {
    Overlay {
        id: overlay
        Component.onCompleted: overlay.open("{}")
    }
    Timer { interval: 8000; running: true; onTriggered: overlay.showLibrary("profiles") }
    Timer { interval: 12000; running: true; onTriggered: overlay.showLibrary("prompts") }
}
