// AddJobDialog.qml — 1:1 port of vsg_qt/add_job_dialog/ui.py
// Dynamic source inputs with drag-and-drop and job discovery.

import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import QtQuick.Dialogs
import com.vsg.ui 1.0

Dialog {
    id: root
    title: "Add Job(s) to Queue"
    width: 700
    height: 300
    modal: true
    standardButtons: Dialog.Ok | Dialog.Cancel

    property var discoveredJobs: []

    AddJobLogic {
        id: logic
        onDiscoveryError: function(message) {
            errorDialog.text = message
            errorDialog.open()
        }
    }

    MessageDialog {
        id: errorDialog
        title: "Job Discovery"
        buttons: MessageDialog.Ok
    }

    onAccepted: {
        var result = logic.find_jobs()
        var jobs = JSON.parse(result)
        if (jobs.length > 0) {
            discoveredJobs = jobs
        } else {
            // Don't close if no jobs found — error signal handles the message
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 8
        spacing: 6

        ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                id: inputsColumn
                width: parent.width
                spacing: 4

                Repeater {
                    model: logic.source_count
                    delegate: RowLayout {
                        spacing: 4
                        Layout.fillWidth: true

                        Label {
                            text: index === 0 ? "Source 1 (Reference):" : "Source " + (index + 1) + ":"
                            Layout.preferredWidth: 140
                        }
                        TextField {
                            id: pathField
                            text: logic.get_source_path(index)
                            onTextChanged: logic.set_source_path(index, text)
                            Layout.fillWidth: true
                        }
                        Button {
                            text: "Browse…"
                            onClicked: {
                                fileDialog.sourceIndex = index
                                fileDialog.open()
                            }
                        }
                    }
                }
            }
        }

        Button {
            text: "Add Another Source"
            onClicked: logic.add_source_input()
        }
    }

    FileDialog {
        id: fileDialog
        property int sourceIndex: 0
        title: "Select Source"
        onAccepted: {
            logic.set_source_path(sourceIndex, selectedFile.toString().replace("file://", ""))
        }
    }

    function getDiscoveredJobs() {
        return discoveredJobs
    }
}
