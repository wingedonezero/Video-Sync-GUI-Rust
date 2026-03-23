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
    }

    onClosing: function(close) {
        controller.on_close()
        close.accepted = true
    }

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
                    Label {
                        text: "Source 1 (Reference):"
                        Layout.preferredWidth: 140
                    }
                    TextField {
                        text: controller.ref_path
                        onTextChanged: controller.ref_path = text
                        Layout.fillWidth: true
                    }
                    Button {
                        text: "Browse…"
                        onClicked: controller.browse_for_path(1)
                    }
                }

                // Source 2
                RowLayout {
                    spacing: 4
                    Label {
                        text: "Source 2:"
                        Layout.preferredWidth: 140
                    }
                    TextField {
                        text: controller.sec_path
                        onTextChanged: controller.sec_path = text
                        Layout.fillWidth: true
                    }
                    Button {
                        text: "Browse…"
                        onClicked: controller.browse_for_path(2)
                    }
                }

                // Source 3
                RowLayout {
                    spacing: 4
                    Label {
                        text: "Source 3:"
                        Layout.preferredWidth: 140
                    }
                    TextField {
                        text: controller.ter_path
                        onTextChanged: controller.ter_path = text
                        Layout.fillWidth: true
                    }
                    Button {
                        text: "Browse…"
                        onClicked: controller.browse_for_path(3)
                    }
                }

                // Analyze button (right-aligned)
                RowLayout {
                    Item { Layout.fillWidth: true }
                    Button {
                        text: "Analyze Only"
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
