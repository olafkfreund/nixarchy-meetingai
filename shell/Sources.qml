pragma ComponentBehavior: Bound
// qmllint disable missing-property
// ^ qs.Commons / qs.Ui expose nested QtObjects (Style.spacing, Color.popups) the linter cannot type
import QtQuick
import qs.Commons
import qs.Commons as Commons

// "Based on" chips under an answer (NowCard and the archived card), or a dim caption when the
// answer had meeting context and cited none of it. `sources` is the JSON string Client keeps.
Column {
    id: root

    required property var daemon
    required property var theme
    property string sources: ""
    property int citeMax: 0
    property int citeNotes: 0
    property bool done: true
    property bool show: true
    readonly property var list: { try { return JSON.parse(root.sources || "[]"); } catch (e) { return []; } }
    readonly property bool uncited: root.done && root.sources !== "" && root.list.length === 0 && root.citeMax + root.citeNotes > 0   // "": answer.sources never came (cancelled)

    visible: root.show && (root.list.length > 0 || root.uncited)
    spacing: Style.space(4)

    Flow {
        objectName: "basedOn"
        visible: root.list.length > 0
        width: parent.width
        spacing: Style.spacing.xs
        UiText { height: root.theme.ctl; verticalAlignment: Text.AlignVCenter; textFormat: Text.PlainText; text: "Based on"; color: Commons.Color.popups.text; opacity: root.theme.dim; font.pixelSize: root.theme.caption }
        Repeater {
            model: root.list
            IconButton {
                id: chip
                required property var modelData
                required property int index
                objectName: "sourceChip" + index
                ts: root.theme.ts
                bordered: true
                text: modelData.kind === "notes" ? "Notes · " + root.daemon.fmtTime(modelData.t0) + "–" + root.daemon.fmtTime(modelData.t1)
                    : root.daemon.fmtTime(modelData.t0) + (String(modelData.speaker || "") !== "" ? " · " + modelData.speaker : "")
                tooltipText: String(modelData.snippet || "")
                onClicked: root.daemon.openLine(String(modelData.id))
            }
        }
    }
    UiText {
        objectName: "noCitations"
        visible: root.uncited
        width: parent.width
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        text: "No meeting lines cited"
        color: Commons.Color.popups.text
        opacity: root.theme.dim
        font.italic: true
        font.pixelSize: root.theme.caption
    }
}
