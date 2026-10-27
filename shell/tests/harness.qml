import QtQuick
import Quickshell

// Loads the service, the overlay (which also builds Library and Composer) and
// the bar widget with its menu open. smoke.sh installs this as shell.qml.
ShellRoot {
    Service {}
    Overlay {
        id: overlay
        Component.onCompleted: overlay.open("{}")
    }
    QtObject {   // stand-in for the shell's Bar host
        id: barStub
        property string position: "top"
        property bool vertical: false
        property int barSize: 32
        property var activePopout: null
        property string fontFamily: "monospace"
        property color barForeground: "#dddddd"
        property color urgent: "#ff5555"
        property bool foregroundAnimationEnabled: false
        function showTooltip(w, t) {}
        function hideTooltip(w) {}
        function run(cmd) {}
        function requestPopout(k) { activePopout = k; }
        function releasePopout(k) { activePopout = null; }
    }
    PanelWindow { // qmllint disable uncreatable-type
        anchors { top: true; left: true; right: true }
        implicitHeight: 32
        color: "#202020"
        BarWidget {
            bar: barStub
            anchors.fill: parent
            Component.onCompleted: menuOpen = true
        }
    }
}
