pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects (Style.spacing, Color.popups) the linter cannot type
import QtQuick
import QtQuick.Controls
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// Interviewer: the Question bank checklist (lives inside ItemsPanel's Flickable).
Column {
    id: qcol

    required property var daemon
    required property real ts

    readonly property var qStatuses: [{ status: "asked", title: "Asked" }, { status: "covered", title: "Covered" }, { status: "skipped", title: "Skipped" }]

    spacing: Style.space(8)
    Text { textFormat: Text.PlainText; text: "Question bank" + (qcol.daemon.cand ? "  " + qcol.daemon.cand.questions.filter(function(q) { return q.status !== "planned"; }).length + "/" + qcol.daemon.cand.questions.length : ""); color: Commons.Color.popups.text; font.pixelSize: Style.font.caption * qcol.ts; font.bold: true; font.capitalization: Font.SmallCaps; font.letterSpacing: 1.5 }
    Text { visible: !qcol.daemon.cand || qcol.daemon.cand.questions.length === 0; textFormat: Text.PlainText; text: "No questions: pick a candidate and generate a bank in Settings, Candidate"; color: Commons.Color.popups.text; opacity: 0.6; font.italic: true; font.pixelSize: Style.font.body * qcol.ts }
    Repeater {
        model: qcol.daemon.cand ? qcol.daemon.cand.questions : []
        Column {
            id: qrow
            required property var modelData
            width: qcol.width
            spacing: Style.space(2)
            Item {
                width: parent.width
                height: Math.max(Style.spacing.controlHeight, qtext.implicitHeight + Style.space(4))
                Row {
                    id: qacts
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    Repeater {
                        model: qcol.qStatuses
                        IconButton {
                            required property var modelData
                            ts: qcol.ts
                            text: modelData.title
                            active: qrow.modelData.status === modelData.status
                            tooltipText: active ? "Back to planned" : "Mark " + modelData.status
                            onClicked: qcol.daemon.updateQuestion(qrow.modelData.id, { status: active ? "planned" : modelData.status })
                        }
                    }
                }
                Text {
                    id: qtext
                    anchors.left: parent.left; anchors.right: qacts.left; anchors.rightMargin: Style.spacing.md
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    text: qrow.modelData.text
                    color: Commons.Color.popups.text
                    opacity: qrow.modelData.status === "planned" ? 1 : 0.6
                    font.pixelSize: Style.font.body * qcol.ts
                    font.strikeout: qrow.modelData.status === "skipped"
                }
            }
            TextField {   // evidence note: what the candidate said; never generated
                width: parent.width
                placeholderText: "Evidence note"
                text: String(qrow.modelData.note || "")
                onEditingFinished: { if (text !== String(qrow.modelData.note || "")) qcol.daemon.updateQuestion(qrow.modelData.id, { note: text }); }
            }
        }
    }
}
