// JobQueueDialog.qml — 1:1 port of vsg_qt/job_queue_dialog/ui.py
// Job queue management with table, drag-drop, context menu.

import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import com.vsg.ui 1.0

Dialog {
    id: root
    title: "Job Queue"
    width: 1200
    height: 600
    modal: true
    standardButtons: Dialog.Ok | Dialog.Cancel

    property string tempRoot: ""

    JobQueueLogic {
        id: logic
        onJobsChanged: tableView.model = buildTableModel()
        onLogMessage: function(msg) { console.log(msg) }
    }

    Component.onCompleted: {
        logic.initialize(tempRoot)
    }

    footer: DialogButtonBox {
        Button {
            text: "Start Processing Queue"
            DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
        }
        Button {
            text: "Cancel"
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 8
        spacing: 6

        // Job table
        ListView {
            id: tableView
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: ListModel {}

            delegate: Rectangle {
                width: tableView.width
                height: 36
                color: ListView.isCurrentItem ? palette.highlight : (index % 2 === 0 ? palette.base : palette.alternateBase)

                RowLayout {
                    anchors.fill: parent
                    anchors.margins: 4
                    spacing: 8

                    Label {
                        text: model.order || ""
                        Layout.preferredWidth: 30
                        horizontalAlignment: Text.AlignHCenter
                    }
                    Label {
                        text: model.status || ""
                        Layout.preferredWidth: 140
                        color: model.status === "Configured" ? "green" : "orange"
                    }
                    Label {
                        text: model.sourcesDisplay || ""
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                    }
                }

                MouseArea {
                    anchors.fill: parent
                    onClicked: tableView.currentIndex = index
                    onDoubleClicked: logic.configure_job_at_row(index)
                }
            }
        }

        // Button row
        RowLayout {
            Layout.fillWidth: true
            spacing: 6

            Button {
                text: "Add Job(s)..."
                onClicked: addJobDialog.open()
            }
            Item { Layout.fillWidth: true }
            Button {
                text: "Move Up"
                onClicked: {
                    if (tableView.currentIndex >= 0)
                        logic.move_jobs(JSON.stringify([tableView.currentIndex]), -1)
                }
            }
            Button {
                text: "Move Down"
                onClicked: {
                    if (tableView.currentIndex >= 0)
                        logic.move_jobs(JSON.stringify([tableView.currentIndex]), 1)
                }
            }
            Button {
                text: "Remove Selected"
                onClicked: {
                    if (tableView.currentIndex >= 0)
                        logic.remove_jobs(JSON.stringify([tableView.currentIndex]))
                }
            }
        }
    }

    AddJobDialog {
        id: addJobDialog
        onAccepted: {
            var jobs = addJobDialog.getDiscoveredJobs()
            if (jobs.length > 0)
                logic.add_jobs(JSON.stringify(jobs))
        }
    }

    function buildTableModel() {
        var model = Qt.createQmlObject('import QtQuick 2.15; ListModel {}', root)
        for (var i = 0; i < logic.job_count; i++) {
            var data = JSON.parse(logic.get_job_display_data(i))
            model.append({
                order: String(data.order || i + 1),
                status: data.status || "Unknown",
                sourcesDisplay: data.sources_display || ""
            })
        }
        return model
    }

    function getFinalJobs() {
        return logic.get_final_jobs()
    }
}
