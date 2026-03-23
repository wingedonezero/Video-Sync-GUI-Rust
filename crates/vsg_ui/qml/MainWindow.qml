// MainWindow.qml — 1:1 port of vsg_qt/main_window/window.py
//
// Pure declarative UI shell. All logic is in MainController (controller.rs).
// Layout matches the Python PySide6 version exactly:
//   - Settings button (top row)
//   - "Main Workflow" group (Open Job Queue button + archive logs checkbox)
//   - "Quick Analysis" group (3 source inputs with browse buttons + Analyze button)
//   - Status row (status label + progress bar)
//   - "Latest Job Results" group (delay labels for Source 2, 3, 4)
//   - "Log" group (read-only monospace text area)

import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import QtQuick.Dialogs
import QtQuick.Window 2.15
import com.vsg.ui 1.0

ApplicationWindow {
    id: root
    title: "Video/Audio Sync & Merge - Rust Edition"
    width: 1000
    height: 600
    x: 100
    y: 100

    // The controller QObject handles all logic
    MainController {
        id: controller

        onOpenDialogRequested: function(dialogName) {
            switch (dialogName) {
                case "OptionsDialog":
                    optionsDialog.settingsJson = controller.get_settings_json()
                    optionsDialog.open()
                    break
                case "JobQueueDialog":
                    jobQueueDialog.open()
                    break
                case "BatchCompletionDialog":
                    batchCompletionDialog.open()
                    break
            }
        }

        onWorkerStartRequested: function(jobsJson, andMerge, outputDir, settingsJson) {
            // Worker runs in a background thread via the runner module.
            // The QML side handles signal routing from WorkerSignals.
            controller.set_status_text("Worker started...")
        }
    }

    Component.onCompleted: controller.initialize()

    onClosing: function(close) {
        controller.on_close()
        close.accepted = true
    }

    // ── Dialogs ──

    OptionsDialog {
        id: optionsDialog
        onAccepted: {
            controller.update_settings_from_json(JSON.stringify(optionsDialog.collectSettings()))
        }
    }

    JobQueueDialog {
        id: jobQueueDialog
        tempRoot: {
            var settings = JSON.parse(controller.get_settings_json())
            return settings.temp_root || ""
        }
        onAccepted: {
            var finalJobsJson = jobQueueDialog.getFinalJobs()
            var jobs = JSON.parse(finalJobsJson)
            if (jobs.length > 0) {
                // Determine output dir from settings
                var settings = JSON.parse(controller.get_settings_json())
                var outputDir = settings.output_folder || "sync_output"
                controller.workerStartRequested(finalJobsJson, true, outputDir, controller.get_settings_json())
            }
        }
    }

    BatchCompletionDialog {
        id: batchCompletionDialog
    }

    // File dialogs for Browse buttons
    FileDialog {
        id: browseDialog
        property int sourceIndex: 0
        title: "Select File or Directory"
        onAccepted: {
            var path = selectedFile.toString().replace("file://", "")
            switch (sourceIndex) {
                case 1: controller.ref_path = path; break
                case 2: controller.sec_path = path; break
                case 3: controller.ter_path = path; break
            }
        }
    }

    // ── Main Layout ──

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 8
        spacing: 6

        // ── Top Row: Settings button ──
        RowLayout {
            Layout.fillWidth: true
            Button {
                text: "Settings…"
                onClicked: controller.open_options_dialog()
            }
            Item { Layout.fillWidth: true }
        }

        // ── Main Workflow Group ──
        GroupBox {
            title: "Main Workflow"
            Layout.fillWidth: true

            ColumnLayout {
                anchors.fill: parent
                spacing: 4

                Button {
                    text: "Open Job Queue for Merging..."
                    font.pixelSize: 14
                    padding: 5
                    Layout.fillWidth: true
                    enabled: !controller.worker_running
                    onClicked: controller.open_job_queue()
                }

                CheckBox {
                    text: "Archive logs to a zip file on batch completion"
                    checked: controller.archive_logs
                    onCheckedChanged: controller.archive_logs = checked
                }
            }
        }

        // ── Quick Analysis Group ──
        GroupBox {
            title: "Quick Analysis (Analyze Only)"
            Layout.fillWidth: true

            ColumnLayout {
                anchors.fill: parent
                spacing: 4

                // Source 1 (Reference)
                RowLayout {
                    spacing: 4
                    Label { text: "Source 1 (Reference):"; Layout.preferredWidth: 140 }
                    TextField {
                        text: controller.ref_path
                        onTextChanged: controller.ref_path = text
                        Layout.fillWidth: true
                    }
                    Button {
                        text: "Browse…"
                        onClicked: { browseDialog.sourceIndex = 1; browseDialog.open() }
                    }
                }

                // Source 2
                RowLayout {
                    spacing: 4
                    Label { text: "Source 2:"; Layout.preferredWidth: 140 }
                    TextField {
                        text: controller.sec_path
                        onTextChanged: controller.sec_path = text
                        Layout.fillWidth: true
                    }
                    Button {
                        text: "Browse…"
                        onClicked: { browseDialog.sourceIndex = 2; browseDialog.open() }
                    }
                }

                // Source 3
                RowLayout {
                    spacing: 4
                    Label { text: "Source 3:"; Layout.preferredWidth: 140 }
                    TextField {
                        text: controller.ter_path
                        onTextChanged: controller.ter_path = text
                        Layout.fillWidth: true
                    }
                    Button {
                        text: "Browse…"
                        onClicked: { browseDialog.sourceIndex = 3; browseDialog.open() }
                    }
                }

                // Analyze button (right-aligned)
                RowLayout {
                    Item { Layout.fillWidth: true }
                    Button {
                        text: "Analyze Only"
                        enabled: !controller.worker_running
                        onClicked: controller.start_analyze_only()
                    }
                }
            }
        }

        // ── Status Row ──
        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            Label { text: "Status:" }
            Label {
                text: controller.status_text
                Layout.fillWidth: true
            }
            ProgressBar {
                from: 0
                to: 100
                value: controller.progress_value * 100
                Layout.preferredWidth: 200
            }
        }

        // ── Latest Job Results Group ──
        GroupBox {
            title: "Latest Job Results"
            Layout.fillWidth: true

            RowLayout {
                anchors.fill: parent
                spacing: 8

                Label { text: "Source 2 Delay:" }
                Label { text: controller.sec_delay_text.length > 0 ? controller.sec_delay_text : "—" }

                Item { implicitWidth: 20 }

                Label { text: "Source 3 Delay:" }
                Label { text: controller.ter_delay_text.length > 0 ? controller.ter_delay_text : "—" }

                Item { implicitWidth: 20 }

                Label { text: "Source 4 Delay:" }
                Label { text: controller.src4_delay_text.length > 0 ? controller.src4_delay_text : "—" }

                Item { Layout.fillWidth: true }
            }
        }

        // ── Log Group ──
        GroupBox {
            title: "Log"
            Layout.fillWidth: true
            Layout.fillHeight: true

            ScrollView {
                anchors.fill: parent

                TextArea {
                    id: logArea
                    text: controller.log_text
                    readOnly: true
                    font.family: "monospace"
                    wrapMode: TextEdit.Wrap
                }
            }
        }
    }
}
