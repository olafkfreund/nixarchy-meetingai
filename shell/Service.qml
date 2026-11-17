pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import Quickshell.Io

// Watches the call state written by nixarchy.meeting-mode (CallState.qml) and offers to take
// notes, or records at once with auto_record_on_call (never in interviewer mode: consent stays
// manual). The daemon ends notes started for a call; this only shows its countdown and chip.
Item {
    id: root

    property var shell: null
    property var manifest: null

    Client { id: client }
    readonly property var calls: client.calls   // titles stay in memory; never logged
    property string prevCall: "none"
    property double handledHandoff: 0   // the daemon's last handoff token acted on
    property double offeredFor: -1   // the call (its start) already offered or auto-started
    property double calledKey: 0     // the call (its start) that reached "call"; it may end through "maybe"
    property bool waiting: false     // a call came before the daemon or its settings were ready
    property double stopGen: 0       // the countdown (daemon gen, a u64) already notified
    property bool chipShown: false   // the inactivity notification for this chip is up

    // One recording: a restart into the same meeting is a different recording
    function recId() { return String(client.state.meeting) + "@" + Number(client.state.since); }

    // The daemon keeps its one-time provider notice (e.g. the removed Gemini key) until this service asks.
    // The daemon ended the old notes because a different call began; once they are saved it offers a token and the new call gets its own.
    // meeting-mode keeps callSince across such a switch, so the call may already be marked as offered.
    function checkHandoff() {
        var h = client.state.handoff;
        if (typeof h !== "number" || h === root.handledHandoff || calls.call !== "call") return;
        root.handledHandoff = h;
        root.offeredFor = -1;
        root.react();
    }

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
    function startNotes(quiet, title, reason) {
        var args = title ? { new: true, title: title } : { new: true };
        if (reason) args.reason = reason;
        client.send("record.start", args, function(r) {
            var e = String(r.error || "");
            if (r.ok) { if (!quiet) Quickshell.execDetached(["notify-send", "-a", "nixarchy-meetingai", "nixarchy-meetingai", "Taking notes"]); return; }
            if (!quiet && e.indexOf("language_unsupported: ") === 0) {
                root.offer(e.replace("language_unsupported: ", "Recording was not started: "), "no", "Dismiss", calls.since || 1);
                return;
            }
            Quickshell.execDetached(["notify-send", "-a", "nixarchy-meetingai", "nixarchy-meetingai", e.replace(/^(consent_required|local_only|language_unsupported): /, "") + (e.indexOf("consent_required: ") === 0 ? " Open the overlay and start recording." : "")]);
        });
    }

    // An old notification must not act on a later call: Take needs the offered call still on, Stop must not hit a newer call's notes.
    // Keep carries the daemon's gen, so a stale answer is refused there.
    function act(what, key, meeting) {
        if (what === "start") {
            if (calls.call === "call" && (calls.since || 1) === key) root.startNotes(true, calls.title);
        } else if (what === "stop") {
            if (!(calls.call === "call" && (calls.since || 1) !== key) && client.recording && root.recId() === meeting)
                client.send("record.stop", { reason: "notification" });
        } else if (what === "keep") {
            client.send("record.keep", { gen: key });
            root.calledKey = 0;   // kept: the end of this call needs no "still recording" offer
        } else if (what === "chipstop") {
            if (client.recording && root.recId() === meeting) client.send("record.stop", { reason: "inactivity_chip" });
        }
    }

    function offer(body, action, label, key, meeting, timeoutMs) {
        var command = ["notify-send", "-a", "nixarchy-meetingai", "-A", action + "=" + label];
        if (action !== "no" && action !== "keep") command.push("-A", "no=Not now");
        if (timeoutMs !== undefined) command.push("-t", String(timeoutMs));
        command.push("nixarchy-meetingai", body);
        var p = notifier.createObject(root, {
            command: command,
            running: true,
            actionName: action,
            key: key,
            meeting: meeting || "",
            then: action === "take" ? "start" : action === "chipstop" ? "chipstop" : action
        });
    }

    // A call is on and nobody is taking notes: record (consent stays manual, so never in interviewer mode) or offer.
    // Once per call, whether it comes from the state change or from meeting-mode's hook.
    function react() {
        var key = calls.since || 1;
        if (calls.call !== "call" || client.recording || root.offeredFor === key) return "idle";
        if (client.state.handoff_pending === true) return "idle";   // the old notes are being saved; checkHandoff() starts this call's notes when they are
        if (!client.connected || !client.settingsReady) { root.waiting = true; return "waiting"; }   // settings decide auto-record
        root.waiting = false;
        root.offeredFor = key;
        if (client.settings.auto_record_on_call === true && client.settings.meeting_mode !== "interviewer") {
            root.startNotes(false, calls.title, "call");
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
            if (!client.recording) { root.stopGen = 0; root.chipShown = false; }
        }
        function onStateChanged() {
            root.checkHandoff();
            var st = client.state.stopping;
            if (st && st.gen !== root.stopGen) {
                root.stopGen = st.gen;
                root.offer(st.reason === "call_end" ? "The call ended. Notes stop in " + st.in_s + " s." : "No sound for a while. Notes stop in " + st.in_s + " s.", "keep", "Keep recording", st.gen, root.recId(), 30000);
            } else if (!st) root.stopGen = 0;
            var chip = client.state.inactivity_chip;
            if (chip && !root.chipShown) {
                root.chipShown = true;
                root.offer("No sound for " + chip.minutes + " min.", "chipstop", "Stop notes", 1, root.recId());
            } else if (!chip) root.chipShown = false;
        }
    }

    Connections {
        target: root.calls
        function onCallChanged() {
            root.prevCall = root.calls.call;
            root.checkHandoff();
            if (root.calls.call === "call") root.calledKey = root.calls.since || 1;   // also a call already on at startup
            if (!root.calls.seen) return;
            if (client.connected) client.send("calendar.refresh", {});
            if (root.calls.call === "call") {
                root.react();
            } else if (root.calls.call === "maybe" && (root.calls.since || 1) !== root.calledKey) root.calledKey = 0;   // a different call
            else if (root.calls.call === "none") {
                // "call" may end through "maybe": the daemon ends notes it owns; the rest stay, so say so
                if (root.calledKey !== 0 && client.recording && !client.state.for_call)
                    root.offer("The call ended. Notes are still recording.", "stop", "Stop notes", root.calledKey, root.recId());
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
