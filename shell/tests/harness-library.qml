import QtQuick
import Quickshell
import "geometry.js" as Geo

// Library session list: selection, notes pane states, late-reply guard, row menu, metadata, capHeadings and "no results".
// Rows are found by title (accessible name), never by position. Phases are 500 ms apart; the fake answers notes.get after 0.8 s.
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
    function findAll(it, name, acc, depth) {
        if (!it || !it.visible || (depth || 0) > 60) return acc;
        if (it.objectName === name) acc.push(it);
        var kids = it.children || [];
        for (var i = 0; i < kids.length; i++) findAll(kids[i], name, acc, (depth || 0) + 1);
        return acc;
    }
    function library(it, depth) {
        if (!it || (depth || 0) > 60) return null;
        if (typeof it.capHeadings === "function") return it;
        var kids = it.children || [];
        for (var i = 0; i < kids.length; i++) { var f = library(kids[i], (depth || 0) + 1); if (f) return f; }
        return null;
    }
    function visibleText(it, needle, depth) {
        if (!it || !it.visible || (depth || 0) > 60) return "";
        if (typeof it.text === "string" && it.text.indexOf(needle) >= 0) return String(it.text);
        var kids = it.children || [];
        for (var i = 0; i < kids.length; i++) { var s = visibleText(kids[i], needle, (depth || 0) + 1); if (s) return s; }
        return "";
    }
    function texts(it, depth) {
        if (!it || (depth || 0) > 60) return "";
        var s = typeof it.text === "string" ? it.text : "", kids = it.children || [];
        for (var i = 0; i < kids.length; i++) s += " " + texts(kids[i], (depth || 0) + 1);
        return s;
    }
    // the row (openNotes: meeting, openChat: chat) whose accessible name holds the title
    function row(card, kind, title) {
        var all = root.findAll(card, kind, []);
        for (var i = 0; i < all.length; i++) if (String(all[i].accName).indexOf(title) >= 0) return all[i];
        return null;
    }
    function stateText(card) { var t = root.find(card, "notesStateText"); return t ? String(t.text) : ""; }
    function whenX(r) { var w = root.find(r, "sessionWhen"); return w ? w.mapToItem(null, 0, 0).x : -1; }

    Overlay { id: overlay; Component.onCompleted: overlay.open("{}") }
    Timer {
        interval: 500; repeat: true; running: true
        onTriggered: {
            root.phase++;
            var card = overlay.geometryRoot(), lib = root.library(card), r, input, more;
            if (root.phase === 2) {
                overlay.showLibrary("restore");
            } else if (root.phase === 4) {
                overlay.showLibrary("settings");   // the session list loads on a tab change
            } else if (root.phase === 5) {
                overlay.showLibrary("restore");
            } else if (root.phase === 7) {
                Geo.dump(card, overlay.ctl, "library", overlay.winW, overlay.winH);   // before any search or selection
                r = root.row(card, "openNotes", "Fake standup");
                root.out.rows = !!r && !!root.row(card, "openNotes", "Quick sync") && !!root.row(card, "openChat", "Fake chat");
                if (r) r.clicked();
                root.out.loading = root.stateText(card);
            } else if (root.phase === 9) {
                root.out.title = root.texts(root.find(card, "notesTitle"));
                var recap = root.visibleText(card, "Meeting recap");
                root.out.recapShown = recap !== "";
                root.out.recapNoHash = recap !== "" && recap.indexOf("#") < 0;
                root.out.selectedBar = !!root.find(root.row(card, "openNotes", "Fake standup"), "selectedBar");
                root.out.beforeBump = root.visibleText(card, "updated") === "";
                if (lib) lib.client.send("notes.bump", {}, function(res) {});
            } else if (root.phase === 11) {
                root.out.reloaded = root.visibleText(card, "updated") !== "";
                r = root.row(card, "openChat", "Fake chat");
                if (r) r.clicked();
                root.out.chatHidesPane = !root.find(card, "notesBox");
            } else if (root.phase === 13) {
                r = root.row(card, "openNotes", "Fake standup");
                if (r) r.clicked();
                r = root.row(card, "openChat", "Fake chat");
                if (r) r.clicked();   // the notes.get reply of the meeting is still in flight
            } else if (root.phase === 16) {
                root.out.lateReplyHidden = !root.find(card, "notesBox");
                r = root.row(card, "openChat", "Fake chat");
                root.out.whenBefore = root.whenX(r);
                more = root.find(r, "rowMore");
                if (more) more.clicked();
            } else if (root.phase === 17) {
                r = root.row(card, "openChat", "Fake chat");
                root.out.chatDelete = !!root.find(r, "deleteSession");
                root.out.chatNoExport = !root.find(card, "exportNow");
                root.out.whenAfter = root.whenX(r);
                root.out.standupMeta = root.visibleText(root.row(card, "openNotes", "Fake standup"), " lines");
                root.out.quickMeta = root.visibleText(root.row(card, "openNotes", "Quick sync"), " lines");
            } else if (root.phase === 18) {
                r = root.row(card, "openNotes", "Quick sync");
                if (r) r.clicked();
                root.out.quickLoading = root.stateText(card);
            } else if (root.phase === 20) {
                root.out.quickEmpty = root.stateText(card);
                root.out.caps = lib ? lib.capHeadings("# Title\n  ## Sub\n```\n# kept\n```\n~~~\n# kept2\n~~~\n# A **b**") : "";
            } else if (root.phase === 21) {
                input = root.find(card, "search");
                if (input) input.text = "zzzz";
                root.out.noResultsEarly = !!root.find(card, "noResults");   // the transcript search has not answered yet
            } else if (root.phase === 23) {
                root.out.noResults = !!root.find(card, "noResults");
                console.log("LIBRARY " + JSON.stringify(root.out));
                Qt.quit();
            }
        }
    }
}
