import QtQuick
import Quickshell
import qs.Commons
import "geometry.js" as Geo

// For tests/smoke.sh --autoshots: the toggle (first-time confirmation, refusal, count, limit) while recording,
// then the Library's Screenshots section (time, automatic, preview, delete with confirmation).
ShellRoot {
    id: root
    property int phase: 0
    property var out: ({})
    function find(it, name, depth) {
        if (!it || !it.visible || (depth || 0) > 60) return null;
        if (it.objectName === name) return it;
        var kids = it.children || [];
        for (var i = 0; i < kids.length; i++) { var f = find(kids[i], name, (depth || 0) + 1); if (f) return f; }
        return null;
    }
    function client(it, depth) {   // the Client inside the overlay, to send a test command
        if (!it || (depth || 0) > 60) return null;
        if (typeof it.openLine === "function" && it.transcript !== undefined) return it;
        var kids = it.children || [];
        for (var i = 0; i < kids.length; i++) { var f = client(kids[i], (depth || 0) + 1); if (f) return f; }
        return null;
    }
    function texts(it, depth) {
        if (!it || !it.visible || (depth || 0) > 60) return "";
        var s = typeof it.text === "string" ? it.text : "", kids = it.children || [];
        for (var i = 0; i < kids.length; i++) s += " " + texts(kids[i], (depth || 0) + 1);
        return s;
    }
    Overlay { id: overlay; Component.onCompleted: overlay.open("{}") }
    Timer {
        interval: 500; repeat: true; running: true
        onTriggered: {
            root.phase++;
            var card = overlay.geometryRoot(), b, ok;
            if (root.phase === 4) {
                b = root.find(card, "autoShots");
                root.out.toggle = !!b;
                if (b) b.toggled();
            } else if (root.phase === 5) {
                ok = root.find(card, "confirmOk");
                var t = root.texts(card);
                root.out.confirm = t.indexOf("including other windows") >= 0 && t.indexOf("export folder may sync") >= 0 && t.indexOf("Check that you may capture what others share") >= 0 && !!ok;
                if (ok) ok.clicked();
            } else if (root.phase === 6) {
                root.out.refusal = String((root.find(card, "autoShotsNotice") || {}).text || "");
                b = root.find(card, "autoShots");
                if (b) b.toggled();
            } else if (root.phase === 7) {
                root.out.noSecondConfirm = !root.find(card, "confirmOk");
                root.out.label = root.texts(card).indexOf("Auto screenshots · on · 12") >= 0;
                Geo.dump(card, overlay.ctl, "autoshots", overlay.winW, overlay.winH);
                b = root.find(card, "autoShots");
                if (b) b.toggled();
            } else if (root.phase === 8) {
                root.client(overlay).send("test.limit", {});   // the daemon reaches 60 while on
            } else if (root.phase === 9) {
                root.out.limit = root.texts(card).indexOf("limit reached (60)") >= 0;
                b = root.find(card, "autoShots");
                if (b) b.toggled();   // on is false at the limit, so this asks the daemon to start and is refused
            } else if (root.phase === 10) {
                root.out.limitNotice = String((root.find(card, "autoShotsNotice") || {}).text || "");
                overlay.showLibrary("settings");   // the session list loads on a tab change
            } else if (root.phase === 11) {
                overlay.showLibrary("restore");
            } else if (root.phase === 13) {
                var n = root.find(card, "openNotes");
                root.out.openNotes = !!n;
                if (n) n.clicked();
            } else if (root.phase === 16) {
                var s0 = root.find(card, "shotOpen0"), s1 = root.find(card, "shotOpen1");
                root.out.rows = !!s0 && !!s1 && String(s0.text).indexOf("1:05") >= 0 && String(s0.text).indexOf("automatic") >= 0 && String(s1.text).indexOf("manual") >= 0;
                if (s0) s0.clicked();
            } else if (root.phase === 17) {
                var im = root.find(card, "shotImage0");
                root.out.preview = !!im && im.status === Image.Ready;
                var d = root.find(card, "shotDelete0");
                if (d) d.clicked();
            } else if (root.phase === 18) {
                root.out.deleteWording = root.texts(card).indexOf("Delete this screenshot? It is removed from the meeting and its export folder. Copies already synced elsewhere are not.") >= 0;
                ok = root.find(card, "confirmOk");
                root.out.deleteConfirm = !!ok;
                if (ok) ok.clicked();
            } else if (root.phase === 20) {
                root.out.exportError = String((root.find(card, "exportStatus") || {}).text || "");
                root.out.afterDelete = !root.find(card, "shotOpen1") && !!root.find(card, "shotOpen0");
                console.log("AUTOSHOTS " + JSON.stringify(root.out));
                Qt.quit();
            }
        }
    }
}
