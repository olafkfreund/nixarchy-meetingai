import QtQuick
import Quickshell
import "geometry.js" as Geo

// For tests/live-shots.sh SCENARIO=providers: pick each provider in turn, as the Settings dropdowns do, and log what
// the model lists hold ("PROV {json}"). claude-sub's list is slow in fake-daemon.py and is followed at once by ollama,
// so its late reply must be dropped; opencode's list fails. live-shots.sh asserts the dumps.
ShellRoot {
    id: root
    property int n: 0
    readonly property var steps: [
        { id: "ollama", wait: 1200 }, { id: "claude-sub", wait: 150, quiet: true }, { id: "ollama", wait: 1800 },
        { id: "agy-sub", wait: 1200 }, { id: "opencode", wait: 1200 }, { id: "codex-sub", wait: 1200 }, { id: "ollama-cloud", wait: 1200 }, { id: "ollama", wait: 1200 }
    ]
    Overlay {
        id: overlay
        Component.onCompleted: overlay.open("{}")
    }
    Timer { interval: 3000; running: true; onTriggered: { overlay.showLibrary("settings"); step.start(); } }
    Timer { interval: 13000; running: true; onTriggered: overlay.showLibrary("providers") }   // the status rows with their login commands
    Timer { interval: 1500; repeat: true; running: Quickshell.env("MEETINGAI_GEOMETRY") === "1"; onTriggered: Geo.dump(overlay.geometryRoot(), overlay.ctl, "", overlay.winW, overlay.winH) }
    Timer {
        id: step
        interval: 400; repeat: true
        onTriggered: {
            var prev = root.n > 0 ? root.steps[root.n - 1] : null;
            if (prev && !prev.quiet) console.log("PROV " + JSON.stringify(Object.assign({ at: prev.id }, JSON.parse(overlay.providerDump()))));
            if (root.n >= root.steps.length) { step.stop(); console.log("PROV done"); return; }
            var s = root.steps[root.n++];
            overlay.setProvider(false, s.id);
            overlay.setProvider(true, s.id);
            step.interval = s.wait;
        }
    }
}
