.pragma library
// Geometry dump for tests/check-geometry.py. dump(card, ctl, tag, ww, wh) logs one "GEO {json}" line:
// every visible control (IconButton, Input, Select, toggle, slider) and every visible Text, in card
// coordinates, including the content of an open Select or mode-picker popup (ww x wh: the window).

function isButton(it) { return it.tooltipText !== undefined && it.iconText !== undefined; }   // IconButton
function isField(it) { return it.placeholderText !== undefined && it.echoMode !== undefined; }   // Input / TextField
function isToggle(it) { return it.checked !== undefined && it.accName !== undefined && it.tooltipText === undefined; }   // Check
function isSlider(it) { return it.knobSize !== undefined && it.trackHeight !== undefined && it.minimum !== undefined; }   // PanelSlider
function isControl(it) {
    return isButton(it) || isField(it)
        || it.fontPx !== undefined      // Select
        || it.rowHeight !== undefined   // qs.Ui Dropdown / SearchableDropdown: fixed font size, must not be used
        || isToggle(it) || isSlider(it);
}
function isText(it) { return it.truncated !== undefined && it.textFormat !== undefined && it.elide !== undefined; }

function walk(it, card, out, depth, vp, pop) {
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
        if (pop) r.pop = 1;
        if (ctl) {
            r.c = 1;
            r.k = isToggle(it) ? "toggle" : isSlider(it) ? "slider" : "";   // not button-tall: aligned by their centre
            r.fpx = it.fontPx !== undefined ? it.fontPx : it.fontSize !== undefined ? it.fontSize : (it.font ? it.font.pixelSize : 0);
            r.acc = String(it.accName !== undefined ? it.accName : "");
            r.ui = it.rowHeight !== undefined && it.fontPx === undefined;
            r.label = String(it.text || it.placeholderText || it.iconText || it.accName || "");
            r.clip = it.iconText !== undefined && !it.elidable && it.implicitWidth > it.width + 1;
            out.push(r);
        } else if (it.text !== "") {
            r.label = String(it.text).slice(0, 40);
            // wrapped text grows downwards; elided text is intended; plain text wider than its box spills out
            r.clip = it.elide === 0 && it.wrapMode === 0 && it.implicitWidth > it.width + 1;
            r.cut = it.truncated === true && it.mayCut !== true;   // text lost to elision or a line limit that the design does not intend
            r.ch = it.contentHeight; r.ih = it.implicitHeight;
            r.vo = it.contentHeight > it.height + 1.5;   // taller than its box (a fixed height under wrapped text)
            out.push(r);
        }
    }
    if (it.popupOpen === true && it.popupContent) walk(it.popupContent, card, out, depth + 1, { x0: -1e6, y0: -1e6, x1: 1e6, y1: 1e6 }, true);
    for (var i = 0; i < kids.length; i++) walk(kids[i], card, out, depth + 1, vp, pop);
}

function dump(card, ctl, tag, ww, wh) {
    var out = [], o = card.mapToItem(null, 0, 0);
    walk(card, card, out, 0, { x0: 0, y0: 0, x1: card.width, y1: card.height }, false);
    console.log("GEO " + JSON.stringify({ tag: tag, ctl: ctl, w: card.width, h: card.height, ox: o.x, oy: o.y, ww: ww || 0, wh: wh || 0, items: out }));
}

function find(it, name, depth) {   // the visible Select or mode picker with this objectName
    if (!it || !it.visible || (depth || 0) > 60) return null;
    if (it.objectName === name && it.popupContent !== undefined) return it;
    var kids = it.children || [];
    for (var i = 0; i < kids.length; i++) { var f = find(kids[i], name, (depth || 0) + 1); if (f) return f; }
    return null;
}
// Open the Select `name` (closing the one open before), so the next dump includes its popup.
function openSelect(card, prev, name) {
    var p = prev ? find(card, prev) : null, s = find(card, name);
    if (p && p.closePopup) p.closePopup();
    if (s && s.openPopup) s.openPopup();
}
