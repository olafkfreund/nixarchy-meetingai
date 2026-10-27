import QtQuick
import Quickshell

// Loads the service and the overlay (which also builds Library and Composer).
// smoke.sh installs this as shell.qml next to the plugin files.
ShellRoot {
    Service {}
    Overlay {
        id: overlay
        Component.onCompleted: overlay.open("{}")
    }
}
