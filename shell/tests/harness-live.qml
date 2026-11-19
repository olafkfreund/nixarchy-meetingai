import QtQuick
import Quickshell
import Quickshell.Wayland
import qs.Commons as Commons
import "geometry.js" as Geo

// Overlay plus bar pill for tests/live-shots.sh, against fake-daemon.py --live.
ShellRoot {
    id: root
    readonly property string scenario: Quickshell.env("SCENARIO")
    property var snap: ({})
    property string lastWin: ""
    function find(it, name, depth) {
        if (!it || !it.visible || (depth || 0) > 60) return null;
        if (it.objectName === name) return it;
        var kids = it.children || [];
        for (var i = 0; i < kids.length; i++) { var f = find(kids[i], name, (depth || 0) + 1); if (f) return f; }
        return null;
    }
    function texts(it, depth) {
        if (!it || !it.visible || (depth || 0) > 60) return "";
        var s = typeof it.text === "string" ? it.text : "", kids = it.children || [];
        for (var i = 0; i < kids.length; i++) { var t = texts(kids[i], (depth || 0) + 1); if (t) s = s ? s + " " + t : t; }
        return s;
    }
    function click(it) {
        if (!it) return;
        var kids = it.children || [];
        for (var i = 0; i < kids.length; i++) if (typeof kids[i].clicked === "function") { kids[i].clicked(null); return; }
    }
    function barTip(it, depth) {
        if (!it || (depth || 0) > 20) return "";
        if (typeof it.tooltipText === "string" && it.tooltipText !== "") return it.tooltipText;
        var kids = it.children || [];
        for (var i = 0; i < kids.length; i++) { var t = barTip(kids[i], (depth || 0) + 1); if (t) return t; }
        return "";
    }
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
    Timer {
        interval: 4600; running: root.scenario === "stopping" || root.scenario === "inactivity"
        onTriggered: {
            var card = overlay.geometryRoot();
            var chipName = root.scenario === "stopping" ? "stopChip" : "inactivityChip";
            var chip = root.find(barWidget, chipName), notice = root.find(card, "stopNotice");
            root.snap = { chip: root.texts(chip), notice: root.texts(notice), paused: root.texts(card).indexOf("Paused: notes are stopping") >= 0 };
            root.click(chip);
            afterAct.start();
        }
    }
    Timer {
        id: afterAct
        interval: 700
        onTriggered: {
            var card = overlay.geometryRoot();
            var chipName = root.scenario === "stopping" ? "stopChip" : "inactivityChip";
            root.snap.cleared = !root.find(barWidget, chipName) && !root.find(card, "stopNotice");
            console.log((root.scenario === "stopping" ? "STOPPING " : "INACTIVITY ") + JSON.stringify(root.snap));
            Qt.quit();
        }
    }
    Timer {
        interval: 250; repeat: true; running: root.scenario === "window-states"
        onTriggered: {
            var card = overlay.geometryRoot(), wi = root.find(card, "windowIdentity"), tip = root.barTip(barWidget);
            var v = wi ? String(wi.text) : "";
            if (v !== "" && v !== root.lastWin && tip.indexOf("Taking notes") === 0) {
                root.lastWin = v;
                console.log("WIN_STATE " + JSON.stringify({ view: v, tip: tip }));
                if (Quickshell.env("MEETINGAI_GEOMETRY") === "1") Geo.dump(card, overlay.ctl, v, overlay.winW, overlay.winH);
            }
        }
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
        anchors { top: true; left: true }
        implicitWidth: 320; implicitHeight: 36
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        color: "transparent"
        Rectangle { anchors.fill: parent; color: Commons.Color.bar.background }
        BarWidget { id: barWidget; bar: barStub; anchors.fill: parent }
    }
}
