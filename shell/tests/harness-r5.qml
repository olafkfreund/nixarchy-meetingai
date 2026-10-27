import QtQuick
import Quickshell
import "geometry.js" as Geo

// Round 5 frames for tests/live-shots.sh (SCENARIO=r5): resized card, Profiles tab, Prompts tab.
ShellRoot {
    Overlay {
        id: overlay
        Component.onCompleted: overlay.open("{}")
    }
    Timer { interval: 8000; running: true; onTriggered: overlay.showLibrary("profiles") }
    Timer { interval: 12000; running: true; onTriggered: overlay.showLibrary("prompts") }
    Timer { interval: 15000; running: true; onTriggered: overlay.showLibrary("restore") }   // after the last crop: geometry only
    Timer { interval: 18000; running: true; onTriggered: overlay.showLibrary("providers") }
    Timer { interval: 21000; running: true; onTriggered: overlay.showLibrary("settings") }
    // open Selects, one at a time, for the geometry check (the popup must fit in the window)
    Timer { interval: 22500; running: Quickshell.env("MEETINGAI_GEOMETRY") === "1"; onTriggered: Geo.openSelect(overlay.geometryRoot(), "", "chatModel") }
    Timer { interval: 24500; running: Quickshell.env("MEETINGAI_GEOMETRY") === "1"; onTriggered: Geo.openSelect(overlay.geometryRoot(), "chatModel", "liveProvider") }
    Timer { interval: 26500; running: Quickshell.env("MEETINGAI_GEOMETRY") === "1"; onTriggered: Geo.openSelect(overlay.geometryRoot(), "liveProvider", "pace") }
    Timer { interval: 1500; repeat: true; running: Quickshell.env("MEETINGAI_GEOMETRY") === "1"; onTriggered: Geo.dump(overlay.geometryRoot(), overlay.ctl, "", overlay.winW, overlay.winH) }
}
