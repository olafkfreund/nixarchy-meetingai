.pragma library
// Geometry dump for tests/check-geometry.py. dump(card, ctl) logs one "GEO {json}" line:
// every visible control (IconButton, Input, Dropdown) and every visible Text, in card coordinates.

function isControl(it) {
    return (it.tooltipText !== undefined && it.iconText !== undefined)   // IconButton
        || (it.placeholderText !== undefined && it.echoMode !== undefined)   // Input / TextField
        || it.rowHeight !== undefined;   // Dropdown, SearchableDropdown
}
function isText(it) { return it.truncated !== undefined && it.textFormat !== undefined && it.elide !== undefined; }

function walk(it, card, out, depth, vp) {
    if (!it || !it.visible || depth > 60) return;
    if (it.clip === true) {   // a clipping parent (Flickable, ListView): what is outside it is not on screen
        var q = it.mapToItem(card, 0, 0);
        vp = { x0: Math.max(vp.x0, q.x), y0: Math.max(vp.y0, q.y), x1: Math.min(vp.x1, q.x + it.width), y1: Math.min(vp.y1, q.y + it.height) };
    }
    var kids = it.children || [];
    var ctl = isControl(it), txt = !ctl && isText(it);
    var p = (ctl || txt) ? it.mapToItem(card, 0, 0) : null;
    // only what is fully inside every clipping ancestor: a half-visible control tells nothing about alignment
    if (p && it.width > 0 && it.height > 0 && p.x >= vp.x0 - 1 && p.y >= vp.y0 - 1 && p.x + it.width <= vp.x1 + 1 && p.y + it.height <= vp.y1 + 1) {
        var r = { n: String(it.objectName || ""), t: String(it).split("(")[0], x: p.x, y: p.y, w: it.width, h: it.height };
        if (ctl) {
            r.c = 1;
            r.label = String(it.text || it.placeholderText || it.iconText || "");
            r.clip = it.iconText !== undefined && it.implicitWidth > it.width + 1;
            out.push(r);
        } else if (it.text !== "") {
            r.label = String(it.text).slice(0, 40);
            // wrapped text grows downwards; elided text is intended; plain text wider than its box spills out
            r.clip = it.elide === 0 && it.wrapMode === 0 && it.implicitWidth > it.width + 1;
            out.push(r);
        }
    }
    for (var i = 0; i < kids.length; i++) walk(kids[i], card, out, depth + 1, vp);
}

function dump(card, ctl, tag) {
    var out = [];
    walk(card, card, out, 0, { x0: 0, y0: 0, x1: card.width, y1: card.height });
    console.log("GEO " + JSON.stringify({ tag: tag, ctl: ctl, w: card.width, h: card.height, items: out }));
}
