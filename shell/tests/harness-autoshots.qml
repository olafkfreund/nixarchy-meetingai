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
                root.out.confirm = root.texts(card).indexOf("including other windows") >= 0 && !!ok;
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
                root.out.limit = root.texts(card).indexOf("limit reached (60)") >= 0;
                overlay.showLibrary("settings");   // the session list loads on a tab change
            } else if (root.phase === 9) {
                overlay.showLibrary("restore");
            } else if (root.phase === 11) {
                var n = root.find(card, "openNotes");
                root.out.openNotes = !!n;
                if (n) n.clicked();
            } else if (root.phase === 14) {
                var s0 = root.find(card, "shotOpen0"), s1 = root.find(card, "shotOpen1");
                root.out.rows = !!s0 && !!s1 && String(s0.text).indexOf("1:05") >= 0 && String(s0.text).indexOf("automatic") >= 0 && String(s1.text).indexOf("manual") >= 0;
                if (s0) s0.clicked();
            } else if (root.phase === 15) {
                var im = root.find(card, "shotImage0");
                root.out.preview = !!im && im.status === Image.Ready;
                var d = root.find(card, "shotDelete0");
                if (d) d.clicked();
            } else if (root.phase === 16) {
                ok = root.find(card, "confirmOk");
                root.out.deleteConfirm = !!ok;
                if (ok) ok.clicked();
            } else if (root.phase === 18) {
                root.out.afterDelete = !root.find(card, "shotOpen1") && !!root.find(card, "shotOpen0");
                console.log("AUTOSHOTS " + JSON.stringify(root.out));
                Qt.quit();
            }
        }
    }
}
