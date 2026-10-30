import QtQuick
import Quickshell
import "geometry.js" as Geo

// For tests/smoke.sh --voice: room rows (transcript.get, transcript.relabel, transcript.final)
// and the voice controls in Settings (status, sentence, Record my voice, Record again, Pause check, Delete).
ShellRoot {
    Overlay {
        id: overlay
        Component.onCompleted: overlay.open("{}")
    }
    Timer { interval: 1500; running: true; onTriggered: { Geo.dump(overlay.geometryRoot(), overlay.ctl, "transcript", overlay.winW, overlay.winH); console.log("VOICE1 " + overlay.dropDump()); } }
    Timer { interval: 2000; running: true; onTriggered: overlay.showLibrary("settings") }
    Timer { interval: 2500; running: true; onTriggered: { Geo.dump(overlay.geometryRoot(), overlay.ctl, "before-enroll", overlay.winW, overlay.winH); overlay.voiceHook("enroll"); } }
    Timer { interval: 2650; running: true; onTriggered: console.log("VOICE2 " + overlay.dropDump()) }
    Timer { interval: 3500; running: true; onTriggered: { Geo.dump(overlay.geometryRoot(), overlay.ctl, "enrolled", overlay.winW, overlay.winH); console.log("VOICE3 " + overlay.dropDump()); overlay.voiceHook("pause"); } }
    Timer { interval: 4200; running: true; onTriggered: { Geo.dump(overlay.geometryRoot(), overlay.ctl, "paused", overlay.winW, overlay.winH); console.log("VOICE4 " + overlay.dropDump()); overlay.voiceHook("delete"); } }
    Timer { interval: 5000; running: true; onTriggered: console.log("VOICE5 " + overlay.dropDump()) }
}
