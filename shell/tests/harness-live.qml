import QtQuick
import Quickshell
import Quickshell.Wayland

// Overlay plus bar pill for tests/live-shots.sh, against fake-daemon.py --live.
ShellRoot {
    Overlay {
        id: overlay
        Component.onCompleted: overlay.open("{}")
    }
    Timer { interval: 12000; running: true; onTriggered: overlay.showLibrary("settings") }
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
        anchors { top: true; left: true }
        implicitWidth: 120; implicitHeight: 32
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        color: "transparent"
        BarWidget { bar: barStub; anchors.fill: parent }
    }
}
