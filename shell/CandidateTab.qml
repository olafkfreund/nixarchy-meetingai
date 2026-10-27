pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects (Style.spacing, Color.popups) the linter cannot type
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// Library's Candidate tab (interviewer mode): Candidate / Documents / Question bank / Danger zone.
// A candidate whose interview was recorded is read-only except for deletion.
Item {
    id: root

    required property var client
    required property var theme

    signal confirmRequested(string message, string label, var yes)

    readonly property var cand: client.cand   // candidate.get of the shown candidate
    readonly property bool linked: root.cand !== null && !!root.cand.candidate.meeting_id
    property bool newOpen: false         // the inline "new candidate" name row
    property string attachKind: ""       // "cv" | "job": which document the open path row attaches
    property bool generating: false
    property string genNote: ""

    property var errors: ({})   // the last failed request per section, shown under that section's header
    function fail(r, section) { var e = Object.assign({}, root.errors); e[section] = r.ok ? "" : String(r.error); root.errors = e; }
    // Any candidate can be opened to view or delete; only one without a meeting is chosen for a new interview.
    function pickCandidate(id, unlinked) {
        root.client.viewCandidate(id);
        if (unlinked) root.client.send("session.new", { candidate: id }, function(r) { root.fail(r, "candidate"); });
    }
    function moveQuestion(i, d) {
        var ids = root.cand.questions.map(function(q) { return q.id; }), t = ids[i + d];
        ids[i + d] = ids[i]; ids[i] = t;
        root.client.send("questions.reorder", { candidate: root.cand.candidate.id, ids: ids }, function(r) { root.fail(r, "questions"); root.client.refreshCand(); });
    }
    function hook(what) {   // tests/harness-live.qml opens the inline rows and the dialog
        if (what === "new") root.newOpen = true;
        else if (what === "attach") root.attachKind = "cv";
        else if (what === "delete") root.askDelete();
    }
    function plural(n, word) { return n + " " + word + (n === 1 ? "" : "s"); }
    function askDelete() {
        var c = root.cand;
        if (c === null) return;
        root.confirmRequested("Delete " + (c.candidate.name || "this candidate") + "? This removes their " + root.plural(c.documents.length, "document") + " and " + root.plural(c.questions.length, "question") + (root.linked ? ", and the recorded interview with its notes and transcript" : "") + ". It cannot be undone.", "Delete",
            function() { root.client.send("candidates.delete", { id: c.candidate.id }, function(r) { root.fail(r, "danger"); root.client.viewId = ""; root.client.refreshCand(); root.client.refreshCandidates(); }); });
    }
    onCandChanged: { root.attachKind = ""; root.genNote = ""; root.errors = ({}); }

    component Section: PanelSectionHeader { fontSize: root.theme.caption }
    component Note: UiText {
        Layout.fillWidth: true
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: Commons.Color.popups.text
        opacity: 0.7
        font.pixelSize: root.theme.caption
    }
    component Err: UiText {   // the section's last failure
        property string section
        visible: !!root.errors[section]
        width: parent.width
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        text: String(root.errors[section] || "")
        color: Commons.Color.urgent
        font.pixelSize: root.theme.caption
    }
    component QField: TextArea {   // wraps; the field grows with its text
        id: fld
        wrapMode: TextEdit.Wrap
        color: Commons.Color.popups.text
        font.family: Style.font.family
        font.pixelSize: root.theme.body
        background: BorderSurface {
            color: Style.controlFill(fld.activeFocus, fld.hovered, Commons.Color.foreground, Commons.Color.accent)
            borderSpec: Border.controlSpec(fld.activeFocus ? "focus" : (fld.hovered ? "hover-cursor" : "normal"), Commons.Color.foreground, Commons.Color.accent)
            radius: Style.cornerRadius
        }
    }

    ScrollView {
        id: sv
        anchors.fill: parent
        contentWidth: availableWidth
        clip: true
        Column {
            id: col
            width: sv.availableWidth - root.theme.gap   // room for the scroll bar
            spacing: root.theme.section

            Column {   // Candidate
                width: parent.width
                spacing: root.theme.gap
                Section { text: "Candidate" }
                    Err { section: "candidate" }
                RowLayout {
                    width: parent.width
                    spacing: root.theme.gap
                    Select {
                        objectName: "candidatePick"
                        accessibleName: "Candidate"
                        theme: root.theme
                        Layout.fillWidth: true
                        value: String(root.client.shownCandidate)
                        options: root.client.candidates.map(function(c) { return { value: c.id, label: (c.name || "Unnamed") + (c.meeting_id ? " (interviewed)" : " (new)") }; })
                        onChanged: function(v) { root.pickCandidate(v, root.client.candidates.some(function(c) { return c.id === v && !c.meeting_id; })); }
                    }
                    IconButton { objectName: "candidateNew"; ts: root.theme.ts; text: "New…"; bordered: true; active: root.newOpen; tooltipText: "Create a candidate"; onClicked: root.newOpen = !root.newOpen }
                }
                RowLayout {   // inline name row: Create / Cancel
                    visible: root.newOpen
                    width: parent.width
                    spacing: root.theme.gap
                    Input { id: newName; objectName: "candidateName"; ts: root.theme.ts; Layout.fillWidth: true; placeholderText: "Candidate name"; onAccepted: create.clicked() }
                    IconButton {
                        id: create
                        objectName: "candidateCreate"
                        ts: root.theme.ts; text: "Create"; bordered: true
                        enabled: newName.text.trim() !== ""; opacity: enabled ? 1 : 0.5
                        onClicked: root.client.send("candidate.create", { name: newName.text.trim() }, function(r) { root.fail(r, "candidate"); if (!r.ok) return; newName.text = ""; root.newOpen = false; root.client.refreshCandidates(); root.pickCandidate(r.data.id, true); })
                    }
                    IconButton { ts: root.theme.ts; text: "Cancel"; onClicked: { newName.text = ""; root.newOpen = false; } }
                }
                Rectangle {   // an interview that already happened
                    visible: root.linked
                    width: parent.width
                    height: ro.implicitHeight + root.theme.padY * 2
                    radius: root.theme.radius
                    color: Qt.alpha(Commons.Color.accent, 0.12)
                    UiText {
                        id: ro
                        x: root.theme.padX; y: root.theme.padY
                        width: parent.width - root.theme.padX * 2
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        text: "This interview has been recorded. Its data is read-only here; you can delete it below."
                        color: Commons.Color.popups.text
                        font.pixelSize: root.theme.body
                    }
                }
                RowLayout {
                    visible: root.cand !== null
                    enabled: !root.linked
                    opacity: enabled ? 1 : 0.6
                    width: parent.width
                    spacing: root.theme.gap
                    Check { objectName: "localOnly"; accessibleName: "Local-only"; Layout.alignment: Qt.AlignVCenter; checked: root.cand !== null && root.cand.candidate.local_only === true; onToggled: root.client.send("candidate.local_only", { id: root.cand.candidate.id, on: !checked }, function(r) { root.fail(r, "candidate"); root.client.refreshCand(); }) }
                    UiText { Layout.fillWidth: true; Layout.alignment: Qt.AlignVCenter; wrapMode: Text.Wrap; textFormat: Text.PlainText; text: "Local-only: speech-to-text and models stay on this machine"; color: Commons.Color.popups.text; font.pixelSize: root.theme.body }
                }
                RowLayout {
                    visible: root.cand === null
                    width: parent.width
                    Note { text: "Pick or create a candidate to prepare the interview. Their data stays in this interview only." }
                }
            }

            Column {   // everything below needs a candidate
                visible: root.cand !== null
                width: parent.width
                spacing: root.theme.section

                Column {   // Documents
                    enabled: !root.linked
                    opacity: enabled ? 1 : 0.6
                    width: parent.width
                    spacing: root.theme.gap
                    Section { text: "Documents" }
                    Err { section: "documents" }
                    Repeater {
                        model: root.cand ? root.cand.documents : []
                        UiText {
                            required property var modelData
                            width: parent.width
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                            text: (modelData.kind === "cv" ? "CV" : "Job description") + "  ·  " + modelData.name + "  ·  " + modelData.chars + " chars"
                            color: Commons.Color.popups.text
                            font.pixelSize: root.theme.body
                        }
                    }
                    RowLayout {
                        visible: root.cand !== null && root.cand.documents.length === 0
                        width: parent.width
                        Note { text: "No documents yet. Attach the CV and the job description; questions are generated from them." }
                    }
                    Flow {
                        width: parent.width
                        spacing: root.theme.gap
                        IconButton { objectName: "attachCv"; ts: root.theme.ts; text: "Attach CV…"; bordered: true; active: root.attachKind === "cv"; onClicked: root.attachKind = root.attachKind === "cv" ? "" : "cv" }
                        IconButton { objectName: "attachJob"; ts: root.theme.ts; text: "Attach job description…"; bordered: true; active: root.attachKind === "job"; onClicked: root.attachKind = root.attachKind === "job" ? "" : "job" }
                    }
                    RowLayout {   // the path row of the document being attached
                        visible: root.attachKind !== ""
                        width: parent.width
                        spacing: root.theme.gap
                        Input { id: apath; objectName: "attachPath"; ts: root.theme.ts; Layout.fillWidth: true; placeholderText: root.attachKind === "cv" ? "Path to the CV file" : "Path to the job description file" }
                        IconButton {
                            ts: root.theme.ts; text: "Attach"; bordered: true
                            enabled: apath.text.trim() !== ""; opacity: enabled ? 1 : 0.5
                            onClicked: root.client.send("candidate.attach", { id: root.cand.candidate.id, kind: root.attachKind, path: apath.text.trim() }, function(r) { root.fail(r, "documents"); if (r.ok) { apath.text = ""; root.attachKind = ""; root.client.refreshCand(); } })
                        }
                        IconButton { ts: root.theme.ts; text: "Cancel"; onClicked: { apath.text = ""; root.attachKind = ""; } }
                    }
                }

                Column {   // Question bank
                    enabled: !root.linked
                    opacity: enabled ? 1 : 0.6
                    width: parent.width
                    spacing: root.theme.gap
                    Section { text: "Question bank" + (root.cand ? "  " + root.cand.questions.length : "") }
                    Err { section: "questions" }
                    RowLayout {
                        width: parent.width
                        spacing: root.theme.gap
                        IconButton {
                            objectName: "generate"
                            ts: root.theme.ts; text: root.generating ? "Generating…" : "Generate question bank"; bordered: true
                            enabled: !root.generating; opacity: enabled ? 1 : 0.6
                            onClicked: {
                                root.generating = true; root.genNote = "";
                                root.client.send("questions.generate", { candidate: root.cand.candidate.id }, function(r) {
                                    root.generating = false; root.fail(r, "questions");
                                    root.genNote = r.ok && r.data.dropped > 0 ? r.data.dropped + " dropped (source not in documents)" : "";
                                    root.client.refreshCand();
                                });
                            }
                        }
                        Note { text: root.generating ? "Reading the documents and writing questions. This can take a minute." : root.genNote; visible: text !== ""; Layout.alignment: Qt.AlignVCenter }
                    }
                    Repeater {
                        model: root.cand ? root.cand.questions : []
                        Rectangle {   // one card per question
                            id: qc
                            required property var modelData
                            required property int index
                            width: parent.width
                            height: qcol.implicitHeight + root.theme.padY * 2
                            radius: root.theme.radius
                            color: Qt.alpha(Commons.Color.foreground, 0.06)
                            Column {
                                id: qcol
                                x: root.theme.padX; y: root.theme.padY
                                width: parent.width - root.theme.padX * 2
                                spacing: root.theme.gap
                                QField {
                                    objectName: "questionText"
                    Accessible.name: "Question text"
                                    width: parent.width
                                    text: qc.modelData.text
                                    onActiveFocusChanged: { if (!activeFocus && text.trim() !== "" && text !== qc.modelData.text) root.client.send("questions.update", { id: qc.modelData.id, text: text.trim() }); }
                                }
                                UiText {
                                    visible: String(qc.modelData.source || "") !== ""
                                    width: parent.width
                                    textFormat: Text.PlainText
                                    wrapMode: Text.Wrap
                                    text: "“" + String(qc.modelData.source || "") + "”"
                                    color: Commons.Color.popups.text
                                    opacity: 0.7
                                    font.italic: true
                                    font.pixelSize: root.theme.caption
                                }
                                RowLayout {   // footer: competency tag left, order and delete right
                                    width: parent.width
                                    spacing: root.theme.gap
                                    Input { objectName: "questionCompetency"; ts: root.theme.ts; Layout.preferredWidth: Math.round(Style.space(190) * root.theme.ts); placeholderText: "Competency"; text: String(qc.modelData.competency || ""); onEditingFinished: { if (text !== String(qc.modelData.competency || "")) root.client.send("questions.update", { id: qc.modelData.id, competency: text }); } }
                                    Item { Layout.fillWidth: true }
                                    IconButton { ts: root.theme.ts; iconText: String.fromCodePoint(0xf0143); enabled: qc.index > 0; opacity: enabled ? 1 : 0.4; tooltipText: "Move up"; onClicked: root.moveQuestion(qc.index, -1) }
                                    IconButton { ts: root.theme.ts; iconText: String.fromCodePoint(0xf0140); enabled: qc.index < root.cand.questions.length - 1; opacity: enabled ? 1 : 0.4; tooltipText: "Move down"; onClicked: root.moveQuestion(qc.index, 1) }
                                    IconButton { ts: root.theme.ts; iconText: String.fromCodePoint(0xf0156); tooltipText: "Delete question"; onClicked: root.client.send("questions.delete", { id: qc.modelData.id }, function(r) { root.fail(r, "questions"); root.client.refreshCand(); }) }
                                }
                            }
                        }
                    }
                    RowLayout {
                        width: parent.width
                        spacing: root.theme.gap
                        Input { id: addQ; objectName: "addQuestion"; ts: root.theme.ts; Layout.fillWidth: true; placeholderText: "Add a question"; onAccepted: addBtn.clicked() }
                        IconButton {
                            id: addBtn
                            ts: root.theme.ts; text: "Add"; bordered: true
                            enabled: addQ.text.trim() !== ""; opacity: enabled ? 1 : 0.5
                            onClicked: root.client.send("questions.add", { candidate: root.cand.candidate.id, text: addQ.text.trim() }, function(r) { root.fail(r, "questions"); if (r.ok) { addQ.text = ""; root.client.refreshCand(); } })
                        }
                    }
                }

                Column {   // Danger zone
                    width: parent.width
                    spacing: root.theme.gap
                    Section { text: "Danger zone" }
                    Err { section: "danger" }
                    Flow {
                        width: parent.width
                        IconButton { objectName: "deleteCandidate"; ts: root.theme.ts; text: "Delete candidate…"; bordered: true; tooltipText: "Remove this candidate and everything linked to them"; onClicked: root.askDelete() }
                    }
                }
            }
        }
    }
}
