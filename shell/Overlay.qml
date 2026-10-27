pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects (Style.spacing, Color.popups) the linter cannot type
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Wayland
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// Meeting AI overlay: transcript, streamed answer, follow-ups, selectors and
// the question box. `omarchy-shell meetingai toggle` opens it.
Item {
    id: root

    property var shell: null
    property var manifest: null
    property bool opened: false
    property bool library: false
    property bool setupDismissed: false
    property string seriesError: ""
    onLibraryChanged: { backend.libraryOpen = root.library; if (!root.library) Qt.callLater(function() { composer.focusInput(); }); }
    property string previewText: ""

    // MarkdownText ignores linkColor and paints links web-blue; inline HTML keeps the theme accent.
    function themedLinks(md) {
        var c = String(Commons.Color.accent);
        return String(md).replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, '<a href="$2"><span style="color:' + c + '">$1</span></a>');
    }
    property var providers: []
    property var models: []
    property bool noContext: false
    property real now: Date.now()
    property bool transcriptOpen: true
    readonly property bool tList: root.transcriptOpen && backend.transcript.count > 0   // an empty or collapsed transcript gives its room to NOW
    readonly property bool tEmpty: root.transcriptOpen && backend.transcript.count === 0 && !root.panel   // nothing to list: NOW sits right under the hint line
    readonly property string status: backend.answering ? "Writing" : backend.now.error ? "Failed" : backend.now.body !== "" ? "Ready" : "Listening"

    readonly property string pluginId: String((root.manifest && root.manifest.id) || "nixarchy.meetingai")
    // popups.background carries the theme alpha; the overlay must not show what is behind it.
    readonly property color solid: Qt.rgba(Commons.Color.popups.background.r, Commons.Color.popups.background.g, Commons.Color.popups.background.b, 1)
    readonly property string elapsed: {
        var t = Math.max(0, Math.floor((root.now - (backend.state.since || root.now)) / 1000));
        var p = function(n) { return (n < 10 ? "0" : "") + n; };
        return Math.floor(t / 3600) + ":" + p(Math.floor(t / 60) % 60) + ":" + p(t % 60);
    }
    readonly property real ts: backend.textScale
    Theme { id: metrics; ts: root.ts }   // the one metric: control height, padding, radius, font sizes
    readonly property bool team: backend.settings.meeting_mode === "team_meeting"
    readonly property bool iv: backend.settings.meeting_mode === "interviewer"
    readonly property bool panel: root.team || root.iv   // the card above NOW: Items (team) or Question bank (interviewer)
    readonly property string seriesId: String(backend.state.series || "")
    readonly property string meetingId: String(backend.state.meeting || "")
    property bool lastOpen: false
    property var lastTime: null   // last_time.get reply for the current series
    function loadLastTime() {
        root.lastTime = null;
        if (root.team && root.seriesId !== "") backend.send("last_time.get", { series: root.seriesId }, function(r) { if (r.ok) root.lastTime = r.data; });
    }
    onTeamChanged: loadLastTime()
    onSeriesIdChanged: loadLastTime()
    onMeetingIdChanged: loadLastTime()
    property var liveModels: []
    // Card geometry. Settings overlay_x/y/w/h (-1 = default, centred) persist it; `live` overrides while dragging.
    readonly property real minW: 520
    readonly property real minH: 420
    readonly property real edgeGap: Style.space(40)
    property var live: null
    property real winW: 1200
    property real winH: 800
    function clamp(v, lo, hi) { return Math.max(lo, Math.min(hi, v)); }
    readonly property var geom: {
        var s = backend.settings, r = root.live;
        var mw = Math.max(root.minW, root.winW - root.edgeGap * 2), mh = Math.max(root.minH, root.winH - root.edgeGap * 2);
        var w = r ? r.w : (Number(s.overlay_w) > 0 ? Number(s.overlay_w) : Math.min(mw, Style.space(900)));
        var h = r ? r.h : (Number(s.overlay_h) > 0 ? Number(s.overlay_h) : Math.min(mh, Style.space(760)));
        w = root.clamp(w, root.minW, mw); h = root.clamp(h, root.minH, mh);
        var x = r ? r.x : (Number(s.overlay_x) >= 0 ? Number(s.overlay_x) : (root.winW - w) / 2);
        var y = r ? r.y : (Number(s.overlay_y) >= 0 ? Number(s.overlay_y) : (root.winH - h) / 2);
        return { x: root.clamp(x, 0, Math.max(0, root.winW - w)), y: root.clamp(y, 0, Math.max(0, root.winH - h)), w: w, h: h };
    }
    property var grab: null
    function beginGrab(gx, gy) { root.grab = { gx: gx, gy: gy, r: root.geom }; }
    // edges: any of "l" "r" "t" "b"; "m" moves
    function dragTo(edges, gx, gy) {
        var g = root.grab, o = g.r, dx = gx - g.gx, dy = gy - g.gy;
        var mw = Math.max(root.minW, root.winW - root.edgeGap * 2), mh = Math.max(root.minH, root.winH - root.edgeGap * 2);
        var n = { x: o.x, y: o.y, w: o.w, h: o.h };
        if (edges === "m") { n.x = o.x + dx; n.y = o.y + dy; }
        if (edges.indexOf("r") >= 0) n.w = root.clamp(o.w + dx, root.minW, mw);
        if (edges.indexOf("b") >= 0) n.h = root.clamp(o.h + dy, root.minH, mh);
        if (edges.indexOf("l") >= 0) { n.w = root.clamp(o.w - dx, root.minW, mw); n.x = o.x + o.w - n.w; }
        if (edges.indexOf("t") >= 0) { n.h = root.clamp(o.h - dy, root.minH, mh); n.y = o.y + o.h - n.h; }
        root.live = n;
    }
    function endGrab() {   // persist on release, not on every move
        var g = root.live;
        root.grab = null;
        if (!g) return;
        backend.setSetting("overlay_x", Math.round(g.x)); backend.setSetting("overlay_y", Math.round(g.y));
        backend.setSetting("overlay_w", Math.round(g.w)); backend.setSetting("overlay_h", Math.round(g.h));
        root.live = null;
    }
    function resetGeom() { ["overlay_x", "overlay_y", "overlay_w", "overlay_h"].forEach(function(k) { backend.setSetting(k, -1); }); }
    readonly property string profileName: {
        var p = backend.profiles.filter(function(q) { return q.selected; });
        return p.length ? String(p[0].name) : "";
    }
    readonly property var paces: ["Fast", "Balanced", "Relaxed"]

    Client { id: backend }
    Timer { running: root.opened && backend.recording; interval: 1000; repeat: true; triggeredOnStart: true; onTriggered: root.now = Date.now() }

    function open(payloadJson) {
        root.opened = true;
        root.setupDismissed = false;
        root.loadModels();
        Qt.callLater(function() { composer.focusInput(); });
    }

    // Hooks for tests/shots.sh.
    function openModelPicker() { root.library = true; lib.tab = "settings"; }
    function showLibrary(tab) { root.library = true; lib.tab = tab; }
    function recordHook(consent) { backend.startRecording(consent); }
    function candidateHook(what) { lib.candidateHook(what); }
    function modeHook() { hdr.openPicker(); }   // tests/harness-live.qml opens the mode list for a crop
    function ask(message, confirmText, yes) { confirm.ask(message, confirmText, yes); }   // the one confirmation dialog
    function focusProbe(next) {   // tests/keys.sh: what has the keyboard focus; next = do what Tab does first
        var w = card.Window.window;
        if (!w) return "no-window";
        if (next && w.activeFocusItem) { const n = w.activeFocusItem.nextItemInFocusChain(true); if (n) n.forceActiveFocus(Qt.TabFocusReason); }
        return String(w.activeFocusItem);
    }
    function focusStart() { card.forceActiveFocus(); }   // tests/keys.sh: Tab from the card, so the first stop is the header
    function dropDump() {   // tests/smoke.sh --drop: what the transcript holds after a reconnect
        const rows = [];
        for (let i = 0; i < backend.transcript.count; i++) rows.push(backend.transcript.get(i).who + "|" + backend.transcript.get(i).body);
        return JSON.stringify({ rows: rows, now: backend.now.body, answering: backend.answering, connected: backend.connected });
    }
    function geometryRoot() { return card; }   // tests/geometry.js walks this item
    readonly property real ctl: metrics.ctl

    function close() {
        root.opened = false;
        if (root.shell && typeof root.shell.hide === "function") root.shell.hide(root.pluginId);
    }

    function bumpScale(delta) {
        var cur = Math.round(backend.textScale * 100);
        backend.setSetting("text_scale", delta === 0 ? 100 : Math.max(80, Math.min(160, cur + delta)));
    }

    // Each model list belongs to the provider picked above it. A reply for a provider that is no longer
    // picked is dropped; a failed list clears the dropdown and shows the error with the fix.
    readonly property string chatProvider: String(backend.settings.chat_provider || "")
    readonly property string liveProvider: String(backend.settings.live_provider || "")
    property string resetChat: ""   // set by setProvider: when this provider's list arrives, its first model replaces the old model
    property string resetLive: ""   // (a provider, not a flag: a settings echo can flip the provider back and forth before the list arrives)
    property string modelsError: ""
    property string liveModelsError: ""
    property var pstatus: ({})   // providers.status by id, for the fix shown under a failed list
    onChatProviderChanged: root.loadModelList(false, true)
    onLiveProviderChanged: root.loadModelList(true, true)

    function loadModels() {
        backend.send("providers.list", {}, function(r) { if (r.ok) root.providers = r.data || []; });
        root.loadModelList(false, false);
        root.loadModelList(true, false);
    }
    function loadModelList(live, clear) {
        var p = live ? root.liveProvider : root.chatProvider;
        if (clear) { if (live) { root.liveModels = []; root.liveModelsError = ""; } else { root.models = []; root.modelsError = ""; } }
        if (p === "") return;
        backend.send("models.list", { provider: p }, function(r) {
            if ((live ? root.liveProvider : root.chatProvider) !== p || (r.provider !== undefined && r.provider !== p)) return;
            var list = r.ok ? (r.data || []).map(String) : [], err = r.ok ? "" : String(r.error || "request failed"), reset = (live ? root.resetLive : root.resetChat) === p;
            if (reset) { if (live) root.resetLive = ""; else root.resetChat = ""; }
            if (live) { root.liveModels = list; root.liveModelsError = err; } else { root.models = list; root.modelsError = err; }
            if (!r.ok) backend.send("providers.status", {}, function(s) { if (s.ok) { var m = {}; (s.data || []).forEach(function(row) { m[row.id] = row; }); root.pstatus = m; } });
            else if (reset && list.length > 0) backend.setSetting(live ? "live_model" : "chat_model", list[0]);
        });
    }
    function fix(id) { var s = root.pstatus[id]; return s && s.state !== "ready" && s.login ? " Fix: " + s.login + "." : ""; }
    function setProvider(live, id) {   // the provider pickers, and tests/harness-providers.qml
        if (id === (live ? root.liveProvider : root.chatProvider)) return;
        if (live) root.resetLive = id; else root.resetChat = id;
        backend.setSetting(live ? "live_provider" : "chat_provider", id);
        backend.setSetting(live ? "live_model" : "chat_model", "");
    }
    function providerDump() {   // tests/harness-providers.qml
        return JSON.stringify({ chat: root.chatProvider, models: root.models, error: root.modelsError, fix: root.fix(root.chatProvider), model: String(backend.settings.chat_model || ""), live: root.liveProvider, liveModels: root.liveModels, liveError: root.liveModelsError, liveModel: String(backend.settings.live_model || "") });
    }

    function submit(text, chips) {
        var files = chips.filter(function(c) { return c.kind === "file"; }).map(function(c) { return c.path; });
        var inline = chips.filter(function(c) { return c.kind === "text"; }).map(function(c) { return c.text; });
        backend.ask([text].concat(inline).join("\n\n"), files, false, root.noContext);
    }

    function pickSuggestion(n) {
        var t = backend.suggestions[n - 1];
        if (t !== undefined) root.submit(t, []);
    }

    function openRef(chip) {
        if (chip.kind === "text") root.previewText = chip.text;
        else backend.send("attachment.preview", { path: chip.path }, function(r) { root.previewText = r.ok ? String((r.data && r.data.text !== undefined) ? r.data.text : r.data) : r.error; });
    }

    Connections {
        target: backend
        function onConnectedChanged() { if (backend.connected) root.loadModels(); }
    }

    PanelWindow { // qmllint disable uncreatable-type
        id: win
        visible: root.opened
        anchors { top: true; bottom: true; left: true; right: true }
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.namespace: "nixarchy-meetingai"
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand

        // While recording the window only takes input over the card; clicks elsewhere reach the apps below.
        onWidthChanged: root.winW = width
        onHeightChanged: root.winH = height
        mask: Region { item: backend.recording ? card : full }
        Item { id: full; anchors.fill: parent }
        Rectangle { visible: !backend.recording; anchors.fill: parent; color: Commons.Color.menu.scrim }
        MouseArea { enabled: !backend.recording; anchors.fill: parent; onClicked: root.close() }

        BorderSurface {
            id: card
            x: root.geom.x; y: root.geom.y
            width: root.geom.w
            height: root.geom.h
            radius: metrics.radius
            color: root.solid
            borderSpec: Border.localOrSurfaceSpec("popups", "border", Commons.Color.popups.border, Commons.Color.popups.border, Style.normalBorderWidth)
            focus: true
            // Escape closes one thing: a dialog or list (they take it themselves), the preview, Settings, the Last time drawer, then the overlay
            Keys.onEscapePressed: { if (root.previewText !== "") root.previewText = ""; else if (root.library) root.library = false; else if (root.lastOpen) root.lastOpen = false; else root.close(); }
            Keys.onPressed: function(e) {
                if (e.modifiers & Qt.ControlModifier) {   // Ctrl+= / Ctrl+- / Ctrl+0: text size
                    if (e.key === Qt.Key_Equal || e.key === Qt.Key_Plus) { root.bumpScale(10); e.accepted = true; }
                    else if (e.key === Qt.Key_Minus) { root.bumpScale(-10); e.accepted = true; }
                    else if (e.key === Qt.Key_0) { root.bumpScale(0); e.accepted = true; }
                    return;
                }
                // 1/2/3 pick a suggestion only when nothing is being typed
                if (!root.library && composer.empty && e.key >= Qt.Key_1 && e.key <= Qt.Key_3 && !(e.modifiers & ~Qt.KeypadModifier)) {
                    root.pickSuggestion(e.key - Qt.Key_0);
                    e.accepted = true;
                }
            }

            MouseArea { anchors.fill: parent }

            Column {
                x: Style.spacing.lg; y: Style.spacing.lg
                width: parent.width - Style.spacing.lg * 2
                height: parent.height - Style.spacing.lg * 2
                spacing: Style.spacing.md

                Header {
                    id: hdr
                    daemon: backend
                    theme: metrics
                    library: root.library
                    profileName: root.profileName
                    elapsed: root.elapsed
                    width: parent.width
                    onGrabBegan: function(gx, gy) { root.beginGrab(gx, gy); }
                    onGrabMoved: function(gx, gy) { root.dragTo("m", gx, gy); }
                    onGrabEnded: root.endGrab()
                    onResetRequested: { root.live = null; root.resetGeom(); }
                    onSettingsClicked: { root.library = !root.library; lib.tab = "settings"; }
                }
                Rectangle { width: parent.width; height: 1; color: Qt.alpha(Commons.Color.popups.border, 0.4) }

                UiText {   // only for a lost connection; every other failure shows where it happened
                    visible: !backend.connected
                    width: parent.width
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    text: "Error: not connected to the Meeting AI daemon. Retrying."
                    color: Commons.Color.popups.text
                    font.pixelSize: metrics.caption
                }

                SetupCard {
                    id: setup
                    daemon: backend
                    theme: metrics
                    width: parent.width
                    visible: needed && !root.setupDismissed && !root.library
                    onOpenProviders: root.showLibrary("providers")
                    onDismissed: root.setupDismissed = true
                }

                ConsentBanner {
                    daemon: backend
                    theme: metrics
                    width: parent.width
                }

                Library {
                    id: lib
                    settingsHeader: settingsControls
                    onConfirmRequested: function(message, label, yes) { root.ask(message, label, yes); }
                    visible: root.library
                    client: backend
                    theme: metrics
                    width: parent.width
                    height: parent.height - y
                }

                // Conversation: compact transcript (bottom-anchored, collapsible), the
                // pinned NOW panel, ASK NEXT, and the composer.
                Item {
                    id: convo
                    visible: !root.library
                    width: parent.width
                    height: parent.height - y

                    Composer {
                        id: composer
                        theme: metrics
                        anchors.bottom: parent.bottom
                        width: parent.width
                        team: root.team
                        seriesList: backend.series
                        seriesId: root.seriesId
                        seriesLocked: backend.recording
                        seriesError: root.seriesError
                        onSeriesPicked: function(id) { backend.send("session.new", id === "" ? {} : { series: id }, function(r) { root.seriesError = r.ok ? "" : String(r.error); }); }
                        onSeriesCreated: function(name) { backend.send("series.create", { name: name }, function(r) { if (!r.ok) { root.seriesError = String(r.error); return; } backend.refreshSeries(); backend.send("session.new", { series: r.data.id }, function(r2) { root.seriesError = r2.ok ? "" : String(r2.error); }); }); }
                        onSubmit: function(text, chips) { root.submit(text, chips); }
                        onPreview: function(text) { root.previewText = text; }
                        onScreenshot: backend.send("screenshot.capture", { region: true })
                        onQuick: root.submit("What should I say?", [])
                        onPick: function(n) { root.pickSuggestion(n); }
                    }

                    Column {   // ASK NEXT: up to 3 rows, each with a [1]/[2]/[3] badge
                        id: sugg
                        anchors.bottom: composer.top
                        anchors.bottomMargin: Style.spacing.md
                        width: parent.width
                        UiText {
                            visible: backend.suggestions.length > 0
                            textFormat: Text.PlainText
                            text: "Ask next"
                            color: Commons.Color.popups.text
                            font.pixelSize: metrics.caption
                            font.bold: true
                            bottomPadding: Style.spacing.xs
                        }
                        Repeater {
                            model: backend.suggestions
                            Rectangle {
                                id: srow
                                required property string modelData
                                required property int index
                                objectName: "suggestion"
                                activeFocusOnTab: true
                                Accessible.role: Accessible.Button
                                Accessible.name: srow.modelData
                                Accessible.onPressAction: root.pickSuggestion(srow.index + 1)
                                Keys.onReturnPressed: root.pickSuggestion(srow.index + 1)
                                Keys.onEnterPressed: root.pickSuggestion(srow.index + 1)
                                Keys.onSpacePressed: root.pickSuggestion(srow.index + 1)
                                border.width: srow.activeFocus ? Style.normalBorderWidth : 0
                                border.color: Commons.Color.accent
                                width: sugg.width
                                height: Math.max(metrics.ctl, stext.implicitHeight + metrics.padY * 2)
                                radius: metrics.radius
                                color: sma.containsMouse || srow.activeFocus ? Style.controlFill(false, true, Commons.Color.foreground, Commons.Color.accent) : "transparent"
                                Rectangle {
                                    id: badge
                                    x: Style.spacing.md; anchors.verticalCenter: parent.verticalCenter
                                    width: Math.round(Style.space(20) * root.ts); height: width; radius: width / 2
                                    color: "transparent"   // monochrome badge: outline + number, no fill
                                    border.width: Style.normalBorderWidth
                                    border.color: Commons.Color.popups.text
                                    UiText {
                                        anchors.centerIn: parent
                                        textFormat: Text.PlainText
                                        text: String(srow.index + 1)
                                        color: Commons.Color.popups.text
                                        font.pixelSize: metrics.body
                                        font.bold: true
                                    }
                                }
                                UiText {
                                    id: stext
                                    anchors.left: badge.right; anchors.leftMargin: Style.spacing.lg
                                    anchors.right: parent.right; anchors.rightMargin: Style.spacing.md
                                    anchors.verticalCenter: parent.verticalCenter
                                    textFormat: Text.PlainText
                                    wrapMode: Text.Wrap
                                    maximumLineCount: 2
                                    elide: Text.ElideRight
                                    mayCut: true
                                    text: srow.modelData
                                    color: Commons.Color.popups.text
                                    font.pixelSize: metrics.subtitle
                                }
                                MouseArea { id: sma; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.pickSuggestion(srow.index + 1) }
                            }
                        }
                    }

                    // NOW: who asked, the question, status, and the answer larger than the transcript.
                    readonly property real below: composer.height + sugg.height + Style.spacing.md * 2
                    readonly property real avail: height - below - transcript.barHeight
                    NowCard {
                        id: nowPanel
                        daemon: backend
                        theme: metrics
                        status: root.status
                        themedLinks: root.themedLinks
                        // Team mode: only for a typed question, above the items.
                        visible: !root.team || (backend.now.turn >= 0 && !backend.now.automatic)
                        anchors.bottom: sugg.top
                        anchors.bottomMargin: root.tEmpty ? Style.spacing.md + convo.avail - height : Style.spacing.md   // nothing to list: rise to the hint line
                        width: parent.width
                        height: root.panel ? Math.min(nowWant, convo.avail * 0.35) : root.tEmpty ? Math.min(nowWant, convo.avail) : root.tList ? Math.min(nowWant, convo.avail * 0.7) : convo.avail
                    }

                    // Team meeting: replaces NOW. Last time drawer, "Earlier task?" callout, items by kind.
                    ItemsPanel {
                        id: itemsPanel
                        daemon: backend
                        theme: metrics
                        iv: root.iv
                        lastOpen: root.lastOpen
                        lastTime: root.lastTime
                        onLastOpenToggled: root.lastOpen = !root.lastOpen
                        onItemsChanged: root.loadLastTime()
                        onPrepareRequested: root.showLibrary("candidate")
                        visible: root.panel
                        anchors.bottom: nowPanel.visible ? nowPanel.top : sugg.top
                        anchors.bottomMargin: Style.spacing.md
                        width: parent.width
                        height: root.tList ? Math.min(want, convo.avail * (nowPanel.visible ? 0.4 : 0.7)) : Math.max(0, convo.avail - (nowPanel.visible ? nowPanel.height + Style.spacing.md : 0))
                    }

                    Transcript {
                        id: transcript
                        daemon: backend
                        theme: metrics
                        themedLinks: root.themedLinks
                        open: root.transcriptOpen
                        onToggled: root.transcriptOpen = !root.transcriptOpen
                        anchors.top: parent.top
                        width: parent.width
                        height: root.tEmpty ? barHeight : (root.panel ? itemsPanel : nowPanel).y
                    }
                }
            }

            Component {   // top of the Library's Settings tab: captioned cells, one control tall each
                id: settingsControls
                Column {   // the cells, then why a model list is empty
                    id: scol
                    width: parent ? parent.width : 0
                Flow {
                    id: sflow
                    readonly property real cellW: Math.round(Style.space(240) * metrics.ts)
                    width: scol.width
                    spacing: metrics.gap
                    bottomPadding: metrics.gap
                    Cell {
                        theme: metrics; width: sflow.cellW; label: "Chat provider"
                        Select {
                            objectName: "chatProvider"
                            accessibleName: "Chat provider"
                            theme: metrics; anchors.fill: parent
                            value: String(backend.settings.chat_provider || "")
                            options: root.providers.map(function(p) { return { value: p.id, label: p.name || p.id }; })
                            onChanged: function(v) { root.setProvider(false, v); }
                        }
                    }
                    Cell {
                        theme: metrics; width: sflow.cellW; label: backend.lockedLabel("chat_model", "Chat model")
                        Select {   // typing filters the list
                            objectName: "chatModel"
                            accessibleName: "Chat model"
                            searchable: true
                            theme: metrics; anchors.fill: parent
                            enabled: !backend.isLocked("chat_model")
                            value: String(backend.settings.chat_model || "")
                            options: root.models.map(function(m) { return String(m); })
                            triggerLabel: "default"
                            onChanged: function(v) { backend.setSetting("chat_model", v); }
                        }
                    }
                    Cell {
                        theme: metrics; width: sflow.cellW; label: "Other model id"
                        Input {   // a model id the list does not have
                            objectName: "customModel"
                            accessibleName: "Other model id"
                            ts: metrics.ts
                            anchors.fill: parent
                            placeholderText: "or type a model id"
                            onAccepted: { if (text.trim() !== "") backend.setSetting("chat_model", text.trim()); text = ""; }
                        }
                    }
                    Cell {   // automatic answers while a call is recorded
                        theme: metrics; width: sflow.cellW; label: "Live answers provider"
                        Select {
                            objectName: "liveProvider"
                            accessibleName: "Live answers provider"
                            theme: metrics; anchors.fill: parent
                            value: String(backend.settings.live_provider || "")
                            options: root.providers.map(function(p) { return { value: p.id, label: p.name || p.id }; })
                            onChanged: function(v) { root.setProvider(true, v); }
                        }
                    }
                    Cell {
                        theme: metrics; width: sflow.cellW; label: backend.lockedLabel("live_model", "Live answers model")
                        Select {
                            objectName: "liveModel"
                            accessibleName: "Live answers model"
                            searchable: true
                            theme: metrics; anchors.fill: parent
                            enabled: !backend.isLocked("live_model")
                            value: String(backend.settings.live_model || "")
                            options: root.liveModels.map(function(m) { return String(m); })
                            triggerLabel: "default"
                            onChanged: function(v) { backend.setSetting("live_model", v); }
                        }
                    }
                    Cell {
                        theme: metrics; width: sflow.cellW; label: backend.lockedLabel("response_pace", "Pace")
                        Select {
                            objectName: "pace"
                            accessibleName: "Pace"
                            theme: metrics; anchors.fill: parent
                            enabled: !backend.isLocked("response_pace")
                            value: String(backend.settings.response_pace || "Balanced")
                            options: root.paces
                            onChanged: function(v) { backend.setSetting("response_pace", v); }
                        }
                    }
                    Cell {
                        theme: metrics; width: sflow.cellW; label: "No context"
                        RowLayout {
                            anchors.fill: parent
                            Check { objectName: "noContext"; ts: metrics.ts; accessibleName: "No context"; Layout.alignment: Qt.AlignVCenter; checked: root.noContext; onToggled: root.noContext = !root.noContext }
                        }
                    }
                    Cell {   // overlay text size, 80..160 % (Ctrl+= / Ctrl+- / Ctrl+0)
                        theme: metrics; width: sflow.cellW; label: backend.lockedLabel("text_scale", "Text size " + Math.round(backend.textScale * 100) + "%")
                        PanelSlider {
                            objectName: "textSize"
                            Accessible.role: Accessible.Slider; Accessible.name: "Text size"; property string accName: "Text size"
                            anchors.verticalCenter: parent.verticalCenter
                            width: parent.width
                            enabled: !backend.isLocked("text_scale")
                            minimum: 80; maximum: 160; step: 10; integer: true
                            trackHeight: Math.max(4, Math.round(Style.spacing.controlHeight * 0.11 * metrics.ts))
                            knobSize: Math.max(14, Math.round(Style.spacing.controlHeight * 0.38 * metrics.ts))
                            value: Math.round(backend.textScale * 100)
                            trackColor: Qt.alpha(Commons.Color.popups.text, 0.2)
                            fillColor: Commons.Color.popups.text
                            knobColor: Commons.Color.popups.text
                            onMoved: function(v) { var n = Math.round(v / 10) * 10; if (n !== Math.round(backend.textScale * 100)) backend.setSetting("text_scale", n); }
                        }
                    }
                }
                UiText { visible: root.modelsError !== ""; width: parent.width; textFormat: Text.PlainText; wrapMode: Text.Wrap; text: "Error: could not list chat models. " + root.modelsError + root.fix(root.chatProvider); color: Commons.Color.popups.text; font.pixelSize: metrics.caption }
                UiText { visible: root.liveModelsError !== ""; width: parent.width; textFormat: Text.PlainText; wrapMode: Text.Wrap; text: "Error: could not list live answer models. " + root.liveModelsError + root.fix(root.liveProvider); color: Commons.Color.popups.text; font.pixelSize: metrics.caption }
                }
            }

            // Scrollable preview of a chip or a sent reference.
            Rectangle {
                visible: root.previewText !== ""
                anchors.fill: parent
                anchors.margins: Style.space(40)
                color: root.solid
                border.color: Commons.Color.popups.border
                border.width: Style.normalBorderWidth
                radius: metrics.radius
                ScrollView {
                    anchors.fill: parent
                    anchors.margins: Style.spacing.md
                    UiText {
                        width: card.width - Style.space(120)
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        text: root.previewText
                        color: Commons.Color.popups.text
                        font.pixelSize: metrics.body
                    }
                }
                IconButton { ts: root.ts; anchors.top: parent.top; anchors.right: parent.right; text: "Close"; tooltipText: "Close the preview"; onClicked: root.previewText = "" }
            }

            // Resize zones on the edges (6 px) and corners (12 px).
            component Grip: MouseArea {
                property string edges: ""
                onPressed: function(m) { var g = mapToItem(null, m.x, m.y); root.beginGrab(g.x, g.y); }
                onPositionChanged: function(m) { if (pressed) { const g = mapToItem(null, m.x, m.y); root.dragTo(edges, g.x, g.y); } }
                onReleased: root.endGrab()
            }
            Grip { edges: "l"; cursorShape: Qt.SizeHorCursor; x: 0; y: 12; width: 6; height: parent.height - 24 }
            Grip { edges: "r"; cursorShape: Qt.SizeHorCursor; x: parent.width - 6; y: 12; width: 6; height: parent.height - 24 }
            Grip { edges: "t"; cursorShape: Qt.SizeVerCursor; x: 12; y: 0; width: parent.width - 24; height: 6 }
            Grip { edges: "b"; cursorShape: Qt.SizeVerCursor; x: 12; y: parent.height - 6; width: parent.width - 24; height: 6 }
            Grip { edges: "tl"; cursorShape: Qt.SizeFDiagCursor; x: 0; y: 0; width: 12; height: 12 }
            Grip { edges: "br"; cursorShape: Qt.SizeFDiagCursor; x: parent.width - 12; y: parent.height - 12; width: 12; height: 12 }
            Grip { edges: "tr"; cursorShape: Qt.SizeBDiagCursor; x: parent.width - 12; y: 0; width: 12; height: 12 }
            Grip { edges: "bl"; cursorShape: Qt.SizeBDiagCursor; x: 0; y: parent.height - 12; width: 12; height: 12 }

            Confirm { id: confirm; theme: metrics; anchors.fill: parent }
        }
    }
}
