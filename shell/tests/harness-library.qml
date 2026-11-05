import QtQuick
import Quickshell
import qs.Commons
import "geometry.js" as Geo

// Library session list: selection, notes column (wide) and swap (narrow), late-reply guard, row menu, metadata, capHeadings and "no results".
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
    function whenY(r) { var w = root.find(r, "sessionWhen"); return w ? w.mapToItem(null, 0, 0).y : -1; }
    // y of the row's title Text (found by its text), to tell that the date sits on a line below it
    function titleY(it, title, depth) {
        if (!it || !it.visible || (depth || 0) > 60) return -1;
        if (it.text === title && it.objectName !== "sessionWhen") return it.mapToItem(null, 0, 0).y;
        var kids = it.children || [];
        for (var i = 0; i < kids.length; i++) { var y = titleY(kids[i], title, (depth || 0) + 1); if (y >= 0) return y; }
        return -1;
    }
    function emptyText(card) { var t = root.find(card, "notesEmpty"); return t ? String(t.text) : ""; }
    function listShown(card) { return !!root.find(card, "restoreList"); }
    // the Escape handler of the overlay card: the Library's back() first, then close it
    function pressEscape(lib) { if (!lib.back()) overlay.library = false; }
    // card geometry sets, then dumps one tick later (layout needs a pass): wide (100%/150%) and narrow, units around the wide threshold
    property var queue: []
    property bool queueRun: false
    function applyEntry(e, lib) {
        var delta = overlay.geom.w - lib.width, w = e.w, h = e.h;
        if (e.units) w = Style.space(e.units) * lib.theme.ts + delta;
        overlay.bumpScale(0);
        if (e.scale) overlay.bumpScale(e.scale);
        if (w) { overlay.winW = w + 80; overlay.winH = h + 80; overlay.live = { x: 40, y: 40, w: w, h: h }; }
        else { overlay.winW = 1200; overlay.winH = 800; overlay.live = null; }
    }

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
                root.out.emptyBefore = root.emptyText(card);
                root.out.noBoxBefore = !root.find(card, "notesBox");
                r = root.row(card, "openNotes", "Fake standup");
                root.out.rows = !!r && !!root.row(card, "openNotes", "Quick sync") && !!root.row(card, "openChat", "Fake chat");
                if (r) r.clicked();
                root.out.loading = root.stateText(card);
            } else if (root.phase === 9) {
                root.out.title = root.texts(root.find(card, "notesTitle"));
                var nbx = root.find(card, "notesBox"), rls = root.find(card, "restoreList");
                if (nbx && rls) {
                    var pn = nbx.mapToItem(null, 0, 0), pl = rls.mapToItem(null, 0, 0);
                    root.out.sideBySide = pn.x >= pl.x + rls.width - 0.5 && pn.y < pl.y + rls.height && pl.y < pn.y + nbx.height;
                } else root.out.sideBySide = false;
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
                root.out.chatEmpty = root.emptyText(card);
            } else if (root.phase === 13) {
                r = root.row(card, "openNotes", "Fake standup");
                if (r) r.clicked();
                r = root.row(card, "openChat", "Fake chat");
                if (r) r.clicked();   // the notes.get reply of the meeting is still in flight
            } else if (root.phase === 16) {
                root.out.lateReplyHidden = !root.find(card, "notesBox");
                r = root.row(card, "openChat", "Fake chat");
                root.out.whenBefore = root.whenX(r);
                root.out.whenYBefore = root.whenY(r);
                root.out.whenBelowTitle = root.whenY(r) > root.titleY(r, "Fake chat");
                more = root.find(r, "rowMore");
                if (more) more.clicked();
            } else if (root.phase === 17) {
                r = root.row(card, "openChat", "Fake chat");
                root.out.chatDelete = !!root.find(r, "deleteSession");
                root.out.chatNoExport = !root.find(card, "exportNow");
                root.out.whenAfter = root.whenX(r);
                root.out.whenYAfter = root.whenY(r);
                root.out.standupMeta = root.visibleText(root.row(card, "openNotes", "Fake standup"), " lines");
                root.out.quickMeta = root.visibleText(root.row(card, "openNotes", "Quick sync"), " lines");
            } else if (root.phase === 18) {
                r = root.row(card, "openNotes", "Quick sync");
                if (r) r.clicked();
                root.out.quickLoading = root.stateText(card);
            } else if (root.phase === 20) {
                root.out.quickEmpty = root.stateText(card);
                root.out.caps = lib ? lib.capHeadings("# Title\n  ## Sub\n```\n# kept\n```\n~~~\n# kept2\n~~~\n# A **b**\n# C#") : "";
            } else if (root.phase === 21) {
                input = root.find(card, "search");
                if (input) input.text = "zzzz";
                root.out.noResultsEarly = !!root.find(card, "noResults");   // the transcript search has not answered yet
            } else if (root.phase === 23) {
                root.out.noResults = !!root.find(card, "noResults");
                input = root.find(card, "search");
                if (input) input.text = "Fake";
                overlay.live = { x: 40, y: 40, w: 520, h: 420 };   // narrow: 520 x 420 is the smallest card
                overlay.winW = 600; overlay.winH = 500;
            } else if (root.phase === 25) {
                root.out.narrowListShown = root.listShown(card) && root.find(card, "notesBack") === null;
                r = root.row(card, "openNotes", "Fake standup");
                if (r) r.clicked();
            } else if (root.phase === 26) {   // narrow: the list is swapped for the notes
                root.out.swapListHidden = !root.listShown(card);
                root.out.swapBack = !!root.find(card, "notesBack");
                root.out.swapBoxShown = !!root.find(card, "notesBox");
                root.out.backClosed = lib ? lib.back() === true : false;
            } else if (root.phase === 27) {   // back(): the list returns, the search is unchanged
                input = root.find(card, "search");
                root.out.backListShown = root.listShown(card) && !root.find(card, "notesBack");
                root.out.backSearch = input ? String(input.text) : "";
                root.out.backNothingToClose = lib ? lib.back() === false : false;
                r = root.row(card, "openChat", "Fake chat");
                if (r) r.clicked();
            } else if (root.phase === 28) {   // a chat does not swap
                root.out.chatKeepsList = root.listShown(card) && !root.find(card, "notesBack");
                root.out.chatNarrowEmpty = root.emptyText(card) === "" ;
                r = root.row(card, "openNotes", "Fake standup");
                if (r) r.clicked();
            } else if (root.phase === 29) {   // Escape: first back to the list, then the Library closes
                root.out.escSwap = !root.listShown(card) && !!root.find(card, "notesBack");
                root.pressEscape(lib);
            } else if (root.phase === 30) {
                root.out.escList = root.listShown(card) && !root.find(card, "notesBack") && overlay.library;
                root.pressEscape(lib);
            } else if (root.phase === 31) {
                root.out.escClose = overlay.library === false;
                overlay.showLibrary("restore");
            } else if (root.phase === 33) {
                r = root.row(card, "openNotes", "Fake standup");
                if (r) r.clicked();
            } else if (root.phase === 34) {   // typing a search inside the swap leaves it; "No results" stays hidden until back()
                input = root.find(card, "search");
                if (input) input.text = "zzzz";
            } else if (root.phase === 37) {
                root.out.swapNoResultsHidden = !root.find(card, "noResults") && !!root.find(card, "notesBack");
                root.out.backFromSearch = lib ? lib.back() === true : false;
            } else if (root.phase === 38) {
                root.out.noResultsAfterBack = !!root.find(card, "noResults");
                input = root.find(card, "search");
                if (input) input.text = "";
                var units = [{ tag: "w1200x800", w: 0 }, { tag: "w760u", units: 760, h: 700 }, { tag: "w759u", units: 759, h: 700 }, { tag: "n520x420", w: 520, h: 420 }];
                root.queue = [];
                [0, 50].forEach(function(sc) {
                    units.forEach(function(u) { root.queue.push({ tag: u.tag + "@" + (100 + sc), w: u.w, units: u.units, h: u.h, scale: sc }); });
                });
                root.queue.push({ tag: "restore", w: 0 });
                root.queueRun = true;
            } else if (root.phase > 38 && !root.queueRun) {
                console.log("LIBRARY " + JSON.stringify(root.out));
                Qt.quit();
            }
        }
    }
    // one entry per two ticks: apply the geometry, then dump it
    Timer {
        id: qtimer
        property var cur: null
        interval: 400; repeat: true; running: root.queueRun
        onTriggered: {
            var card = overlay.geometryRoot(), lib = root.library(card), e;
            if (qtimer.cur) {
                e = qtimer.cur;
                qtimer.cur = null;
                if (e.tag === "restore") {
                    var r = root.row(card, "openChat", "Fake chat");
                    var rb = r ? root.find(r, "restore") : null;
                    root.out.restoreBtn = !!rb;
                    if (rb) rb.clicked();   // one click: the daemon log must hold session.restore
                } else {
                    root.out["wide@" + e.tag] = lib.wideNotes;
                    Geo.dump(card, overlay.ctl, "library-" + e.tag, overlay.winW, overlay.winH);
                }
                return;
            }
            if (!root.queue.length) { root.queueRun = false; return; }
            e = root.queue.shift();
            root.applyEntry(e, lib);   // "restore" runs at the default wide card, 100%
            if (e.tag !== "restore") {
                if (e.tag.indexOf("w1200") === 0 || e.tag.indexOf("n520") === 0) { var s = root.row(card, "openNotes", "Fake standup"); if (s) s.clicked(); }
            }
            qtimer.cur = e;
        }
    }
}
