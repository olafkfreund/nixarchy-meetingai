import QtQuick
import Quickshell
import "geometry.js" as Geo

ShellRoot {
    id: root
    property int phase: 0
    property int waited: 0
    property var out: ({})
    function find(it, name, depth) {
        if (!it || !it.visible || (depth || 0) > 60) return null;
        if (it.objectName === name) return it;
        var kids = it.children || [];
        for (var i = 0; i < kids.length; i++) { var f = find(kids[i], name, (depth || 0) + 1); if (f) return f; }
        return null;
    }
    function client(it, depth) {
        if (!it || (depth || 0) > 60) return null;
        if (typeof it.openLine === "function" && it.transcript !== undefined) return it;
        var kids = it.children || [];
        for (var i = 0; i < kids.length; i++) { var f = client(kids[i], (depth || 0) + 1); if (f) return f; }
        return null;
    }
    function visibleText(it, needle, depth) {
        if (!it || !it.visible || (depth || 0) > 60) return "";
        if (typeof it.text === "string" && it.text.indexOf(needle) >= 0) return String(it.text);
        var kids = it.children || [];
        for (var i = 0; i < kids.length; i++) { var s = visibleText(kids[i], needle, (depth || 0) + 1); if (s) return s; }
        return "";
    }
    Overlay { id: overlay; Component.onCompleted: overlay.open("{}") }
    Timer {
        interval: 500; repeat: true; running: true
        onTriggered: {
            if (root.phase === 1 && root.waited++ < 16) {
                var readyCard = overlay.geometryRoot();
                var readySources = root.find(readyCard, "nowSources");
                var readyChip0 = root.find(readySources, "sourceChip0");
                var readyChip1 = root.find(readySources, "sourceChip1");
                var readyLabel = root.find(readyCard, "bubbleLabel");
                if (!readyChip0 || !readyChip1 || !readyChip0.text || !readyChip1.text || !readyLabel || String(readyLabel.text).indexOf("12:04") < 0) return;
            }
            root.phase++;
            var card = overlay.geometryRoot(), c = root.client(overlay), src, chip, label, input, hit, mouse;
            if (root.phase === 2) {
                src = root.find(card, "nowSources");
                chip = root.find(src, "sourceChip0");
                root.out.lineChip = chip ? String(chip.text) : "";
                chip = root.find(src, "sourceChip1");
                root.out.notesChip = chip ? String(chip.text) : "";
                label = root.find(card, "bubbleLabel");
                root.out.bubbleStamp = !!label && String(label.text).indexOf("12:04") >= 0;
                var shown = root.visibleText(card, "Launch Friday");
                root.out.markersHidden = shown !== "" && shown.indexOf("[1]") < 0 && shown.indexOf("[N1]") < 0;
                Geo.dump(card, overlay.ctl, "grounded-chips", overlay.winW, overlay.winH);
                c.send("test.next", {});
            } else if (root.phase === 4) {
                root.out.noCitations = !!root.find(card, "noCitations");
                root.out.archivedChips = !!root.find(card, "answerSources") && !!root.find(card, "sourceChip0");
                root.out.staleSuggestionHidden = !!c && c.suggestions.indexOf("STALE SUGGESTION") < 0 && c.suggestions.indexOf("Current follow-up") >= 0;
            } else if (root.phase === 5) {
                label = root.find(card, "renameLabel");
                if (label) label.clicked(null);
                input = root.find(card, "renameInput");
                if (input) { input.text = "Alex"; input.accepted(); }
            } else if (root.phase === 6) {
                label = root.find(card, "bubbleLabel");
                root.out.renamed = !!label && String(label.text).indexOf("Alex") >= 0;
                root.out.namesChanged = !!root.find(card, "namesChanged");
                root.out.nowNamesChanged = !!root.find(card, "nowNamesChanged");
            } else if (root.phase === 7) {
                overlay.showLibrary("restore");
                input = root.find(card, "search");
                if (input) input.text = "migration";
            } else if (root.phase === 9) {
                root.out.linesBox = !!root.find(card, "linesBox");
                var hits = root.find(card, "lineHits");
                root.out.lineHits = !!hits && hits.model.length === 2 && !!root.find(card, "lineHit0");
                hit = root.find(card, "lineHit0");
                if (hit) {
                    for (var i = 0; i < hit.children.length; i++) if (typeof hit.children[i].clicked === "function") { mouse = hit.children[i]; break; }
                    if (mouse) mouse.clicked(null);
                }
            } else if (root.phase === 10) {
                root.out.past = !!c && c.viewingPast;
                root.out.flashed = !!c && c.transcript.count > 0 && c.transcript.get(0).lineId === "old-1" && c.transcript.get(0).flash;
                root.out.oldInView = root.visibleText(card, "Old migration detail") !== "";
                root.out.backButton = !!root.find(card, "backToLatest");
                var back = root.find(card, "backToLatest");
                if (back) back.clicked();
            } else if (root.phase === 11) {
                root.out.latest = !!c && !c.viewingPast && c.transcript.count > 0 && c.transcript.get(0).lineId === "line-2";
                overlay.showLibrary("settings");   // the session list loads on a tab change
            } else if (root.phase === 12) {
                overlay.showLibrary("restore");
                input = root.find(card, "search");
                if (input) input.text = "";
            } else if (root.phase === 13) {
                var notes = root.find(card, "openNotes");
                if (notes) notes.clicked();
            } else if (root.phase === 14) {
                root.out.notesStale = !!root.find(card, "notesStale");
                input = root.find(card, "search");
                if (input) input.text = "other migration";
            } else if (root.phase === 16) {
                hit = root.find(card, "lineHit0");
                if (hit) {
                    for (var j = 0; j < hit.children.length; j++) if (typeof hit.children[j].clicked === "function") { mouse = hit.children[j]; break; }
                    if (mouse) mouse.clicked(null);
                }
            } else if (root.phase === 17) {
                var at = -1;
                for (var k = 0; c && k < c.transcript.count; k++) if (c.transcript.get(k).lineId === "other-1") at = k;
                root.out.otherMeeting = !!c && c.currentMeeting === "m2" && at >= 0 && c.transcript.get(at).flash;
                console.log("GROUNDED " + JSON.stringify(root.out));
                Qt.quit();
            }
        }
    }
}
