import QtQuick
import Quickshell
import Quickshell.Io

// The call state nixarchy.meeting-mode writes to $XDG_RUNTIME_DIR/meeting-mode.json, validated:
// a file whose writer (pid) is gone, or an old file with no pid to check, reads as "none".
// The service and the bar widget each make one. Titles are never logged.
Item {
    id: root

    readonly property string file: Quickshell.env("XDG_RUNTIME_DIR") + "/meeting-mode.json"
    property string call: "none"   // none | maybe | call
    property string app: ""
    property string title: ""
    property double since: 0
    property bool seen: false      // the first read is done: a call already running at startup is the baseline
    property bool missing: false   // no state file 30 s after start: the detector is not installed
    property var raw: null
    property bool writerGone: false
    property double checkedPid: 0

    function refresh() {
        var r = root.raw;
        var pid = r ? Number(r.pid) || 0 : 0;
        // shortcut: meeting-mode writes only on change, so age alone cannot tell a long call from a dead file; it counts only when no pid can be checked
        var stale = !r || root.writerGone || (pid <= 0 && Date.now() - (Number(r.updated) || 0) > 600000);
        var c = !stale && (r.call === "call" || r.call === "maybe") ? r.call : "none";
        root.app = c === "none" ? "" : String(r.callApp || "");
        root.title = c === "none" ? "" : String(r.callTitle || "");
        root.since = c === "none" ? 0 : Number(r.callSince) || 0;
        root.call = c;
    }

    // exit 0 = the writer is alive
    Process {
        id: alive
        onExited: function(code) { root.writerGone = code !== 0; root.refresh(); root.seen = true; }
    }

    // Only while a call is on (or when the writer changed): an idle state needs no /proc probe.
    function check(force) {
        var pid = root.raw ? Number(root.raw.pid) || 0 : 0;
        if (pid > 0 && !alive.running && (force === true || root.raw.call !== "none")) {
            alive.command = ["test", "-d", "/proc/" + pid];
            alive.running = true;
        }
    }

    // Reads the file now (blocking), for a caller that must not act on a state the watcher has not seen yet.
    function reloadNow() {
        callIn.blockLoading = true;
        callIn.reload();
        callIn.blockLoading = false;
    }

    FileView {
        id: callIn
        path: root.file
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onLoadFailed: root.seen = true   // no file yet: the baseline is "no call"
        onLoaded: {
            try {
                root.raw = JSON.parse(text());
                let pid = Number(root.raw.pid) || 0;
                root.missing = false;
                if (pid > 0 && pid !== root.checkedPid) {   // a new writer counts as gone until /proc says otherwise, so a dead file never flashes as a call
                    root.checkedPid = pid;
                    root.writerGone = true;
                    root.refresh();
                    root.check(true);
                } else {
                    root.refresh();
                    root.seen = true;
                }
            } catch (e) {}
        }
    }

    // Atomic replaces can slip past the watcher; this also notices a writer that died.
    Timer {
        interval: 5000; repeat: true; running: true
        onTriggered: { callIn.reload(); root.check(); }
    }

    Timer {
        interval: 30000; running: true
        onTriggered: if (root.raw === null) root.missing = true
    }
}
