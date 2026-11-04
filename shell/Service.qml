pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import Quickshell.Io

// Watches the call state written by nixarchy.meeting-mode (CallState.qml) and offers to take
// notes, or records at once with auto_record_on_call (never in interviewer mode: consent stays
// manual). Notes started for a call stop after a grace period when that call ends.
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
    property double ownerSince: 0    // callSince of the call that owns the notes; kept across a takeover
    property var pendingStop: null   // { meeting } while the grace timer runs
    property double handledFor: 0    // the owner whose end was already handled: one timer or offer per end
    property double keptFor: 0       // the owner the user chose Keep recording for: never stopped or asked again
    property int stopGraceMs: 30000  // the tests set 2000
    readonly property int slackMs: 5000   // notes started in a short "maybe" just before the call still count

    // ownerSince 1 is a call with no callSince: it owns nothing, so older notes are never stopped
    function forCall() {
        return root.ownerSince > 1 && client.recording && Number(client.state.since) >= root.ownerSince - root.slackMs;
    }

    // One recording: a restart into the same meeting is a different recording
    function recId() { return String(client.state.meeting) + "@" + Number(client.state.since); }

    Timer {
        id: stopTimer
        interval: root.stopGraceMs
        onTriggered: {
            if (root.pendingStop && client.recording && root.recId() === root.pendingStop.meeting && root.calls.call !== "call" && !root.calls.writerGone)
                client.send("record.stop", {});
            root.pendingStop = null;
        }
    }

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
            property string meeting: ""
            stdout: StdioCollector {
                onStreamFinished: if (String(text).trim() === p.actionName) root.act(p.then, p.key, p.meeting)
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
            if (!quiet && e.indexOf("language_unsupported: ") === 0) {
                root.offer(e.replace("language_unsupported: ", "Recording was not started: "), "no", "Dismiss", calls.since || 1);
                return;
            }
            Quickshell.execDetached(["notify-send", "-a", "nixarchy-meetingai", "nixarchy-meetingai", e.replace(/^(consent_required|local_only): /, "") + (e.indexOf("consent_required: ") === 0 ? " Open the overlay and start recording." : "")]);
        });
    }

    // An old notification must not act on a later call: Take needs the offered call still on, Stop must not hit a newer call's notes.
    function act(what, key, meeting) {
        if (what === "start") {
            if (calls.call === "call" && (calls.since || 1) === key) root.startNotes(true, calls.title);
        } else if (what === "stop") {
            if (!(calls.call === "call" && (calls.since || 1) !== key) && client.recording && root.recId() === meeting)
                client.send("record.stop", {});
        } else if (what === "keep" && root.pendingStop && root.pendingStop.meeting === meeting) {
            stopTimer.stop();
            root.pendingStop = null;
            root.keptFor = root.ownerSince;
        }
    }

    function offer(body, action, label, key, meeting, timeoutMs) {
        var command = ["notify-send", "-a", "nixarchy-meetingai", "-A", action + "=" + label];
        if (action !== "no") command.push("-A", "no=Not now");
        if (timeoutMs !== undefined) command.push("-t", String(timeoutMs));
        command.push("nixarchy-meetingai", body);
        var p = notifier.createObject(root, {
            command: command,
            running: true,
            actionName: action,
            key: key,
            meeting: meeting || "",
            then: action === "take" ? "start" : action
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
        function onRecordingChanged() {
            if (!client.recording) {
                stopTimer.stop();
                root.pendingStop = null;
                root.keptFor = 0;   // Keep was for that recording, not the next
            }
        }
    }

    Connections {
        target: root.calls
        function onCallChanged() {
            var was = root.prevCall;
            root.prevCall = root.calls.call;
            if (root.calls.call === "call") {
                root.calledKey = root.calls.since || 1;   // also a call already on at startup
                if (root.pendingStop) {
                    stopTimer.stop();
                    root.pendingStop = null;
                    root.handledFor = 0;
                } else {   // a new call, or the same one again: its end is handled afresh, Keep only holds for the same call
                    root.ownerSince = root.calls.since || 1;
                    if (root.keptFor !== root.ownerSince) root.keptFor = 0;
                    root.handledFor = 0;
                }
            } else if (was === "call" && root.handledFor !== root.ownerSince && !root.calls.writerGone && root.forCall() && root.keptFor !== root.ownerSince) {
                root.handledFor = root.ownerSince;
                root.pendingStop = { meeting: root.recId() };
                stopTimer.restart();
                root.offer("The call ended. Notes stop in " + root.stopGraceMs / 1000 + " s.", "keep", "Keep recording", root.ownerSince, root.pendingStop.meeting, root.stopGraceMs);
            }
            if (!root.calls.seen) return;
            if (client.connected) client.send("calendar.refresh", {});
            if (root.calls.call === "call") {
                root.react();
            } else if (root.calls.call === "maybe" && (root.calls.since || 1) !== root.calledKey) root.calledKey = 0;   // a different call
            else if (root.calls.call === "none") {
                // "call" may end through "maybe": offer when this call had reached "call"
                if (root.calledKey !== 0 && client.recording && !root.pendingStop && root.keptFor !== root.ownerSince && root.handledFor !== root.ownerSince) {
                    root.handledFor = root.ownerSince;
                    root.offer("The call ended. Notes are still recording.", "stop", "Stop notes", root.calledKey, root.recId());
                }
                root.calledKey = 0;
                root.waiting = false;
            }
        }
        function onWriterGoneChanged() {
            if (root.calls.writerGone && root.pendingStop) {
                stopTimer.stop();
                root.pendingStop = null;
                if (client.recording) {
                    root.handledFor = root.ownerSince;
                    root.offer("The call ended. Notes are still recording.", "stop", "Stop notes", root.ownerSince, root.recId());
                }
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
