// ReportViewerDialog.qml — 1:1 port of vsg_qt/report_dialogs/report_viewer.py
// Displays a report with job table and details panel.

import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import com.vsg.ui 1.0

Dialog {
    id: root
    title: "Report Viewer"
    width: 900
    height: 600
    modal: true
    standardButtons: Dialog.Close

    property string reportPath: ""

    ReportViewerLogic { id: logic }

    Component.onCompleted: {
        if (reportPath.length > 0) {
            logic.load_report(reportPath)
            refreshTable()
        }
    }

    SplitView {
        anchors.fill: parent
        orientation: Qt.Vertical

        // Job table
        ListView {
            id: jobTable
            SplitView.preferredHeight: 250
            clip: true
            model: ListModel { id: jobModel }

            header: Rectangle {
                width: jobTable.width
                height: 30
                color: palette.mid
                RowLayout {
                    anchors.fill: parent; anchors.margins: 4; spacing: 8
                    Label { text: "#"; Layout.preferredWidth: 30; font.bold: true }
                    Label { text: "Name"; Layout.fillWidth: true; font.bold: true }
                    Label { text: "Status"; Layout.preferredWidth: 100; font.bold: true }
                    Label { text: "Delays"; Layout.preferredWidth: 150; font.bold: true }
                }
            }

            delegate: Rectangle {
                width: jobTable.width
                height: 30
                color: ListView.isCurrentItem ? palette.highlight : (index % 2 === 0 ? palette.base : palette.alternateBase)

                RowLayout {
                    anchors.fill: parent; anchors.margins: 4; spacing: 8
                    Label { text: String(index + 1); Layout.preferredWidth: 30 }
                    Label { text: model.name || ""; Layout.fillWidth: true; elide: Text.ElideRight }
                    Label {
                        text: model.status || ""
                        Layout.preferredWidth: 100
                        color: model.status === "Failed" ? "#e74c3c" : model.status === "Merged" ? "#27ae60" : "orange"
                    }
                    Label { text: model.delays || ""; Layout.preferredWidth: 150 }
                }

                MouseArea {
                    anchors.fill: parent
                    onClicked: {
                        jobTable.currentIndex = index
                        showDetails(index)
                    }
                }
            }
        }

        // Details panel
        ScrollView {
            SplitView.fillHeight: true
            TextArea {
                id: detailsArea
                readOnly: true
                font.family: "monospace"
                wrapMode: TextEdit.Wrap
                text: "Select a job to see details."
            }
        }
    }

    function refreshTable() {
        jobModel.clear()
        for (var i = 0; i < logic.job_count; i++) {
            var job = JSON.parse(logic.get_job_data(i))
            jobModel.append({
                name: job.name || "",
                status: job.status || "",
                delays: formatDelays(job.delays)
            })
        }
    }

    function showDetails(index) {
        var details = JSON.parse(logic.get_job_details(index))
        var text = "=== " + (details.name || "Job") + " ===\n\n"
        text += "Status: " + (details.status || "Unknown") + "\n"
        if (details.error) text += "Error: " + details.error + "\n"
        if (details.output) text += "Output: " + details.output + "\n"
        if (details.delays) {
            text += "\nDelays:\n"
            for (var key in details.delays) {
                text += "  " + key + ": " + details.delays[key] + " ms\n"
            }
        }
        detailsArea.text = text
    }

    function formatDelays(delays) {
        if (!delays) return "—"
        var parts = []
        for (var key in delays) {
            parts.push(key + ": " + delays[key] + "ms")
        }
        return parts.join(", ") || "—"
    }
}
