import QtQuick
import Quickshell
import Quickshell.Wayland

// Drives the overlay and bar menu through the states tests/shots.sh photographs.
ShellRoot {
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
        anchors { top: true; bottom: true; left: true; right: true }   // like the overlay, so it maps the same way
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        color: "transparent"
        BarWidget {
            bar: barStub
            id: bar
            x: 400; y: 200; width: 48; height: 32
        }
    }
    Timer { interval: 2000; running: true; onTriggered: overlay.openModelPicker() }
    Timer { interval: 4500; running: true; onTriggered: { overlay.showLibrary("settings"); } }
    Timer { interval: 7000; running: true; onTriggered: { overlay.close(); bar.menuOpen = true; } }
}
