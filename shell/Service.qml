pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import Quickshell.Io

// Watches the call state written by nixarchy.meeting-mode and offers to take
// notes. It never stops a recording on its own: the hang-up notification only
// offers to.
Item {
    id: root

    property var shell: null
    property var manifest: null

    readonly property string callFile: Quickshell.env("XDG_RUNTIME_DIR") + "/meeting-mode.json"
    property bool callActive: false
    property bool seen: false   // the first read is the baseline: a call already running at startup is not a new call

    Client { id: client }

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

    function act(what) {
        client.send(what === "start" ? "record.start" : "record.stop", {});
    }

    function offer(body, action, label) {
        var p = notifier.createObject(root, {
            command: ["notify-send", "-a", "nixarchy-meetingai", "-A", action + "=" + label, "-A", "no=Not now", "nixarchy-meetingai", body],
            running: true,
            actionName: action,
            then: action === "take" ? "start" : "stop"
        });
    }

    onCallActiveChanged: {
        if (!root.seen) return;
        if (root.callActive && !client.recording) {
            if (client.settings.auto_record_on_call === true) {
                client.send("record.start", {});
                Quickshell.execDetached(["notify-send", "-a", "nixarchy-meetingai", "nixarchy-meetingai", "Taking notes"]);
            } else {
                root.offer("A call started", "take", "Take notes");
            }
        } else if (!root.callActive && client.recording) {
            root.offer("The call ended. Notes are still recording.", "stop", "Stop notes");
        }
    }

    FileView {
        id: callIn
        path: root.callFile
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onLoadFailed: root.seen = true   // no file yet: the baseline is "no call"
        onLoaded: {
            try {
                root.callActive = JSON.parse(text()).active === true;
                root.seen = true;
            } catch (e) {}
        }
    }

    // Atomic replaces can slip past the watcher.
    Timer {
        interval: 5000; repeat: true; running: true
        onTriggered: callIn.reload()
    }

    // omarchy-shell meetingai toggle
    IpcHandler {
        target: "meetingai"
        function toggle(): string {
            return root.shell && root.shell.toggle("nixarchy.meetingai", "{}") ? "ok" : "unavailable";
        }
    }
}
