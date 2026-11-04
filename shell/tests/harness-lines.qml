import QtQuick
import Quickshell
import "geometry.js" as Geo

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
    function client(it, depth) {
        if (!it || (depth || 0) > 60) return null;
        if (typeof it.openLine === "function" && it.transcript !== undefined) return it;
        var kids = it.children || [];
        for (var i = 0; i < kids.length; i++) { var f = client(kids[i], (depth || 0) + 1); if (f) return f; }
        return null;
    }
    function composer(it, depth) {
        if (!it || (depth || 0) > 60) return null;
        if (typeof it.addChip === "function" && typeof it.send === "function") return it;
        var kids = it.children || [];
        for (var i = 0; i < kids.length; i++) { var f = composer(kids[i], (depth || 0) + 1); if (f) return f; }
        return null;
    }
    function visibleText(it, needle, depth) {
        if (!it || !it.visible || (depth || 0) > 60) return "";
        if (typeof it.text === "string" && it.text.indexOf(needle) >= 0) return String(it.text);
        var kids = it.children || [];
        for (var i = 0; i < kids.length; i++) { var s = visibleText(kids[i], needle, (depth || 0) + 1); if (s) return s; }
        return "";
    }
    function byLabel(it, needle, depth) {
        if (!it || !it.visible || (depth || 0) > 60) return null;
        var lbl = String(it.text || it.tooltipText || it.accName || it.Accessible.name || "");
        if (it.tooltipText !== undefined && lbl.toLowerCase().indexOf(needle.toLowerCase()) >= 0) return it;
        var kids = it.children || [];
        for (var i = 0; i < kids.length; i++) { var f = byLabel(kids[i], needle, (depth || 0) + 1); if (f) return f; }
        return null;
    }
    function allByLabel(it, needle, acc, depth) {
        if (!it || !it.visible || (depth || 0) > 60) return acc;
        var lbl = String(it.text || it.tooltipText || it.accName || it.Accessible.name || "");
        if (it.tooltipText !== undefined && lbl.toLowerCase().indexOf(needle.toLowerCase()) >= 0) acc.push(it);
        var kids = it.children || [];
        for (var i = 0; i < kids.length; i++) allByLabel(kids[i], needle, acc, (depth || 0) + 1);
        return acc;
    }
    function rowItem(it, lineId, depth) {
        if (!it || !it.visible || (depth || 0) > 60) return null;
        if (it.lineId === lineId) return it;
        var kids = it.children || [];
        for (var i = 0; i < kids.length; i++) { var f = rowItem(kids[i], lineId, (depth || 0) + 1); if (f) return f; }
        return null;
    }
    function editField(it, depth) {
        if (!it || !it.visible || (depth || 0) > 60) return null;
        if (it.objectName === "correctInput" || it.objectName === "editInput" || it.objectName === "lineInput" || it.objectName === "renameInput") return it;
        if (it.placeholderText !== undefined && it.echoMode !== undefined && it.objectName !== "composerInput" && it.objectName !== "search") return it;
        var kids = it.children || [];
        for (var i = 0; i < kids.length; i++) { var f = editField(kids[i], (depth || 0) + 1); if (f) return f; }
        return null;
    }
    function press(it) {
        if (!it) return false;
        if (typeof it.clicked === "function") {
            try { it.clicked(); } catch (e) { it.clicked(null); }
            return true;
        }
        var kids = it.children || [];
        for (var i = 0; i < kids.length; i++) if (press(kids[i])) return true;
        return false;
    }
    function clickInNotes(card, needle) {
        var nb = root.find(card, "notesBox") || card;
        var btn = root.byLabel(nb, needle);
        if (btn && root.press(btn)) return true;
        var walk = function(it, depth) {
            if (!it || !it.visible || (depth || 0) > 60) return null;
            if (typeof it.text === "string" && it.text.indexOf(needle) >= 0) {
                var p = it;
                while (p && p !== nb) {
                    if (typeof p.clicked === "function") return p;
                    var ks = p.children || [];
                    for (var j = 0; j < ks.length; j++) if (ks[j].visible && typeof ks[j].clicked === "function") return ks[j];
                    p = p.parent;
                }
            }
            var kids = it.children || [];
            for (var i = 0; i < kids.length; i++) { var f = walk(kids[i], (depth || 0) + 1); if (f) return f; }
            return null;
        };
        return root.press(walk(nb, 0));
    }
    function findRow(c, id) {
        for (var i = 0; c && i < c.transcript.count; i++) if (c.transcript.get(i).lineId === id) return c.transcript.get(i);
        return null;
    }

    Overlay { id: overlay; Component.onCompleted: overlay.open("{}") }

    Timer {
        interval: 500; repeat: true; running: true
        onTriggered: {
            root.phase++;
            var card = overlay.geometryRoot(), c = root.client(overlay), comp = root.composer(overlay);
            var r1, rs, more, corr, bm, askBtn, fld, noteBtn, regBtn;
            if (root.phase === 2) {
                overlay.showLibrary("settings");   // load session.list on tab change
            } else if (root.phase === 3) {
                overlay.showLibrary("restore");
                noteBtn = root.find(card, "openNotes");
                if (noteBtn) root.press(noteBtn);
            } else if (root.phase === 4) {
                // Before any edit, notes are not stale and Regenerate is hidden
                regBtn = root.find(card, "regenerateNotes") || root.find(card, "regenerate") || root.byLabel(card, "Regenerate");
                root.out.regenerateHiddenWhenFresh = !regBtn && !root.find(card, "notesStale");
                overlay.library = false;
            } else if (root.phase === 5) {
                // Shortened live row hides Correct; line-1 opens its "⋯" menu
                rs = root.rowItem(card, "live-short");
                if (rs) {
                    more = root.find(rs, "lineMore") || root.find(rs, "rowMore") || root.byLabel(rs, "⋯") || root.byLabel(rs, "more");
                    if (more) root.press(more);
                    corr = root.find(rs, "correctLine") || root.find(rs, "correct") || root.byLabel(rs, "Correct");
                    root.out.shortenedHidesCorrect = !corr;
                }
                r1 = root.rowItem(card, "line-1");
                more = r1 ? (root.find(r1, "lineMore") || root.find(r1, "rowMore") || root.byLabel(r1, "⋯") || root.byLabel(r1, "more")) : null;
                if (more) root.press(more);
            } else if (root.phase === 6) {
                r1 = root.rowItem(card, "line-1");
                Geo.dump(card, overlay.ctl, "lines-menu", overlay.winW, overlay.winH);
                corr = r1 ? (root.find(r1, "correctLine") || root.find(r1, "correct") || root.byLabel(r1, "Correct")) : null;
                if (corr) root.press(corr);
                fld = r1 ? root.editField(r1, 0) : root.editField(card, 0);
                if (fld) { fld.text = "Corrected Kafka lag"; fld.accepted(); }
            } else if (root.phase === 7) {
                var m1 = root.findRow(c, "line-1");
                r1 = root.rowItem(card, "line-1");
                root.out.correctedText = (!!m1 && m1.body === "Corrected Kafka lag") && root.visibleText(card, "Corrected Kafka lag") !== "";
                root.out.editedMark = (!!m1 && m1.edited === true && m1.heard === "Misheard Kafka lag") || !!root.find(card, "editedMark") || root.visibleText(card, "edited") !== "";
                root.out.changedAfter = (!!c && (c.now.changedAfter === true || c.now.renamedAfter === true)) || !!root.find(card, "nowChangedAfter") || !!root.find(card, "nowNamesChanged") || root.visibleText(card, "changed after") !== "";
                // Bookmark line-1
                more = r1 ? (root.find(r1, "lineMore") || root.find(r1, "rowMore") || root.byLabel(r1, "⋯") || root.byLabel(r1, "more")) : null;
                if (more && !(root.find(r1, "bookmarkLine") || root.find(r1, "bookmark") || root.byLabel(r1, "Bookmark"))) root.press(more);
                bm = r1 ? (root.find(r1, "bookmarkLine") || root.find(r1, "bookmark") || root.byLabel(r1, "Bookmark")) : null;
                if (bm) root.press(bm);
            } else if (root.phase === 8) {
                var mb = root.findRow(c, "line-1");
                root.out.bookmarkedBefore = (!!mb && mb.bookmarked === true) || root.visibleText(card, "\u2605") !== "";
                if (c) c.rebuild();
            } else if (root.phase === 9) {
                var ma = root.findRow(c, "line-1");
                root.out.bookmarkReloaded = (!!ma && ma.bookmarked === true) && (root.visibleText(card, "\u2605") !== "" || !!root.find(card, "bookmarkStar") || !!root.find(card, "bookmarkedMark"));
                // Ask about line-1 with empty input -> sends default text and line-1
                r1 = root.rowItem(card, "line-1");
                more = r1 ? (root.find(r1, "lineMore") || root.find(r1, "rowMore") || root.byLabel(r1, "⋯") || root.byLabel(r1, "more")) : null;
                if (more && !(root.find(r1, "askLine") || root.find(r1, "ask") || root.byLabel(r1, "Ask"))) root.press(more);
                askBtn = r1 ? (root.find(r1, "askLine") || root.find(r1, "ask") || root.byLabel(r1, "Ask")) : null;
                if (askBtn) root.press(askBtn);
                root.out.quoteShown = (!!c && !!c.quote && c.quote.line === "line-1") || !!root.find(card, "quoteChip");
                var ci = root.find(card, "composerInput");
                if (ci) ci.text = "";
                if (comp) comp.send();
            } else if (root.phase === 11) {
                root.out.quoteCleared = !c || !c.quote || !c.quote.line;
                // Open Library -> Restore -> select Fake standup ("1"): notes are now stale after the edit
                overlay.showLibrary("restore");
                noteBtn = root.find(card, "openNotes");
                if (noteBtn) root.press(noteBtn);
            } else if (root.phase === 12) {
                Geo.dump(card, overlay.ctl, "meeting-view", overlay.winW, overlay.winH);
                var staleCap = root.find(card, "notesStale");
                regBtn = root.find(card, "regenerateNotes") || root.find(card, "regenerate") || root.byLabel(card, "Regenerate");
                root.out.staleShown = !!staleCap && String(staleCap.text).indexOf("Names or lines changed after these notes were written") >= 0;
                root.out.regenerateShownWhenStale = !!regBtn;
                root.out.timelineShown = root.visibleText(card, "Discussed the rollout window.") !== "";
                root.out.itemsShown = root.visibleText(card, "Pad the migration by two days") !== "";
                root.out.bookmarksShown = root.visibleText(card, "Past bookmark moment") !== "";
                if (regBtn) root.press(regBtn);
            } else if (root.phase === 13) {
                regBtn = root.find(card, "regenerateNotes") || root.find(card, "regenerate") || root.byLabel(card, "Regenerate");
                root.out.regenerated = !root.find(card, "notesStale") && !regBtn && root.visibleText(card, "Regenerated notes") !== "";
                // Click timeline link (1:00–3:00 -> past-chunk)
                if (!root.press(root.find(card, "chunkLink0") || root.find(card, "timelineLink0") || root.find(card, "chunk0"))) {
                    root.clickInNotes(card, "1:00");
                }
            } else if (root.phase === 14) {
                var rc = root.findRow(c, "past-chunk");
                root.out.chunkJump = !!c && c.currentMeeting === "1" && !!rc && rc.flash === true;
                // Back to m1 so the next link also tests restoring meeting "1"
                if (c) c.send("session.restore", { id: "m1" });
                overlay.showLibrary("restore");
                noteBtn = root.find(card, "openNotes");
                if (noteBtn) root.press(noteBtn);
            } else if (root.phase === 15) {
                // Click item link (2:05 -> past-item)
                if (!root.press(root.find(card, "itemLink0") || root.find(card, "meetingItem0") || root.find(card, "item0"))) {
                    root.clickInNotes(card, "2:05");
                }
            } else if (root.phase === 16) {
                var ri = root.findRow(c, "past-item");
                root.out.itemJump = !!c && c.currentMeeting === "1" && !!ri && ri.flash === true;
                if (c) c.send("session.restore", { id: "m1" });
                overlay.showLibrary("restore");
                noteBtn = root.find(card, "openNotes");
                if (noteBtn) root.press(noteBtn);
            } else if (root.phase === 17) {
                // Click bookmark link (4:00 -> past-bmark)
                if (!root.press(root.find(card, "bookmarkLink0") || root.find(card, "bookmark0"))) {
                    if (!root.clickInNotes(card, "4:00")) root.clickInNotes(card, "Past bookmark moment");
                }
            } else if (root.phase === 18) {
                var rb = root.findRow(c, "past-bmark");
                root.out.bookmarkJump = !!c && c.currentMeeting === "1" && !!rb && rb.flash === true;
                // Restored answer from history shows Based on chips with [1] stripped
                var ans = null;
                for (var k = 0; c && k < c.transcript.count; k++) if (c.transcript.get(k).who === "answer") ans = c.transcript.get(k);
                var ansText = root.visibleText(card, "See the rollout window");
                var chip = root.find(card, "sourceChip0");
                root.out.restoredChips = !!ans && !!root.find(card, "answerSources") && !!chip && String(chip.text) === "1:00 \u00b7 Alex" && ansText !== "" && ansText.indexOf("[1]") < 0;
                // Also test Open transcript button in meeting view
                if (c) c.send("session.restore", { id: "m1" });
                overlay.showLibrary("restore");
                noteBtn = root.find(card, "openNotes");
                if (noteBtn) root.press(noteBtn);
            } else if (root.phase === 19) {
                var openTr = root.find(card, "openTranscript") || root.byLabel(card, "Open transcript");
                root.out.hasOpenTranscript = !!openTr;
                if (openTr) root.press(openTr);
            } else if (root.phase === 20) {
                root.out.openedTranscript = !!c && c.currentMeeting === "1";
                console.log("LINES " + JSON.stringify(root.out));
                Qt.quit();
            }
        }
    }
}
