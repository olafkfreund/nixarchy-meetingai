pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import Quickshell.Io

// Watches the call state written by nixarchy.meeting-mode (CallState.qml) and offers to take
// notes, or records at once with auto_record_on_call (never in interviewer mode: consent stays
// manual). It never stops a recording on its own: the hang-up notification only offers to.
Item {
    id: root

    property var shell: null
    property var manifest: null

    Client { id: client }
    readonly property var calls: client.calls   // titles stay in memory; never logged
    property string prevCall: "none"
    property double offeredFor: -1   // the call (its start) already offered or auto-started
    property double calledKey: 0     // the call (its start) that reached "call"; it may end through "maybe"
    property bool waiting: false     // a call came before the daemon or its settings were ready

    // The daemon keeps its one-time provider notice (e.g. the removed Gemini key) until this service asks.
    Connections {
        target: client
        function onConnectedChanged() {
            if (!client.connected) return;
            client.send("notice.take", {}, function(r) {
                if (r.ok && r.data && r.data.text) Quickshell.execDetached(["notify-send", "-a", "nixarchy-meetingai", "nixarchy-meetingai", String(r.data.text)]);
            });
        }
    }

    // notify-send -A waits, then prints the chosen action id.
    Component {
        id: notifier
        Process {
            id: p
            property string actionName: ""
            property string then: ""
            property double key: 0
            stdout: StdioCollector {
                onStreamFinished: if (String(text).trim() === p.actionName) root.act(p.then, p.key)
            }
            onExited: p.destroy()
        }
    }

    // The overlay's consent banner lives in another Client, so a refused start (candidate not told,
    // or local_only) is shown as a notification here; the interviewer then starts it from the overlay.
    function startNotes(quiet, title) {
        client.send("record.start", title ? { new: true, title: title } : { new: true }, function(r) {
            var e = String(r.error || "");
            if (r.ok) { if (!quiet) Quickshell.execDetached(["notify-send", "-a", "nixarchy-meetingai", "nixarchy-meetingai", "Taking notes"]); return; }
            Quickshell.execDetached(["notify-send", "-a", "nixarchy-meetingai", "nixarchy-meetingai", e.replace(/^(consent_required|local_only): /, "") + (e.indexOf("consent_required: ") === 0 ? " Open the overlay and start recording." : "")]);
        });
    }

    // An old notification must not act on a later call: Take needs the offered call still on, Stop must not hit a newer call's notes.
    function act(what, key) {
        if (what === "start") {
            if (calls.call === "call" && (calls.since || 1) === key) root.startNotes(true, calls.title);
        } else if (!(calls.call === "call" && (calls.since || 1) !== key)) client.send("record.stop", {});
    }

    function offer(body, action, label, key) {
        var p = notifier.createObject(root, {
            command: ["notify-send", "-a", "nixarchy-meetingai", "-A", action + "=" + label, "-A", "no=Not now", "nixarchy-meetingai", body],
            running: true,
            actionName: action,
            key: key,
            then: action === "take" ? "start" : "stop"
        });
    }

    // A call is on and nobody is taking notes: record (consent stays manual, so never in interviewer mode) or offer.
    // Once per call, whether it comes from the state change or from meeting-mode's hook.
    function react() {
        var key = calls.since || 1;
        if (calls.call !== "call" || client.recording || root.offeredFor === key) return "idle";
        if (!client.connected || !client.settingsReady) { root.waiting = true; return "waiting"; }   // settings decide auto-record
        root.waiting = false;
        root.offeredFor = key;
        if (client.settings.auto_record_on_call === true && client.settings.meeting_mode !== "interviewer") {
            root.startNotes(false, calls.title);
            return "recording";
        }
        root.offer("A call started" + (calls.app ? " in " + calls.app : ""), "take", "Take notes", key);
        return "offered";
    }

    Connections {
        target: client
        function onSettingsReadyChanged() {
            if (client.settingsReady && root.waiting) root.react();
        }
    }

    Connections {
        target: calls
        function onCallChanged() {
            var was = root.prevCall;
            root.prevCall = calls.call;
            if (calls.call === "call") root.calledKey = calls.since || 1;   // also a call already on at startup
            if (!calls.seen) return;
            if (client.connected) client.send("calendar.refresh", {});
            if (calls.call === "call") {
                root.react();
            } else if (calls.call === "maybe" && (calls.since || 1) !== root.calledKey) root.calledKey = 0;   // a different call
            else if (calls.call === "none") {
                // "call" may end through "maybe": offer when this call had reached "call"
                if (root.calledKey !== 0 && client.recording)
                    root.offer("The call ended. Notes are still recording.", "stop", "Stop notes", root.calledKey);
                root.calledKey = 0;
                root.waiting = false;
            }
        }
    }

    // omarchy-shell meetingai toggle
    IpcHandler {
        target: "meetingai"
        function offer(): string {
            calls.reloadNow();   // an IPC may beat the file watcher
            return root.react();
        }
        function show(): string {
            return root.shell && root.shell.summon("nixarchy.meetingai", "{}") ? "ok" : "unavailable";
        }
        function toggle(): string {
            return root.shell && root.shell.toggle("nixarchy.meetingai", "{}") ? "ok" : "unavailable";
        }
    }
}
