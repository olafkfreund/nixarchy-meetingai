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

    CallState { id: calls }   // titles stay in memory; never logged
    property string prevCall: "none"
    property double offeredFor: -1   // the call (its start) already offered or auto-started

    Client { id: client }

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
            stdout: StdioCollector {
                onStreamFinished: if (String(text).trim() === p.actionName) root.act(p.then)
            }
            onExited: p.destroy()
        }
    }

    // The overlay's consent banner lives in another Client, so a refused start (candidate not told,
    // or local_only) is shown as a notification here; the interviewer then starts it from the overlay.
    function startNotes(quiet, title) {
        client.send("record.start", title ? { title: title } : {}, function(r) {
            var e = String(r.error || "");
            if (r.ok) { if (!quiet) Quickshell.execDetached(["notify-send", "-a", "nixarchy-meetingai", "nixarchy-meetingai", "Taking notes"]); return; }
            Quickshell.execDetached(["notify-send", "-a", "nixarchy-meetingai", "nixarchy-meetingai", e.replace(/^(consent_required|local_only): /, "") + (e.indexOf("consent_required: ") === 0 ? " Open the overlay and start recording." : "")]);
        });
    }

    function act(what) {
        if (what === "start") root.startNotes(true, calls.title);
        else client.send("record.stop", {});
    }

    function offer(body, action, label) {
        var p = notifier.createObject(root, {
            command: ["notify-send", "-a", "nixarchy-meetingai", "-A", action + "=" + label, "-A", "no=Not now", "nixarchy-meetingai", body],
            running: true,
            actionName: action,
            then: action === "take" ? "start" : "stop"
        });
    }

    // A call is on and nobody is taking notes: record (consent stays manual, so never in interviewer mode) or offer.
    // Once per call, whether it comes from the state change or from meeting-mode's hook.
    function react() {
        var key = calls.since || 1;
        if (calls.call !== "call" || client.recording || root.offeredFor === key) return "idle";
        root.offeredFor = key;
        if (client.settings.auto_record_on_call === true && client.settings.meeting_mode !== "interviewer") {
            root.startNotes(false, calls.title);
            return "recording";
        }
        root.offer("A call started" + (calls.app ? " in " + calls.app : ""), "take", "Take notes");
        return "offered";
    }

    Connections {
        target: calls
        function onCallChanged() {
            var was = root.prevCall;
            root.prevCall = calls.call;
            if (!calls.seen) return;
            if (client.connected) client.send("calendar.refresh", {});
            if (calls.call === "call") root.react();
            else if (calls.call === "none" && was === "call" && client.recording)
                root.offer("The call ended. Notes are still recording.", "stop", "Stop notes");
        }
    }

    // omarchy-shell meetingai toggle
    IpcHandler {
        target: "meetingai"
        function offer(): string {
            return root.react();
        }
        function toggle(): string {
            return root.shell && root.shell.toggle("nixarchy.meetingai", "{}") ? "ok" : "unavailable";
        }
    }
}
