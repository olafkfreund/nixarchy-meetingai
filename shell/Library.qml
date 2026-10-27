pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects (Style.spacing, Color.popups) the linter cannot type
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// Restore / Profiles / Prompts / Providers / Settings. One search field asks
// the daemon (settings.search) and also filters the lists on the other tabs.
Item {
    id: root

    required property var client
    readonly property real ts: client.textScale
    property string tab: "restore"
    property string query: ""
    property var sessions: []
    readonly property var profiles: client.profiles
    property string selName: ""   // the profile being edited
    property var providers: []
    readonly property var labelOverrides: ({ "mcp_allow_record": "Let AI agents start recording" })   // the daemon derives labels from keys
    property var found: []        // settings.search results: { key, label, value }
    property string promptMode: "general"
    property string notes: ""
    property string modelsText: ""
    property Component settingsHeader: null   // controls shown above the Settings list

    readonly property bool iv: client.settings.meeting_mode === "interviewer"
    readonly property var tabs: root.iv ? ["restore", "profiles", "candidate", "prompts", "providers", "settings"] : ["restore", "profiles", "prompts", "providers", "settings"]
    readonly property var cand: client.cand   // candidate.get of the shown candidate
    readonly property bool candLinked: root.cand !== null && !!root.cand.candidate.meeting_id   // already interviewed: read-only
    property string genNote: ""
    property bool confirmDelete: false
    readonly property var promptKinds: ["answer", "live_answer", "follow_up", "recap"]
    readonly property var kindHelp: ({
        answer: "Used for questions you type: how the model answers you.",
        live_answer: "Used for automatic answers during a call: one sentence to say, then short talking points.",
        follow_up: "Used to suggest the next questions to ask (Ask next).",
        recap: "Used to write the meeting notes when a recording stops."
    })
    readonly property var cur: {
        var m = root.profiles.filter(function(p) { return p.name === root.selName; });
        if (m.length) return m[0];
        m = root.profiles.filter(function(p) { return p.selected; });
        return m.length ? m[0] : (root.profiles.length ? root.profiles[0] : null);
    }
    onCurChanged: { aboutText.text = cur ? String(cur.personal_context || "") : ""; briefText.text = cur ? String(cur.meeting_context || "") : ""; nameField.text = cur ? cur.name : ""; }
    function reload() { root.client.refreshProfiles(); }
    function newProfile() {
        var names = root.profiles.map(function(p) { return p.name; }), n = "New profile", i = 1;
        while (names.indexOf(n) >= 0) n = "New profile " + (++i);
        root.selName = n;
        root.client.send("profiles.save", { name: n, personal_context: "", meeting_context: "" }, function() { root.client.send("profiles.select", { name: n }, root.reload); });
    }
    function saveProfile() {
        if (!cur) return;
        var a = aboutText.text, b = briefText.text;
        root.client.send("profiles.save", { name: cur.name, personal_context: a, meeting_context: b }, root.reload);
        if (cur.selected) { root.client.setSetting("personal_context", a); root.client.setSetting("meeting_context", b); }
    }
    property string promptKind: "answer"

    function match(s) { return root.query === "" || String(s).toLowerCase().indexOf(root.query.toLowerCase()) >= 0; }

    function load() {
        root.client.send("session.list", {}, function(r) { if (r.ok) root.sessions = r.data || []; });
        root.reload();
        root.client.send("providers.list", {}, function(r) { if (r.ok) root.providers = r.data || []; });
        root.search();
        root.loadPrompt();
        root.client.refreshCandidates();
        root.confirmDelete = false;
    }
    function fail(r) { if (!r.ok) root.client.lastError = String(r.error); }
    // Any candidate can be opened to view or delete; only one without a meeting is chosen for a new interview.
    function pickCandidate(id, unlinked) {
        root.client.viewCandidate(id);
        if (unlinked) root.client.send("session.new", { candidate: id }, root.fail);
    }
    function moveQuestion(i, d) {
        var ids = root.cand.questions.map(function(q) { return q.id; }), t = ids[i + d];
        ids[i + d] = ids[i]; ids[i] = t;
        root.client.send("questions.reorder", { candidate: root.cand.candidate.id, ids: ids }, function(r) { root.fail(r); root.client.refreshCand(); });
    }

    function search() {
        root.client.send("settings.search", { query: root.query }, function(r) { if (r.ok) root.found = (r.data || []).filter(function(f) { return String(f.key).indexOf("overlay_") !== 0; }); });
    }

    function loadPrompt() {
        root.client.send("prompts.get", { mode: root.promptMode, kind: root.promptKind }, function(r) {
            promptText.text = r.ok ? String((r.data && r.data.text !== undefined) ? r.data.text : r.data) : "";
        });
    }

    onTabChanged: root.load()
    onPromptModeChanged: root.loadPrompt()
    onPromptKindChanged: root.loadPrompt()
    Component.onCompleted: root.load()

    Connections {
        target: root.client
        function onNotesUpdated() { root.load(); }
    }

    Column {
        anchors.fill: parent
        spacing: Style.spacing.md

        Row {
            spacing: Style.spacing.xs
            Repeater {
                model: root.tabs
                IconButton { ts: root.ts;
                    required property string modelData
                    text: modelData.charAt(0).toUpperCase() + modelData.slice(1)
                    active: root.tab === modelData
                    onClicked: root.tab = modelData
                }
            }
        }

        PanelSeparator {}

        TextField {
            width: parent.width
            placeholderText: "Search settings, chats, profiles"
            onTextChanged: { root.query = text; root.search(); }
        }

        // Restore: chats and meetings with their notes.
        ListView {
            visible: root.tab === "restore"
            width: parent.width; height: parent.height - y - notesBox.height
            clip: true
            model: root.sessions.filter(function(s) { return root.match(s.title); })
            delegate: Row {
                required property var modelData
                width: ListView.view.width
                spacing: Style.spacing.xs
                Text {
                    width: parent.width - Style.space(200)
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    text: (parent.modelData.kind === "meeting" ? "Meeting · " : "Chat · ") + parent.modelData.title
                    color: Commons.Color.popups.text
                    font.pixelSize: Style.font.body * root.ts
                }
                IconButton { ts: root.ts; text: "Restore"; onClicked: root.client.send("session.restore", { id: parent.modelData.id }) }
                IconButton { ts: root.ts;
                    text: "Notes"
                    visible: parent.modelData.kind === "meeting"
                    onClicked: root.client.send("notes.get", { meeting: parent.modelData.id }, function(r) { root.notes = r.ok ? String((r.data && r.data.text !== undefined) ? r.data.text : r.data) : r.error; })
                }
                IconButton { ts: root.ts; text: "Delete"; onClicked: { root.client.send("session.delete", { id: parent.modelData.id }); root.load(); } }
            }
        }
        Text {
            id: notesBox
            visible: root.tab === "restore" && root.notes !== ""
            width: parent.width
            height: visible ? Style.space(160) : 0
            textFormat: Text.MarkdownText
            wrapMode: Text.Wrap
            clip: true
            text: root.notes
            color: Commons.Color.popups.text
            font.pixelSize: Style.font.body * root.ts
        }

        component Field: TextArea {
            id: fld
            wrapMode: TextEdit.Wrap
            color: Commons.Color.popups.text
            font.pixelSize: Style.font.body * root.ts
            background: BorderSurface {
                color: Style.controlFill(fld.activeFocus, fld.hovered, Commons.Color.foreground, Commons.Color.accent)
                borderSpec: Border.controlSpec(fld.activeFocus ? "focus" : (fld.hovered ? "hover-cursor" : "normal"), Commons.Color.foreground, Commons.Color.accent)
                radius: Style.cornerRadius
            }
        }
        component Hint: Text {
            width: parent.width
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: Commons.Color.popups.text
            opacity: 0.7
            font.pixelSize: Style.font.caption * root.ts
        }

        Row {
            visible: root.tab === "profiles"
            width: parent.width; height: parent.height - y
            spacing: Style.spacing.md
            Column {   // list + New
                width: Math.max(Style.space(150), parent.width * 0.28); height: parent.height
                spacing: Style.spacing.xs
                ListView {
                    width: parent.width; height: Math.min(contentHeight, parent.height - newBtn.height - parent.spacing)   // New sits right under the list
                    clip: true
                    model: root.profiles.filter(function(p) { return root.match(p.name); })
                    delegate: IconButton {
                        required property var modelData
                        ts: root.ts
                        width: ListView.view.width
                        text: modelData.name + (modelData.selected ? "  (in use)" : "")
                        active: root.cur && root.cur.name === modelData.name
                        onClicked: { root.selName = modelData.name; root.client.send("profiles.select", { name: modelData.name }, root.reload); }
                    }
                }
                IconButton { id: newBtn; ts: root.ts; text: "New"; onClicked: root.newProfile() }
            }
            ScrollView {
                id: editor
                visible: root.cur !== null
                width: parent.width - parent.children[0].width - parent.spacing; height: parent.height
                contentWidth: availableWidth
                clip: true
                Column {
                    width: editor.availableWidth
                    spacing: Style.spacing.xs
                    Row {
                        spacing: Style.spacing.xs
                        TextField { id: nameField; width: Style.space(200) }
                        IconButton { ts: root.ts; text: "Rename"; enabled: root.cur !== null && nameField.text.trim() !== "" && nameField.text !== root.cur.name; opacity: enabled ? 1 : 0.5
                            onClicked: { var n = nameField.text.trim(), o = root.cur.name; root.selName = n; root.client.send("profiles.rename", { name: o, new_name: n }, root.reload); } }
                        IconButton { ts: root.ts; text: "Delete"; onClicked: { root.client.send("profiles.delete", { name: root.cur.name }, root.reload); root.selName = ""; } }
                    }
                    Text { textFormat: Text.PlainText; text: "About me"; color: Commons.Color.popups.text; font.bold: true; font.pixelSize: Style.font.body * root.ts }
                    Hint { text: "Your role and experience; used in every answer." }
                    Field { id: aboutText; width: parent.width; height: Style.space(64) }
                    Text { textFormat: Text.PlainText; text: "Meeting brief"; color: Commons.Color.popups.text; font.bold: true; font.pixelSize: Style.font.body * root.ts }
                    Hint { text: "What this meeting is about (role, company, agenda)." }
                    Field { id: briefText; width: parent.width; height: Style.space(64) }
                    IconButton { ts: root.ts; text: "Save"; onClicked: root.saveProfile() }
                    Text { textFormat: Text.PlainText; text: "Documents"; color: Commons.Color.popups.text; font.bold: true; font.pixelSize: Style.font.body * root.ts; topPadding: Style.spacing.xs }
                    Hint { text: "CV, job description, runbooks (PDF, text, markdown). Newest first in the prompt." }
                    Repeater {
                        model: root.cur ? (root.cur.documents || []) : []
                        Row {
                            id: drow
                            required property var modelData
                            spacing: Style.spacing.xs
                            Text {
                                width: editor.availableWidth - Style.space(110)
                                anchors.verticalCenter: parent.verticalCenter
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                                text: drow.modelData.name + "  ·  " + drow.modelData.chars + " chars"
                                color: Commons.Color.popups.text
                                font.pixelSize: Style.font.body * root.ts
                            }
                            IconButton { ts: root.ts; text: "Remove"; onClicked: root.client.send("profiles.detach", { name: root.cur.name, id: drow.modelData.id }, root.reload) }
                        }
                    }
                    IconButton { ts: root.ts; text: "Attach"; onClicked: attachDialog.open() }
                }
            }
            FileDialog {
                id: attachDialog
                nameFilters: ["Documents (*.pdf *.txt *.md)"]
                onAccepted: {
                    var p = decodeURIComponent(String(selectedFile).replace(/^file:\/\//, ""));
                    root.client.send("profiles.attach", { name: root.cur.name, path: p }, function(r) { if (!r.ok) root.client.lastError = String(r.error); root.reload(); });
                }
            }
        }

        // Candidate (interviewer mode): documents, question bank, local-only, deletion.
        ScrollView {
            id: candView
            visible: root.tab === "candidate"
            width: parent.width; height: parent.height - y
            contentWidth: availableWidth
            clip: true
            Column {
                width: candView.availableWidth
                spacing: Style.spacing.xs
                Row {
                    spacing: Style.spacing.xs
                    Dropdown {
                        label: "Candidate"
                        value: String(root.client.shownCandidate)
                        options: root.client.candidates.map(function(c) { return { value: c.id, label: (c.name || "Unnamed") + (c.meeting_id ? " (interviewed)" : " (new)") }; })
                        onChanged: function(v) { root.pickCandidate(v, root.client.candidates.some(function(c) { return c.id === v && !c.meeting_id; })); }
                    }
                    TextField { id: newCand; anchors.bottom: parent.bottom; width: Style.space(180); placeholderText: "Candidate name" }
                    IconButton { ts: root.ts; anchors.bottom: parent.bottom; text: "New"; bordered: true; enabled: newCand.text.trim() !== ""; opacity: enabled ? 1 : 0.5
                        onClicked: root.client.send("candidate.create", { name: newCand.text.trim() }, function(r) { root.fail(r); if (!r.ok) return; newCand.text = ""; root.client.refreshCandidates(); root.pickCandidate(r.data.id, true); }) }
                }
                Hint { visible: root.cand === null; text: "Pick or create a candidate to prepare the interview. Their data stays in this interview only." }
                Column {
                    visible: root.cand !== null
                    width: parent.width
                    spacing: Style.spacing.xs
                    Hint { visible: root.candLinked; text: "This interview has been recorded. Its data is read-only here; you can delete it." }
                  Column {   // everything but Delete is locked once the interview has a meeting
                    enabled: !root.candLinked
                    opacity: enabled ? 1 : 0.6
                    width: parent.width
                    spacing: Style.spacing.xs
                    Row {
                        spacing: Style.spacing.xs
                        ToggleSwitch { id: lo; anchors.verticalCenter: parent.verticalCenter; checked: root.cand !== null && root.cand.candidate.local_only === true; onToggled: root.client.send("candidate.local_only", { id: root.cand.candidate.id, on: !checked }, function(r) { root.fail(r); root.client.refreshCand(); }) }
                        Text { anchors.verticalCenter: parent.verticalCenter; textFormat: Text.PlainText; text: "Local-only: speech-to-text and models stay on this machine"; color: Commons.Color.popups.text; font.pixelSize: Style.font.body * root.ts }
                    }
                    Text { textFormat: Text.PlainText; text: "Documents"; color: Commons.Color.popups.text; font.bold: true; font.pixelSize: Style.font.body * root.ts; topPadding: Style.spacing.xs }
                    Repeater {
                        model: root.cand ? root.cand.documents : []
                        Text {
                            required property var modelData
                            width: parent.width
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                            text: modelData.kind.toUpperCase() + "  ·  " + modelData.name + "  ·  " + modelData.chars + " chars"
                            color: Commons.Color.popups.text
                            font.pixelSize: Style.font.body * root.ts
                        }
                    }
                    Repeater {
                        model: [{ kind: "cv", title: "CV" }, { kind: "job", title: "Job description" }]
                        Item {
                            id: arow
                            required property var modelData
                            width: parent.width; height: Math.max(Style.spacing.controlHeight, abtn.implicitHeight)
                            TextField { id: apath; anchors.left: parent.left; anchors.right: abtn.left; anchors.rightMargin: Style.spacing.xs; placeholderText: arow.modelData.title + " file path" }
                            IconButton { id: abtn; anchors.right: parent.right; ts: root.ts; text: "Attach"; bordered: true; tooltipText: "Attach the " + arow.modelData.title; enabled: apath.text.trim() !== ""; opacity: enabled ? 1 : 0.5
                                onClicked: root.client.send("candidate.attach", { id: root.cand.candidate.id, kind: arow.modelData.kind, path: apath.text.trim() }, function(r) { root.fail(r); if (r.ok) { apath.text = ""; root.client.refreshCand(); } }) }
                        }
                    }
                    Row {
                        spacing: Style.spacing.md
                        IconButton { ts: root.ts; text: "Generate question bank"; bordered: true
                            onClicked: { root.genNote = "Generating..."; root.client.send("questions.generate", { candidate: root.cand.candidate.id }, function(r) { root.fail(r); root.genNote = r.ok && r.data.dropped > 0 ? r.data.dropped + " dropped (source not in documents)" : ""; root.client.refreshCand(); }); } }
                        Text { anchors.verticalCenter: parent.verticalCenter; visible: root.genNote !== ""; textFormat: Text.PlainText; text: root.genNote; color: Commons.Color.popups.text; opacity: 0.8; font.pixelSize: Style.font.caption * root.ts }
                    }
                    Text { textFormat: Text.PlainText; text: "Questions"; color: Commons.Color.popups.text; font.bold: true; font.pixelSize: Style.font.body * root.ts; topPadding: Style.spacing.xs }
                    Repeater {
                        model: root.cand ? root.cand.questions : []
                        Column {
                            id: qr
                            required property var modelData
                            required property int index
                            width: parent.width
                            spacing: Style.space(2)
                            Item {
                                width: parent.width; height: Style.spacing.controlHeight
                                Row {
                                    id: qbtns
                                    anchors.right: parent.right
                                    IconButton { ts: root.ts; iconText: String.fromCodePoint(0xf0143); enabled: qr.index > 0; opacity: enabled ? 1 : 0.4; tooltipText: "Move up"; onClicked: root.moveQuestion(qr.index, -1) }
                                    IconButton { ts: root.ts; iconText: String.fromCodePoint(0xf0140); enabled: qr.index < root.cand.questions.length - 1; opacity: enabled ? 1 : 0.4; tooltipText: "Move down"; onClicked: root.moveQuestion(qr.index, 1) }
                                    IconButton { ts: root.ts; iconText: String.fromCodePoint(0xf0156); tooltipText: "Delete question"; onClicked: root.client.send("questions.delete", { id: qr.modelData.id }, function(r) { root.fail(r); root.client.refreshCand(); }) }
                                }
                                TextField { id: qcomp; anchors.right: qbtns.left; anchors.rightMargin: Style.spacing.xs; width: Style.space(130); placeholderText: "Competency"; text: String(qr.modelData.competency || ""); onEditingFinished: { if (text !== String(qr.modelData.competency || "")) root.client.send("questions.update", { id: qr.modelData.id, competency: text }); } }
                                TextField { anchors.left: parent.left; anchors.right: qcomp.left; anchors.rightMargin: Style.spacing.xs; text: qr.modelData.text; onEditingFinished: { if (text.trim() !== "" && text !== qr.modelData.text) root.client.send("questions.update", { id: qr.modelData.id, text: text.trim() }); } }
                            }
                            Hint { text: "\u201c" + String(qr.modelData.source || "") + "\u201d"; font.italic: true }
                        }
                    }
                    Item {
                        width: parent.width; height: Math.max(Style.spacing.controlHeight, addBtn.implicitHeight)
                        TextField { id: addQ; anchors.left: parent.left; anchors.right: addBtn.left; anchors.rightMargin: Style.spacing.xs; placeholderText: "Add a question" }
                        IconButton { id: addBtn; anchors.right: parent.right; ts: root.ts; text: "Add"; bordered: true; enabled: addQ.text.trim() !== ""; opacity: enabled ? 1 : 0.5
                            onClicked: root.client.send("questions.add", { candidate: root.cand.candidate.id, text: addQ.text.trim() }, function(r) { root.fail(r); if (r.ok) { addQ.text = ""; root.client.refreshCand(); } }) }
                    }
                  }
                    IconButton { ts: root.ts; text: root.confirmDelete ? "Click again to delete" : "Delete candidate data"; bordered: true; active: root.confirmDelete
                        tooltipText: "Removes the candidate, documents, questions and the linked meeting"
                        onClicked: {
                            if (!root.confirmDelete) { root.confirmDelete = true; return; }
                            root.confirmDelete = false;
                            root.client.send("candidates.delete", { id: root.cand.candidate.id }, function(r) { root.fail(r); root.client.viewId = ""; root.client.refreshCand(); root.client.refreshCandidates(); });
                        }
                    }
                }
            }
        }

        Column {
            visible: root.tab === "prompts"
            width: parent.width; height: parent.height - y
            spacing: Style.spacing.xs
            Row {
                spacing: Style.spacing.xs
                Repeater {
                    model: ["general", "interview", "team_meeting", "interviewer"]
                    IconButton { ts: root.ts; required property string modelData; text: modelData === "team_meeting" ? "Team" : modelData; active: root.promptMode === modelData; onClicked: root.promptMode = modelData }
                }
                Repeater {
                    model: root.promptKinds
                    IconButton { ts: root.ts; required property string modelData; text: modelData; active: root.promptKind === modelData; onClicked: root.promptKind = modelData }
                }
            }
            Hint { text: root.kindHelp[root.promptKind] || "" }
            ScrollView {
                width: parent.width; height: parent.height - y - promptBtns.height - parent.spacing
                TextArea {
                    id: promptText
                    wrapMode: TextEdit.Wrap
                    color: Commons.Color.popups.text
                    font.pixelSize: Style.font.body * root.ts
                    background: BorderSurface {
                        color: Style.controlFill(promptText.activeFocus, promptText.hovered, Commons.Color.foreground, Commons.Color.accent)
                        borderSpec: Border.controlSpec(promptText.activeFocus ? "focus" : (promptText.hovered ? "hover-cursor" : "normal"), Commons.Color.foreground, Commons.Color.accent)
                        radius: Style.cornerRadius
                    }
                }
            }
            Row {
                id: promptBtns
                spacing: Style.spacing.xs
                IconButton { ts: root.ts; text: "Save"; onClicked: root.client.send("prompts.set", { mode: root.promptMode, kind: root.promptKind, text: promptText.text }) }
                IconButton { ts: root.ts; text: "Reset to default"; onClicked: root.client.send("prompts.reset", { mode: root.promptMode, kind: root.promptKind }, function() { root.loadPrompt(); }) }
            }
        }

        ListView {
            visible: root.tab === "providers"
            width: parent.width; height: parent.height - y
            clip: true
            model: root.providers.filter(function(p) { return root.match(p.name || p.id); })
            delegate: Row {
                id: prow
                required property var modelData
                width: ListView.view.width
                spacing: Style.spacing.xs
                Text {
                    width: Style.space(220)
                    textFormat: Text.PlainText
                    text: (prow.modelData.name || prow.modelData.id) + (prow.modelData.requires_key === false ? "" : (prow.modelData.has_key ? " · key set" : " · no key"))
                    color: Commons.Color.popups.text
                    font.pixelSize: Style.font.body * root.ts
                }
                TextField {
                    id: keyField
                    visible: prow.modelData.requires_key !== false
                    echoMode: TextInput.Password
                    placeholderText: "API key"
                }
                IconButton { ts: root.ts;
                    text: "Set key"
                    visible: keyField.visible
                    enabled: keyField.text !== ""
                    opacity: enabled ? 1 : 0.5
                    onClicked: { root.client.send("settings.set", { key: "api_key", provider: prow.modelData.id, value: keyField.text }); keyField.text = ""; root.load(); }
                }
                IconButton { ts: root.ts;
                    text: "Models"
                    onClicked: root.client.send("models.list", { provider: prow.modelData.id }, function(r) { root.modelsText = r.ok ? (r.data || []).join(", ") : r.error; })
                }
            }
            footer: Text { text: root.modelsText; width: parent.width; wrapMode: Text.Wrap; textFormat: Text.PlainText; color: Commons.Color.popups.text; font.pixelSize: Style.font.caption * root.ts }
        }

        ListView {
            visible: root.tab === "settings"
            width: parent.width; height: parent.height - y
            clip: true
            model: root.found
            header: root.settingsHeader
            delegate: Item {   // label left, control right, both centred on the row
                id: srow
                required property var modelData
                width: ListView.view.width
                height: Math.max(label.implicitHeight, ctl.height) + Style.spacing.sm
                Text {
                    id: label
                    anchors.left: parent.left
                    anchors.right: ctl.left
                    anchors.rightMargin: Style.spacing.md
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    text: root.labelOverrides[srow.modelData.key] || srow.modelData.label || srow.modelData.key
                    color: Commons.Color.popups.text
                    font.pixelSize: Style.font.body * root.ts
                }
                Item {
                    id: ctl
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: Style.space(220)
                    height: Style.spacing.controlHeight
                    ToggleSwitch {
                        visible: typeof srow.modelData.value === "boolean"
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        checked: srow.modelData.value === true
                        onToggled: { root.client.setSetting(srow.modelData.key, !srow.modelData.value); root.search(); }
                    }
                    TextField {
                        visible: typeof srow.modelData.value !== "boolean"
                        anchors.fill: parent
                        text: String(srow.modelData.value)
                        onEditingFinished: root.client.setSetting(srow.modelData.key, typeof srow.modelData.value === "number" ? Number(text) : text)
                    }
                }
            }
        }
    }
}
