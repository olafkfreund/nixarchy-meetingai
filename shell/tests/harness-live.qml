import QtQuick
import Quickshell
import Quickshell.Wayland
import qs.Commons as Commons
import "geometry.js" as Geo

// Overlay plus bar pill for tests/live-shots.sh, against fake-daemon.py --live.
ShellRoot {
    Overlay {
        id: overlay
        Component.onCompleted: overlay.open("{}")
    }
    Timer { interval: 19500; running: overlay.iv; onTriggered: overlay.candidateHook("new") }   // interviewer: inline rows, then the delete dialog
    Timer { interval: 21500; running: overlay.iv; onTriggered: overlay.candidateHook("attach") }
    Timer { interval: 23500; running: overlay.iv; onTriggered: overlay.candidateHook("delete") }
    Timer { interval: 13000; running: Quickshell.env("MEETINGAI_ASK") === "1"; onTriggered: overlay.submit("What should I say?", []) }   // SCENARIO=error: a question that fails
    Timer { interval: 19000; running: !overlay.iv && Quickshell.env("MEETINGAI_ASK") !== "1"; onTriggered: overlay.modeHook() }
    Timer { interval: 1500; repeat: true; running: Quickshell.env("MEETINGAI_GEOMETRY") === "1"; onTriggered: Geo.dump(overlay.geometryRoot(), overlay.ctl, "", overlay.winW, overlay.winH) }   // tests/check-geometry.py
    Timer { interval: 10000; running: true; onTriggered: overlay.lastOpen = true }   // team scenario: Last time drawer
    Timer { interval: 2000; running: overlay.iv; onTriggered: overlay.recordHook(false) }   // interviewer: the consent banner
    Timer { interval: 6000; running: overlay.iv; onTriggered: overlay.recordHook(true) }
    Timer { interval: 16000; running: Quickshell.env("MEETINGAI_ASK") !== "1"; onTriggered: overlay.showLibrary(overlay.iv ? "candidate" : "settings") }
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
        implicitWidth: 160; implicitHeight: 36
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        color: "transparent"
        Rectangle { anchors.fill: parent; color: Commons.Color.bar.background }
        BarWidget { bar: barStub; anchors.fill: parent }
    }
}
