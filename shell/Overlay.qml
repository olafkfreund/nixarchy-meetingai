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
    readonly property string status: daemon.answering ? "Writing" : daemon.now.body !== "" ? "Ready" : "Listening"

    readonly property string pluginId: String((root.manifest && root.manifest.id) || "nixarchy.meetingai")
    // popups.background carries the theme alpha; the overlay must not show what is behind it.
    readonly property color solid: Qt.rgba(Commons.Color.popups.background.r, Commons.Color.popups.background.g, Commons.Color.popups.background.b, 1)
    readonly property string elapsed: {
        var t = Math.max(0, Math.floor((root.now - (daemon.state.since || root.now)) / 1000));
        var p = function(n) { return (n < 10 ? "0" : "") + n; };
        return Math.floor(t / 3600) + ":" + p(Math.floor(t / 60) % 60) + ":" + p(t % 60);
    }
    readonly property real round: Math.max(Style.cornerRadius, 12)   // Hyprland rounding is 6
    // Owner: roomier conversation. Bubble/card padding, gap between rows, reading line height.
    readonly property real padX: Style.space(16)
    readonly property real padY: Style.space(12)
    readonly property real gap: Style.space(14)
    readonly property real leading: 1.3
    readonly property real ts: daemon.textScale   // every font below is Style.font.x * ts
    readonly property real iconPx: Style.font.title * ts
    // Nerd Font Material Design glyphs (nf-md-*), monochrome
    readonly property string iRecord: String.fromCodePoint(0xf044b)
    readonly property string iStop: String.fromCodePoint(0xf04db)
    readonly property string iMic: String.fromCodePoint(0xf036c)
    readonly property string iMicOff: String.fromCodePoint(0xf036d)
    readonly property string iVolume: String.fromCodePoint(0xf057e)
    readonly property string iVolumeOff: String.fromCodePoint(0xf0581)
    readonly property string iCog: String.fromCodePoint(0xf0493)
    readonly property string iDown: String.fromCodePoint(0xf0140)
    readonly property string iRight: String.fromCodePoint(0xf0142)
    readonly property string iCheck: String.fromCodePoint(0xf012c)
    readonly property string iClose: String.fromCodePoint(0xf0156)
    readonly property string iAction: String.fromCodePoint(0xf0139)
    readonly property string iDecision: String.fromCodePoint(0xf023b)
    readonly property string iQuestion: String.fromCodePoint(0xf02d7)
    readonly property var kinds: [{ kind: "action", title: "Actions", glyph: root.iAction }, { kind: "decision", title: "Decisions", glyph: root.iDecision }, { kind: "question", title: "Questions", glyph: root.iQuestion }]
    readonly property bool team: daemon.settings.meeting_mode === "team_meeting"
    readonly property string seriesId: String(daemon.state.series || "")
    property bool lastOpen: false
    property var lastTime: null   // last_time.get reply for the current series
    function loadLastTime() {
        root.lastTime = null;
        if (root.team && root.seriesId !== "") daemon.send("last_time.get", { series: root.seriesId }, function(r) { if (r.ok) root.lastTime = r.data; });
    }
    onTeamChanged: loadLastTime()
    onSeriesIdChanged: loadLastTime()
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
        var s = daemon.settings, r = root.live;
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
        daemon.setSetting("overlay_x", Math.round(g.x)); daemon.setSetting("overlay_y", Math.round(g.y));
        daemon.setSetting("overlay_w", Math.round(g.w)); daemon.setSetting("overlay_h", Math.round(g.h));
        root.live = null;
    }
    function resetGeom() { ["overlay_x", "overlay_y", "overlay_w", "overlay_h"].forEach(function(k) { daemon.setSetting(k, -1); }); }
    readonly property string profileName: {
        var p = daemon.profiles.filter(function(q) { return q.selected; });
        return p.length ? String(p[0].name) : "";
    }
    readonly property var paces: ["Fast", "Balanced", "Relaxed"]

    Client { id: daemon }
    Timer { running: root.opened && daemon.recording; interval: 1000; repeat: true; triggeredOnStart: true; onTriggered: root.now = Date.now() }

    function open(payloadJson) {
        root.opened = true;
        root.loadModels();
    }

    // Hooks for tests/shots.sh.
    function openModelPicker() { root.library = true; lib.tab = "settings"; }
    function showLibrary(tab) { root.library = true; lib.tab = tab; }

    function close() {
        root.opened = false;
        if (root.shell && typeof root.shell.hide === "function") root.shell.hide(root.pluginId);
    }

    function bumpScale(delta) {
        var cur = Math.round(daemon.textScale * 100);
        daemon.setSetting("text_scale", delta === 0 ? 100 : Math.max(80, Math.min(160, cur + delta)));
    }

    function loadModels() {
        daemon.send("providers.list", {}, function(r) { if (r.ok) root.providers = r.data || []; });
        var p = daemon.settings.chat_provider;
        if (p) daemon.send("models.list", { provider: p }, function(r) { if (r.ok) root.models = r.data || []; });
        var lp = daemon.settings.live_provider;
        if (lp) daemon.send("models.list", { provider: lp }, function(r) { if (r.ok) root.liveModels = r.data || []; });
    }

    function submit(text, chips) {
        var files = chips.filter(function(c) { return c.kind === "file"; }).map(function(c) { return c.path; });
        var inline = chips.filter(function(c) { return c.kind === "text"; }).map(function(c) { return c.text; });
        daemon.ask([text].concat(inline).join("\n\n"), files, false, root.noContext);
    }

    function pickSuggestion(n) {
        var t = daemon.suggestions[n - 1];
        if (t !== undefined) root.submit(t, []);
    }

    function openRef(chip) {
        if (chip.kind === "text") root.previewText = chip.text;
        else daemon.send("attachment.preview", { path: chip.path }, function(r) { root.previewText = r.ok ? String((r.data && r.data.text !== undefined) ? r.data.text : r.data) : r.error; });
    }

    Connections {
        target: daemon
        function onConnectedChanged() { if (daemon.connected) root.loadModels(); }
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
        mask: Region { item: daemon.recording ? card : full }
        Item { id: full; anchors.fill: parent }
        Rectangle { visible: !daemon.recording; anchors.fill: parent; color: Commons.Color.menu.scrim }
        MouseArea { enabled: !daemon.recording; anchors.fill: parent; onClicked: root.close() }

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

                // Header: one quiet toolbar of icon buttons.
                Item {
                    width: parent.width
                    height: hdrLeft.height
                    MouseArea {   // drag the header to move the card; double-click resets size and position
                        anchors.fill: parent
                        cursorShape: pressed ? Qt.ClosedHandCursor : Qt.ArrowCursor
                        onPressed: function(m) { var g = mapToItem(null, m.x, m.y); root.beginGrab(g.x, g.y); }
                        onPositionChanged: function(m) { if (pressed) { const g = mapToItem(null, m.x, m.y); root.dragTo("m", g.x, g.y); } }
                        onReleased: root.endGrab()
                        onDoubleClicked: { root.live = null; root.resetGeom(); }
                    }
                    Row {
                        id: hdrLeft
                        spacing: Style.spacing.xs
                        IconButton { ts: root.ts; iconText: daemon.recording ? root.iStop : root.iRecord; active: daemon.recording; tooltipText: daemon.recording ? "Stop recording" : "Start recording"; onClicked: daemon.send(daemon.recording ? "record.stop" : "record.start", {}) }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            visible: daemon.recording
                            textFormat: Text.PlainText
                            text: root.elapsed
                            color: Commons.Color.popups.text
                            font.pixelSize: Style.font.body * root.ts
                            leftPadding: Style.spacing.xs; rightPadding: Style.spacing.xs
                        }
                        // Brightness follows the live level; a struck-out glyph means muted.
                        IconButton { ts: root.ts; visible: daemon.recording; iconText: daemon.state.mic !== false ? root.iMic : root.iMicOff; opacity: daemon.state.mic !== false ? 0.4 + 0.6 * daemon.micLevel : 0.5; tooltipText: daemon.state.mic !== false ? "Mute microphone" : "Unmute microphone"; onClicked: daemon.send("record.toggle_mic", {}) }
                        IconButton { ts: root.ts; visible: daemon.recording; iconText: daemon.state.system !== false ? root.iVolume : root.iVolumeOff; opacity: daemon.state.system !== false ? 0.4 + 0.6 * daemon.systemLevel : 0.5; tooltipText: daemon.state.system !== false ? "Mute system audio" : "Unmute system audio"; onClicked: daemon.send("record.toggle_system", {}) }
                    }
                    Row {
                        anchors.right: parent.right
                        spacing: Style.spacing.md
                        Text {   // which profile's context is in use
                            anchors.verticalCenter: parent.verticalCenter
                            visible: root.profileName !== ""
                            textFormat: Text.PlainText
                            text: root.profileName
                            elide: Text.ElideRight
                            width: Math.min(implicitWidth, Style.space(160))
                            color: Commons.Color.popups.text
                            opacity: 0.7
                            font.pixelSize: Style.font.caption * root.ts
                        }
                        Rectangle {   // segmented control
                            width: seg.width + Style.space(4); height: seg.height + Style.space(4)
                            radius: root.round
                            color: Qt.alpha(Commons.Color.foreground, 0.08)
                            Row {
                                id: seg
                                anchors.centerIn: parent
                                IconButton { ts: root.ts; text: "General"; radius: root.round; active: daemon.settings.meeting_mode !== "interview" && !root.team; tooltipText: "General meeting"; onClicked: daemon.setSetting("meeting_mode", "general") }
                                IconButton { ts: root.ts; text: "Interview"; radius: root.round; active: daemon.settings.meeting_mode === "interview"; tooltipText: "Interview mode"; onClicked: daemon.setSetting("meeting_mode", "interview") }
                                IconButton { ts: root.ts; text: "Team"; radius: root.round; active: root.team; tooltipText: "Team meeting: items and series"; onClicked: daemon.setSetting("meeting_mode", "team_meeting") }
                            }
                        }
                        IconButton { ts: root.ts; iconText: root.iCog; active: root.library; tooltipText: "Settings"; onClicked: { root.library = !root.library; lib.tab = "settings"; } }
                    }
                }
                Rectangle { width: parent.width; height: 1; color: Qt.alpha(Commons.Color.popups.border, 0.4) }

                Text {
                    visible: !daemon.connected || daemon.lastError !== ""
                    width: parent.width
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    text: daemon.connected ? daemon.lastError : "The nixarchy-meetingai daemon is not running."
                    color: Commons.Color.urgent
                    font.pixelSize: Style.font.caption * root.ts
                }

                Library {
                    id: lib
                    settingsHeader: settingsControls
                    visible: root.library
                    client: daemon
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
                        seriesList: daemon.series
                        seriesId: root.seriesId
                        onSeriesPicked: function(id) { daemon.send("session.new", id === "" ? {} : { series: id }); }
                        onSeriesCreated: function(name) { daemon.send("series.create", { name: name }, function(r) { if (!r.ok) return; daemon.refreshSeries(); daemon.send("session.new", { series: r.data.id }); }); }
                        onSubmit: function(text, chips) { root.submit(text, chips); }
                        onPreview: function(text) { root.previewText = text; }
                        onScreenshot: daemon.send("screenshot.capture", { region: true })
                        onQuick: root.submit("What should I say?", [])
                        onPick: function(n) { root.pickSuggestion(n); }
                    }

                    Column {   // ASK NEXT: up to 3 rows, each with a [1]/[2]/[3] badge
                        id: sugg
                        anchors.bottom: composer.top
                        anchors.bottomMargin: Style.spacing.md
                        width: parent.width
                        Text {
                            visible: daemon.suggestions.length > 0
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
                            model: daemon.suggestions
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
                    readonly property real avail: height - below - tbar.height
                    Rectangle {
                        id: nowPanel
                        visible: !root.team
                        anchors.bottom: sugg.top
                        anchors.bottomMargin: Style.spacing.md
                        width: parent.width
                        height: root.transcriptOpen ? Math.min(nowWant, convo.avail * 0.7) : Math.max(nowWant, convo.avail)
                        readonly property real nowWant: nowHead.height + (nowQ.visible ? nowQ.implicitHeight : 0) + answerText.implicitHeight + nowCol.spacing * 2 + root.padY * 2
                        radius: root.round
                        color: Qt.alpha(Commons.Color.accent, 0.12)
                        Rectangle { width: Style.space(4); height: parent.height - root.round; anchors.verticalCenter: parent.verticalCenter; x: Style.spacing.xs; radius: width / 2; color: Commons.Color.accent }
                        Column {
                            id: nowCol
                            x: root.padX; y: root.padY
                            width: parent.width - root.padX * 2
                            spacing: Style.space(8)
                            Row {
                                id: nowHead
                                spacing: Style.spacing.md
                                Text { textFormat: Text.PlainText; text: "Now"; color: Commons.Color.popups.text; font.pixelSize: Style.font.caption * root.ts; font.bold: true; font.capitalization: Font.SmallCaps; font.letterSpacing: 1.5 }
                                Text { textFormat: Text.PlainText; text: root.status; color: Commons.Color.popups.text; font.pixelSize: Style.font.caption * root.ts }
                                Text {
                                    textFormat: Text.PlainText
                                    visible: text !== ""
                                    text: (daemon.now.provider + " " + daemon.now.model).trim()
                                    color: Commons.Color.popups.text
                                    font.pixelSize: Style.font.caption * root.ts
                                }
                            }
                            Text {
                                id: nowQ
                                width: parent.width
                                visible: daemon.now.question !== ""
                                textFormat: Text.PlainText
                                wrapMode: Text.Wrap
                                maximumLineCount: 2
                                elide: Text.ElideRight
                                text: daemon.now.asker + " asked: " + daemon.now.question
                                color: Commons.Color.popups.text
                                font.pixelSize: Style.font.body * root.ts
                                font.bold: true
                            }
                            Flickable {
                                id: nowFlick
                                width: parent.width
                                height: Math.max(0, Math.min(answerText.implicitHeight, nowPanel.height - nowHead.height - (nowQ.visible ? nowQ.implicitHeight : 0) - nowCol.spacing * 2 - root.padY * 2))
                                contentHeight: answerText.implicitHeight
                                clip: true
                                // Follow the tail while streaming; a finished answer is read from its top.
                                Binding { target: nowFlick; property: "contentY"; value: daemon.answering ? Math.max(0, nowFlick.contentHeight - nowFlick.height) : 0; restoreMode: Binding.RestoreNone }
                                Text {
                                    id: answerText
                                    width: parent.width
                                    textFormat: Text.MarkdownText
                                    wrapMode: Text.Wrap
                                    text: root.themedLinks(daemon.now.body)
                                    color: Commons.Color.popups.text
                                    linkColor: Commons.Color.accent
                                    font.pixelSize: Style.font.subtitle * root.ts
                                    lineHeight: root.leading
                                    onLinkActivated: function(link) { Qt.openUrlExternally(link); }
                                }
                            }
                        }
                    }

                    // Team meeting: replaces NOW. Last time drawer, "Earlier task?" callout, items by kind.
                    Rectangle {
                        id: itemsPanel
                        visible: root.team
                        anchors.bottom: sugg.top
                        anchors.bottomMargin: Style.spacing.md
                        width: parent.width
                        height: root.transcriptOpen ? Math.min(want, convo.avail * 0.7) : Math.max(want, convo.avail)
                        readonly property real want: icol.implicitHeight + root.padY * 2
                        radius: root.round
                        color: Qt.alpha(Commons.Color.accent, 0.12)
                        Rectangle { width: Style.space(4); height: parent.height - root.round; anchors.verticalCenter: parent.verticalCenter; x: Style.spacing.xs; radius: width / 2; color: Commons.Color.accent }
                        Flickable {
                            x: root.padX; y: root.padY
                            width: parent.width - root.padX * 2
                            height: parent.height - root.padY * 2
                            contentHeight: icol.implicitHeight
                            clip: true
                            Column {
                                id: icol
                                width: parent.width
                                spacing: Style.space(8)
                                Row {
                                    spacing: Style.spacing.md
                                    Text { anchors.verticalCenter: parent.verticalCenter; textFormat: Text.PlainText; text: "Items"; color: Commons.Color.popups.text; font.pixelSize: Style.font.caption * root.ts; font.bold: true; font.capitalization: Font.SmallCaps; font.letterSpacing: 1.5 }
                                    IconButton { ts: root.ts; visible: root.lastTime !== null && root.lastTime.meeting !== undefined; iconText: root.lastOpen ? root.iDown : root.iRight; text: "Last time"; tooltipText: "Recap and open items of the previous meeting in this series"; onClicked: root.lastOpen = !root.lastOpen }
                                }
                                Column {   // Last time drawer
                                    visible: root.lastOpen && root.lastTime !== null
                                    width: parent.width
                                    spacing: Style.space(4)
                                    Text { width: parent.width; textFormat: Text.PlainText; wrapMode: Text.Wrap; text: root.lastTime ? String(root.lastTime.title || "") + " · " + String(root.lastTime.started_at || "") : ""; color: Commons.Color.popups.text; font.pixelSize: Style.font.caption * root.ts; font.bold: true }
                                    Text { width: parent.width; textFormat: Text.MarkdownText; wrapMode: Text.Wrap; text: root.lastTime ? String(root.lastTime.recap || "") : ""; color: Commons.Color.popups.text; font.pixelSize: Style.font.body * root.ts; lineHeight: root.leading }
                                    Repeater {
                                        model: root.lastTime ? (root.lastTime.items || []) : []
                                        Text { required property var modelData; width: icol.width; textFormat: Text.PlainText; wrapMode: Text.Wrap; text: "- " + modelData.text; color: Commons.Color.popups.text; opacity: 0.8; font.pixelSize: Style.font.body * root.ts }
                                    }
                                }
                                Rectangle {   // Earlier task?
                                    id: callout
                                    visible: daemon.earlierItem !== null
                                    width: parent.width
                                    height: ccol.implicitHeight + root.padY * 2
                                    radius: root.round
                                    color: Qt.alpha(Commons.Color.foreground, 0.10)
                                    border.width: Style.normalBorderWidth
                                    border.color: Commons.Color.popups.border
                                    readonly property var it: daemon.earlierItem ? daemon.earlierItem.item : ({})
                                    function resolve(fields) { if (fields) daemon.updateItem(callout.it.id, fields); daemon.earlierItem = null; }
                                    Column {
                                        id: ccol
                                        x: root.padX; y: root.padY
                                        width: parent.width - root.padX * 2
                                        spacing: Style.space(6)
                                        Text { textFormat: Text.PlainText; text: "Earlier task?"; color: Commons.Color.popups.text; font.pixelSize: Style.font.caption * root.ts; font.bold: true; font.capitalization: Font.SmallCaps; font.letterSpacing: 1.5 }
                                        Text { width: parent.width; textFormat: Text.PlainText; wrapMode: Text.Wrap; text: String(callout.it.text || "") + "  (" + String(callout.it.created_at || "").slice(0, 10) + ")"; color: Commons.Color.popups.text; font.pixelSize: Style.font.body * root.ts; font.bold: true }
                                        Text { width: parent.width; visible: daemon.earlierItem !== null && daemon.earlierItem.quote !== ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; text: daemon.earlierItem ? "\u201c" + daemon.earlierItem.quote + "\u201d" : ""; color: Commons.Color.popups.text; opacity: 0.8; font.italic: true; font.pixelSize: Style.font.body * root.ts }
                                        Row {
                                            spacing: Style.spacing.xs
                                            IconButton { ts: root.ts; text: "Discussed"; bordered: true; tooltipText: "It came up again; link it to this meeting"; onClicked: callout.resolve({ origin_item: daemon.earlierItem.meeting }) }
                                            IconButton { ts: root.ts; iconText: root.iCheck; text: "Done"; bordered: true; tooltipText: "Mark the earlier item done"; onClicked: callout.resolve({ status: "done" }) }
                                            IconButton { ts: root.ts; text: "Not related"; bordered: true; tooltipText: "Dismiss"; onClicked: callout.resolve(null) }
                                        }
                                    }
                                }
                                Text {
                                    visible: daemon.items.length === 0
                                    textFormat: Text.PlainText
                                    text: "Nothing captured yet"
                                    color: Commons.Color.popups.text
                                    opacity: 0.6
                                    font.italic: true
                                    font.pixelSize: Style.font.body * root.ts
                                }
                                Repeater {
                                    model: root.kinds
                                    Column {
                                        id: grp
                                        required property var modelData
                                        readonly property var rows: daemon.items.filter(function(i) { return i.kind === grp.modelData.kind; })
                                        visible: rows.length > 0
                                        width: icol.width
                                        spacing: Style.space(4)
                                        Text { textFormat: Text.PlainText; text: grp.modelData.title; color: Commons.Color.popups.text; opacity: 0.7; font.pixelSize: Style.font.caption * root.ts; font.bold: true }
                                        Repeater {
                                            model: grp.rows
                                            Item {
                                                id: irow
                                                required property var modelData
                                                readonly property bool open: modelData.status === "open"
                                                width: grp.width
                                                height: Math.max(Style.spacing.controlHeight, itext.implicitHeight + Style.space(4))
                                                opacity: open ? 1 : 0.5
                                                Text { id: glyph; anchors.verticalCenter: parent.verticalCenter; textFormat: Text.PlainText; text: grp.modelData.glyph; color: Commons.Color.popups.text; font.pixelSize: root.iconPx }
                                                Row {
                                                    id: acts
                                                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                                                    IconButton { ts: root.ts; iconText: root.iCheck; active: irow.modelData.status === "done"; tooltipText: irow.modelData.status === "done" ? "Reopen" : "Mark done"; onClicked: daemon.updateItem(irow.modelData.id, { status: irow.modelData.status === "done" ? "open" : "done" }) }
                                                    IconButton { ts: root.ts; iconText: root.iClose; active: irow.modelData.status === "dropped"; tooltipText: irow.modelData.status === "dropped" ? "Reopen" : "Drop"; onClicked: daemon.updateItem(irow.modelData.id, { status: irow.modelData.status === "dropped" ? "open" : "dropped" }) }
                                                }
                                                Text {   // owner, due, source line
                                                    id: meta
                                                    anchors.right: acts.left; anchors.rightMargin: Style.spacing.md; anchors.verticalCenter: parent.verticalCenter
                                                    textFormat: Text.PlainText
                                                    text: [irow.modelData.owner, irow.modelData.due, irow.modelData.source_line ? "line " + irow.modelData.source_line : ""].filter(function(x) { return x; }).join(" · ")
                                                    color: Commons.Color.popups.text
                                                    opacity: 0.7
                                                    font.pixelSize: Style.font.caption * root.ts
                                                }
                                                TextInput {   // inline edit; a draft is shown italic
                                                    id: itext
                                                    anchors.left: glyph.right; anchors.leftMargin: Style.spacing.md
                                                    anchors.right: meta.left; anchors.rightMargin: Style.spacing.md
                                                    anchors.verticalCenter: parent.verticalCenter
                                                    wrapMode: TextInput.Wrap
                                                    text: irow.modelData.text
                                                    color: Commons.Color.popups.text
                                                    selectionColor: Commons.Color.accent
                                                    font.pixelSize: Style.font.body * root.ts
                                                    font.italic: irow.modelData.draft === true || irow.modelData.draft === 1
                                                    font.strikeout: irow.modelData.status !== "open"
                                                    onEditingFinished: { if (text.trim() !== "" && text !== irow.modelData.text) daemon.updateItem(irow.modelData.id, { text: text.trim() }); }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    Row {   // transcript toggle
                        id: tbar
                        anchors.top: parent.top
                        IconButton { ts: root.ts; iconText: root.transcriptOpen ? root.iDown : root.iRight; text: "Transcript"; tooltipText: root.transcriptOpen ? "Collapse the transcript" : "Expand the transcript"; onClicked: root.transcriptOpen = !root.transcriptOpen }
                    }

                    // Transcript and archived answers in one virtualized list (capped at 500 rows by Client).
                    Item {
                        visible: root.transcriptOpen
                        anchors { top: tbar.bottom; left: parent.left; right: parent.right; bottom: root.team ? itemsPanel.top : nowPanel.top; bottomMargin: Style.spacing.md }
                        ListView {
                            id: tlist
                            anchors { bottom: parent.bottom; left: parent.left; right: parent.right }
                            height: Math.min(contentHeight, parent.height)
                            clip: true
                            spacing: root.gap
                            model: daemon.transcript
                            property bool follow: true
                            // Exact bottom; positionViewAtEnd works from estimated delegate heights.
                            function toEnd() { if (follow) contentY = originY + Math.max(0, contentHeight - height); }
                            onContentHeightChanged: Qt.callLater(toEnd)
                            onHeightChanged: Qt.callLater(toEnd)   // the NOW panel grows and shrinks the list
                            onMovementEnded: follow = atYEnd
                            delegate: Item {
                                id: row
                                required property string who
                                required property string speaker
                                required property string body
                                required property bool partial
                                readonly property bool answer: who === "answer"
                                readonly property bool mine: who === "mic" || who === "asked"
                                readonly property bool bubble: who === "mic" || who === "system" || who === "asked"
                                readonly property string label: who === "asked" ? "You · Asked" : who === "mic" ? "You · Spoke" : (speaker !== "" ? speaker : "Them")
                                width: ListView.view.width
                                height: answer ? card2.height : bubble ? bub.height : err.implicitHeight
                                // Markdown and line height settle after layout; keep the end pinned while following.
                                onHeightChanged: Qt.callLater(tlist.toEnd)
                                Rectangle {   // flat, borderless bubble
                                    id: bub
                                    visible: row.bubble
                                    x: row.mine ? row.width - width : 0
                                    width: Math.min(row.width * 0.8, Math.max(btxt.implicitWidth, blabel.implicitWidth) + root.padX * 2)
                                    height: bcol.implicitHeight + root.padY * 2
                                    radius: root.round
                                    color: row.mine ? Qt.alpha(Commons.Color.accent, 0.22) : Qt.alpha(Commons.Color.foreground, 0.10)
                                    Column {
                                        id: bcol
                                        x: root.padX; y: root.padY
                                        width: parent.width - root.padX * 2
                                        spacing: Style.space(4)
                                        Text { id: blabel; textFormat: Text.PlainText; text: row.label; color: Commons.Color.popups.text; font.pixelSize: Style.font.caption * root.ts; font.bold: true }
                                        Text {
                                            id: btxt
                                            width: parent.width
                                            textFormat: Text.PlainText
                                            wrapMode: Text.Wrap
                                            text: row.body
                                            color: Commons.Color.popups.text
                                            font.italic: row.partial
                                            font.pixelSize: Style.font.body * root.ts
                                            lineHeight: root.leading
                                        }
                                    }
                                }
                                Rectangle {   // an earlier answer: accent stripe, label, markdown
                                    id: card2
                                    visible: row.answer
                                    width: parent.width
                                    height: ansCol.implicitHeight + root.padY * 2
                                    radius: root.round
                                    color: Qt.alpha(Commons.Color.accent, 0.12)
                                    Rectangle { width: Style.space(4); height: parent.height - root.round; anchors.verticalCenter: parent.verticalCenter; x: Style.spacing.xs; radius: width / 2; color: Commons.Color.accent }
                                    Column {
                                        id: ansCol
                                        x: root.padX; y: root.padY
                                        width: parent.width - root.padX * 2
                                        spacing: Style.space(6)
                                        Text { textFormat: Text.PlainText; text: "Suggested answer"; color: Commons.Color.popups.text; font.pixelSize: Style.font.caption * root.ts; font.bold: true; font.capitalization: Font.SmallCaps; font.letterSpacing: 1.5 }
                                        Text {
                                            width: parent.width
                                            textFormat: Text.MarkdownText
                                            wrapMode: Text.Wrap
                                            text: root.themedLinks(row.body)
                                            color: Commons.Color.popups.text
                                            linkColor: Commons.Color.accent
                                            font.pixelSize: Style.font.body * root.ts
                                            lineHeight: root.leading
                                            onLinkActivated: function(link) { Qt.openUrlExternally(link); }
                                        }
                                    }
                                }
                                Text {
                                    id: err
                                    visible: !row.answer && !row.bubble
                                    width: parent.width
                                    textFormat: Text.PlainText
                                    wrapMode: Text.Wrap
                                    text: row.body
                                    color: row.who === "error" ? Commons.Color.urgent : Commons.Color.popups.text
                                    opacity: row.who === "notice" ? 0.6 : 1
                                    font.italic: row.who === "notice"
                                    font.pixelSize: (row.who === "notice" ? Style.font.caption : Style.font.bodySmall) * root.ts
                                }
                            }
                        }
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
                        value: String(daemon.settings.chat_provider || "")
                        options: root.providers.map(function(p) { return { value: p.id, label: p.name || p.id }; })
                        onChanged: function(v) { daemon.setSetting("chat_provider", v); daemon.setSetting("chat_model", ""); root.loadModels(); }
                    }
                    SearchableDropdown {   // typing filters the list
                        id: modelBox
                        label: "Chat model"
                        value: String(daemon.settings.chat_model || "")
                        options: root.models.map(function(m) { return String(m); })
                        triggerLabel: "default"
                        onChanged: function(v) { daemon.setSetting("chat_model", v); }
                    }
                    Item {
                        width: Style.space(200); height: modelBox.height
                        TextField {   // a model id the list does not have
                            id: customModel
                            anchors.bottom: parent.bottom
                            width: parent.width
                            placeholderText: "or type a model id"
                            onAccepted: { if (text.trim() !== "") daemon.setSetting("chat_model", text.trim()); text = ""; }
                        }
                    }
                    Dropdown {   // automatic answers while a call is recorded
                        label: "Live answers provider"
                        value: String(daemon.settings.live_provider || "")
                        options: root.providers.map(function(p) { return { value: p.id, label: p.name || p.id }; })
                        onChanged: function(v) { daemon.setSetting("live_provider", v); daemon.setSetting("live_model", ""); root.loadModels(); }
                    }
                    SearchableDropdown {
                        label: "Live answers model"
                        value: String(daemon.settings.live_model || "")
                        options: root.liveModels.map(function(m) { return String(m); })
                        triggerLabel: "default"
                        onChanged: function(v) { daemon.setSetting("live_model", v); }
                    }
                    Dropdown {
                        label: "Pace"
                        value: String(daemon.settings.response_pace || "Balanced")
                        options: root.paces
                        onChanged: function(v) { daemon.setSetting("response_pace", v); }
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
                            text: "Text size " + Math.round(daemon.textScale * 100) + "%"
                            color: Commons.Color.popups.text
                            font.pixelSize: Style.font.caption * root.ts
                        }
                        PanelSlider {
                            anchors.bottom: parent.bottom
                            width: parent.width
                            minimum: 80; maximum: 160; step: 10; integer: true
                            value: Math.round(daemon.textScale * 100)
                            trackColor: Qt.alpha(Commons.Color.popups.text, 0.2)
                            fillColor: Commons.Color.popups.text
                            knobColor: Commons.Color.popups.text
                            onMoved: function(v) { var n = Math.round(v / 10) * 10; if (n !== Math.round(daemon.textScale * 100)) daemon.setSetting("text_scale", n); }
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
