// BatchCompletionDialog.qml — 1:1 port of vsg_qt/report_dialogs/batch_completion_dialog.py
// Shows batch processing summary with success/warning/fail counts.

import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import com.vsg.ui 1.0

Dialog {
    id: root
    title: "Batch Processing Complete"
    width: 500
    height: 350
    modal: true
    standardButtons: Dialog.Ok

    BatchCompletionLogic {
        id: logic
        onOpenReportViewer: function(path) {
            reportViewer.reportPath = path
            reportViewer.open()
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 12

        // Status icon + summary
        RowLayout {
            spacing: 12
            Rectangle {
                width: 48; height: 48; radius: 24
                color: logic.failed > 0 ? "#e74c3c" : logic.warnings > 0 ? "#f39c12" : "#27ae60"
                Label {
                    anchors.centerIn: parent
                    text: logic.failed > 0 ? "✗" : "✓"
                    font.pixelSize: 24
                    color: "white"
                }
            }
            ColumnLayout {
                Label {
                    text: "Batch Complete"
                    font.bold: true
                    font.pixelSize: 16
                }
                Label {
                    text: logic.total_jobs + " jobs processed"
                }
            }
        }

        // Stats
        GroupBox {
            title: "Results"
            Layout.fillWidth: true
            GridLayout {
                columns: 2
                anchors.fill: parent
                Label { text: "Successful:" }
                Label { text: String(logic.successful); color: "#27ae60"; font.bold: true }
                Label { text: "Warnings:" }
                Label { text: String(logic.warnings); color: logic.warnings > 0 ? "#f39c12" : "gray" }
                Label { text: "Failed:" }
                Label { text: String(logic.failed); color: logic.failed > 0 ? "#e74c3c" : "gray" }
            }
        }

        // Report button
        Button {
            text: "Show Report"
            Layout.alignment: Qt.AlignHCenter
            visible: logic.report_path.length > 0
            onClicked: logic.show_report()
        }
    }

    ReportViewerDialog {
        id: reportViewer
    }
}
