pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects (Style.spacing, Color.popups) the linter cannot type
import QtQuick
import QtQuick.Controls
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
    readonly property string status: backend.answering ? "Writing" : backend.now.body !== "" ? "Ready" : "Listening"

    readonly property string pluginId: String((root.manifest && root.manifest.id) || "nixarchy.meetingai")
    // popups.background carries the theme alpha; the overlay must not show what is behind it.
    readonly property color solid: Qt.rgba(Commons.Color.popups.background.r, Commons.Color.popups.background.g, Commons.Color.popups.background.b, 1)
    readonly property string elapsed: {
        var t = Math.max(0, Math.floor((root.now - (backend.state.since || root.now)) / 1000));
        var p = function(n) { return (n < 10 ? "0" : "") + n; };
        return Math.floor(t / 3600) + ":" + p(Math.floor(t / 60) % 60) + ":" + p(t % 60);
    }
    readonly property real round: Math.max(Style.cornerRadius, 12)   // Hyprland rounding is 6
    // Owner: roomier conversation. Bubble/card padding, gap between rows, reading line height.
    readonly property real padX: Style.space(16)
    readonly property real padY: Style.space(12)
    readonly property real gap: Style.space(14)
    readonly property real leading: 1.3
    readonly property real ts: backend.textScale   // every font below is Style.font.x * ts
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
        root.loadModels();
    }

    // Hooks for tests/shots.sh.
    function openModelPicker() { root.library = true; lib.tab = "settings"; }
    function showLibrary(tab) { root.library = true; lib.tab = tab; }
    function recordHook(consent) { backend.startRecording(consent); }

    function close() {
        root.opened = false;
        if (root.shell && typeof root.shell.hide === "function") root.shell.hide(root.pluginId);
    }

    function bumpScale(delta) {
        var cur = Math.round(backend.textScale * 100);
        backend.setSetting("text_scale", delta === 0 ? 100 : Math.max(80, Math.min(160, cur + delta)));
    }

    function loadModels() {
        backend.send("providers.list", {}, function(r) { if (r.ok) root.providers = r.data || []; });
        var p = backend.settings.chat_provider;
        if (p) backend.send("models.list", { provider: p }, function(r) { if (r.ok) root.models = r.data || []; });
        var lp = backend.settings.live_provider;
        if (lp) backend.send("models.list", { provider: lp }, function(r) { if (r.ok) root.liveModels = r.data || []; });
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
        function onSettingsChanged() { if (root.models.length === 0 || root.liveModels.length === 0) root.loadModels(); }
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

        Rectangle {
            id: card
            x: root.geom.x; y: root.geom.y
            width: root.geom.w
            height: root.geom.h
            radius: root.round
            color: root.solid
            border.color: Commons.Color.popups.border
            border.width: Style.normalBorderWidth
            focus: true
            Keys.onEscapePressed: root.close()
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
                    daemon: backend
                    ts: root.ts
                    round: root.round
                    team: root.team
                    iv: root.iv
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

                Text {
                    visible: !backend.connected || backend.lastError !== ""
                    width: parent.width
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    text: backend.connected ? backend.lastError : "The nixarchy-meetingai backend is not running."
                    color: Commons.Color.urgent
                    font.pixelSize: Style.font.caption * root.ts
                }

                ConsentBanner {
                    daemon: backend
                    ts: root.ts
                    round: root.round
                    padX: root.padX
                    padY: root.padY
                    width: parent.width
                }

                Library {
                    id: lib
                    settingsHeader: settingsControls
                    visible: root.library
                    client: backend
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
                        ts: root.ts
                        anchors.bottom: parent.bottom
                        width: parent.width
                        team: root.team
                        seriesList: backend.series
                        seriesId: root.seriesId
                        seriesLocked: backend.recording
                        onSeriesPicked: function(id) { backend.send("session.new", id === "" ? {} : { series: id }, function(r) { if (!r.ok) backend.lastError = r.error; }); }
                        onSeriesCreated: function(name) { backend.send("series.create", { name: name }, function(r) { if (!r.ok) { backend.lastError = r.error; return; } backend.refreshSeries(); backend.send("session.new", { series: r.data.id }, function(r2) { if (!r2.ok) backend.lastError = r2.error; }); }); }
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
                        Text {
                            visible: backend.suggestions.length > 0
                            textFormat: Text.PlainText
                            text: "Ask next"
                            color: Commons.Color.popups.text
                            font.pixelSize: Style.font.caption * root.ts
                            font.bold: true
                            font.capitalization: Font.SmallCaps
                            font.letterSpacing: 1.5
                            bottomPadding: Style.spacing.xs
                        }
                        Repeater {
                            model: backend.suggestions
                            Rectangle {
                                id: srow
                                required property string modelData
                                required property int index
                                width: sugg.width
                                height: Math.max(Style.spacing.controlHeight, stext.implicitHeight + root.padY * 2)
                                radius: root.round
                                color: sma.containsMouse ? Style.controlFill(false, true, Commons.Color.foreground, Commons.Color.accent) : "transparent"
                                Rectangle {
                                    id: badge
                                    x: Style.spacing.md; anchors.verticalCenter: parent.verticalCenter
                                    width: Math.round(Style.space(20) * root.ts); height: width; radius: width / 2
                                    color: "transparent"   // monochrome badge: outline + number, no fill
                                    border.width: Style.normalBorderWidth
                                    border.color: Commons.Color.popups.text
                                    Text {
                                        anchors.centerIn: parent
                                        textFormat: Text.PlainText
                                        text: String(srow.index + 1)
                                        color: Commons.Color.popups.text
                                        font.pixelSize: Style.font.body * root.ts
                                        font.bold: true
                                    }
                                }
                                Text {
                                    id: stext
                                    anchors.left: badge.right; anchors.leftMargin: Style.spacing.lg
                                    anchors.right: parent.right; anchors.rightMargin: Style.spacing.md
                                    anchors.verticalCenter: parent.verticalCenter
                                    textFormat: Text.PlainText
                                    wrapMode: Text.Wrap
                                    maximumLineCount: 2
                                    elide: Text.ElideRight
                                    text: srow.modelData
                                    color: Commons.Color.popups.text
                                    font.pixelSize: Style.font.subtitle * root.ts
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
                        ts: root.ts
                        round: root.round
                        padX: root.padX
                        padY: root.padY
                        leading: root.leading
                        status: root.status
                        themedLinks: root.themedLinks
                        // Team mode: only for a typed question, above the items.
                        visible: !root.team || (backend.now.turn >= 0 && !backend.now.automatic)
                        anchors.bottom: sugg.top
                        anchors.bottomMargin: Style.spacing.md
                        width: parent.width
                        height: root.panel ? Math.min(nowWant, convo.avail * 0.35) : root.transcriptOpen ? Math.min(nowWant, convo.avail * 0.7) : Math.max(nowWant, convo.avail)
                    }

                    // Team meeting: replaces NOW. Last time drawer, "Earlier task?" callout, items by kind.
                    ItemsPanel {
                        id: itemsPanel
                        daemon: backend
                        ts: root.ts
                        round: root.round
                        padX: root.padX
                        padY: root.padY
                        leading: root.leading
                        iv: root.iv
                        lastOpen: root.lastOpen
                        lastTime: root.lastTime
                        onLastOpenToggled: root.lastOpen = !root.lastOpen
                        onItemsChanged: root.loadLastTime()
                        visible: root.panel
                        anchors.bottom: nowPanel.visible ? nowPanel.top : sugg.top
                        anchors.bottomMargin: Style.spacing.md
                        width: parent.width
                        height: root.transcriptOpen ? Math.min(want, convo.avail * (nowPanel.visible ? 0.4 : 0.7)) : Math.max(want, convo.avail - (nowPanel.visible ? nowPanel.height + Style.spacing.md : 0))
                    }

                    Transcript {
                        id: transcript
                        daemon: backend
                        ts: root.ts
                        round: root.round
                        padX: root.padX
                        padY: root.padY
                        gap: root.gap
                        leading: root.leading
                        themedLinks: root.themedLinks
                        open: root.transcriptOpen
                        onToggled: root.transcriptOpen = !root.transcriptOpen
                        anchors.top: parent.top
                        width: parent.width
                        height: (root.panel ? itemsPanel : nowPanel).y
                    }
                }
            }

            Component {   // top of the Library's Settings tab
                id: settingsControls
                Flow {
                    width: parent ? parent.width : 0
                    spacing: Style.spacing.md
                    bottomPadding: Style.spacing.md
                    Dropdown {
                        label: "Chat provider"
                        value: String(backend.settings.chat_provider || "")
                        options: root.providers.map(function(p) { return { value: p.id, label: p.name || p.id }; })
                        onChanged: function(v) { backend.setSetting("chat_provider", v); backend.setSetting("chat_model", ""); root.loadModels(); }
                    }
                    SearchableDropdown {   // typing filters the list
                        id: modelBox
                        label: "Chat model"
                        value: String(backend.settings.chat_model || "")
                        options: root.models.map(function(m) { return String(m); })
                        triggerLabel: "default"
                        onChanged: function(v) { backend.setSetting("chat_model", v); }
                    }
                    Item {
                        width: Style.space(200); height: modelBox.height
                        TextField {   // a model id the list does not have
                            id: customModel
                            anchors.bottom: parent.bottom
                            width: parent.width
                            placeholderText: "or type a model id"
                            onAccepted: { if (text.trim() !== "") backend.setSetting("chat_model", text.trim()); text = ""; }
                        }
                    }
                    Dropdown {   // automatic answers while a call is recorded
                        label: "Live answers provider"
                        value: String(backend.settings.live_provider || "")
                        options: root.providers.map(function(p) { return { value: p.id, label: p.name || p.id }; })
                        onChanged: function(v) { backend.setSetting("live_provider", v); backend.setSetting("live_model", ""); root.loadModels(); }
                    }
                    SearchableDropdown {
                        label: "Live answers model"
                        value: String(backend.settings.live_model || "")
                        options: root.liveModels.map(function(m) { return String(m); })
                        triggerLabel: "default"
                        onChanged: function(v) { backend.setSetting("live_model", v); }
                    }
                    Dropdown {
                        label: "Pace"
                        value: String(backend.settings.response_pace || "Balanced")
                        options: root.paces
                        onChanged: function(v) { backend.setSetting("response_pace", v); }
                    }
                    Item {
                        width: ncRow.width; height: modelBox.height
                        Row {
                            id: ncRow
                            anchors.bottom: parent.bottom
                            height: Style.spacing.controlHeight
                            spacing: Style.spacing.xs
                            ToggleSwitch { id: noContext; anchors.verticalCenter: parent.verticalCenter; checked: root.noContext; onToggled: root.noContext = !root.noContext }
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                textFormat: Text.PlainText
                                text: "No context"
                                color: Commons.Color.popups.text
                                font.pixelSize: Style.font.caption * root.ts
                            }
                        }
                    }
                    Item {   // overlay text size, 80..160 % (Ctrl+= / Ctrl+- / Ctrl+0)
                        width: Style.space(260); height: modelBox.height
                        Text {
                            textFormat: Text.PlainText
                            text: "Text size " + Math.round(backend.textScale * 100) + "%"
                            color: Commons.Color.popups.text
                            font.pixelSize: Style.font.caption * root.ts
                        }
                        PanelSlider {
                            anchors.bottom: parent.bottom
                            width: parent.width
                            minimum: 80; maximum: 160; step: 10; integer: true
                            value: Math.round(backend.textScale * 100)
                            trackColor: Qt.alpha(Commons.Color.popups.text, 0.2)
                            fillColor: Commons.Color.popups.text
                            knobColor: Commons.Color.popups.text
                            onMoved: function(v) { var n = Math.round(v / 10) * 10; if (n !== Math.round(backend.textScale * 100)) backend.setSetting("text_scale", n); }
                        }
                    }
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
                radius: root.round
                ScrollView {
                    anchors.fill: parent
                    anchors.margins: Style.spacing.md
                    Text {
                        width: card.width - Style.space(120)
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        text: root.previewText
                        color: Commons.Color.popups.text
                        font.pixelSize: Style.font.body * root.ts
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
        }
    }
}
