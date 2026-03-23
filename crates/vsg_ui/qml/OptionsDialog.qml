// OptionsDialog.qml — 1:1 port of vsg_qt/options_dialog/ui.py + tabs.py
// Settings dialog with tabbed interface for all application settings.

import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import QtQuick.Dialogs
import com.vsg.ui 1.0

Dialog {
    id: root
    title: "Settings"
    width: 800
    height: 600
    modal: true
    standardButtons: Dialog.Ok | Dialog.Cancel

    property string settingsJson: "{}"
    property var settings: ({})

    OptionsLogic { id: logic }

    Component.onCompleted: {
        var json = logic.load_settings(settingsJson)
        settings = JSON.parse(json)
    }

    onAccepted: {
        // Collect all settings from tabs and save
        var allSettings = collectSettings()
        root.settingsJson = JSON.stringify(allSettings)
    }

    TabBar {
        id: tabBar
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right

        TabButton { text: "Storage" }
        TabButton { text: "Analysis" }
        TabButton { text: "Subtitles" }
        TabButton { text: "Chapters" }
        TabButton { text: "Muxing" }
        TabButton { text: "Stepping" }
        TabButton { text: "Neural/ML" }
        TabButton { text: "Logging" }
    }

    StackLayout {
        anchors.top: tabBar.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 8
        currentIndex: tabBar.currentIndex

        // Tab 0: Storage
        ScrollView {
            ColumnLayout {
                width: parent.width
                spacing: 6
                GroupBox {
                    title: "Directories"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsPathRow { label: "Output Folder"; settingKey: "output_folder" }
                        SettingsPathRow { label: "Temp Root"; settingKey: "temp_root" }
                        SettingsPathRow { label: "Logs Folder"; settingKey: "logs_folder" }
                        SettingsPathRow { label: "Fonts Directory"; settingKey: "fonts_directory" }
                    }
                }
            }
        }

        // Tab 1: Analysis
        ScrollView {
            ColumnLayout {
                width: parent.width
                spacing: 6
                GroupBox {
                    title: "Correlation"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCombo { label: "Method"; settingKey: "correlation_method"; values: ["SCC", "GCC-PHAT", "GCC-SCOT", "Whitened"] }
                        SettingsSpinBox { label: "Min Match %"; settingKey: "min_match_pct"; from: 0; to: 100; decimals: 1 }
                    }
                }
                GroupBox {
                    title: "Dense Sliding Window"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsSpinBox { label: "Window (s)"; settingKey: "dense_window_s"; from: 1; to: 60; decimals: 1 }
                        SettingsSpinBox { label: "Hop (s)"; settingKey: "dense_hop_s"; from: 0.1; to: 30; decimals: 1 }
                    }
                }
            }
        }

        // Tab 2: Subtitles
        ScrollView {
            ColumnLayout {
                width: parent.width
                spacing: 6
                GroupBox {
                    title: "Subtitle Sync"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCombo { label: "Sync Mode"; settingKey: "subtitle_sync_mode"; values: ["time-based", "video-verified"] }
                        SettingsCombo { label: "Rounding"; settingKey: "subtitle_rounding"; values: ["floor", "round", "ceil"] }
                    }
                }
            }
        }

        // Tab 3: Chapters
        ScrollView {
            ColumnLayout {
                width: parent.width
                spacing: 6
                GroupBox {
                    title: "Chapter Processing"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCheckBox { label: "Rename Chapters"; settingKey: "rename_chapters" }
                        SettingsCheckBox { label: "Snap Chapters to Keyframes"; settingKey: "snap_chapters" }
                        SettingsCombo { label: "Snap Mode"; settingKey: "snap_mode"; values: ["previous", "nearest"] }
                        SettingsSpinBox { label: "Snap Threshold (ms)"; settingKey: "snap_threshold_ms"; from: 0; to: 10000 }
                    }
                }
            }
        }

        // Tab 4: Muxing
        ScrollView {
            ColumnLayout {
                width: parent.width
                spacing: 6
                GroupBox {
                    title: "Merge Options"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCheckBox { label: "Apply Dialog Normalization Gain"; settingKey: "apply_dialog_norm_gain" }
                        SettingsCheckBox { label: "Disable Track Statistics Tags"; settingKey: "disable_track_statistics_tags" }
                        SettingsCheckBox { label: "Disable Header Compression"; settingKey: "disable_header_compression" }
                    }
                }
                GroupBox {
                    title: "Post-Merge"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCheckBox { label: "Normalize Timestamps"; settingKey: "post_mux_normalize_timestamps" }
                        SettingsCheckBox { label: "Strip Tags"; settingKey: "post_mux_strip_tags" }
                    }
                }
            }
        }

        // Tab 5: Stepping
        ScrollView {
            ColumnLayout {
                width: parent.width
                spacing: 6
                GroupBox {
                    title: "Stepping Correction"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCheckBox { label: "Enable Stepping Detection"; settingKey: "stepping_enabled" }
                        SettingsCheckBox { label: "Adjust Subtitles"; settingKey: "stepping_adjust_subtitles" }
                    }
                }
            }
        }

        // Tab 6: Neural/ML
        ScrollView {
            ColumnLayout {
                width: parent.width
                spacing: 6
                GroupBox {
                    title: "Video-Verified Settings"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCombo { label: "Method"; settingKey: "video_verified_method"; values: ["classic", "neural"] }
                        SettingsSpinBox { label: "Checkpoints"; settingKey: "video_verified_num_checkpoints"; from: 1; to: 100 }
                    }
                }
            }
        }

        // Tab 7: Logging
        ScrollView {
            ColumnLayout {
                width: parent.width
                spacing: 6
                GroupBox {
                    title: "Log Output"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCheckBox { label: "Compact Log"; settingKey: "log_compact" }
                        SettingsCheckBox { label: "Auto-scroll"; settingKey: "log_autoscroll" }
                        SettingsCheckBox { label: "Archive Logs"; settingKey: "archive_logs" }
                    }
                }
            }
        }
    }

    // ── Reusable settings components ──

    component SettingsCheckBox: CheckBox {
        property string settingKey: ""
        property string label: ""
        text: label
        checked: settings[settingKey] || false
        onCheckedChanged: settings[settingKey] = checked
    }

    component SettingsCombo: RowLayout {
        property string settingKey: ""
        property string label: ""
        property var values: []
        spacing: 8
        Label { text: parent.label; Layout.preferredWidth: 180 }
        ComboBox {
            model: parent.values
            currentIndex: Math.max(0, parent.values.indexOf(settings[parent.settingKey] || ""))
            onCurrentTextChanged: settings[parent.settingKey] = currentText
            Layout.fillWidth: true
        }
    }

    component SettingsSpinBox: RowLayout {
        property string settingKey: ""
        property string label: ""
        property real from: 0
        property real to: 100
        property int decimals: 0
        spacing: 8
        Label { text: parent.label; Layout.preferredWidth: 180 }
        SpinBox {
            from: parent.from * Math.pow(10, parent.decimals)
            to: parent.to * Math.pow(10, parent.decimals)
            value: (settings[parent.settingKey] || 0) * Math.pow(10, parent.decimals)
            stepSize: Math.pow(10, parent.decimals > 0 ? parent.decimals - 1 : 0)
            onValueChanged: settings[parent.settingKey] = value / Math.pow(10, parent.decimals)
            Layout.fillWidth: true
        }
    }

    component SettingsPathRow: RowLayout {
        property string settingKey: ""
        property string label: ""
        spacing: 8
        Label { text: parent.label; Layout.preferredWidth: 140 }
        TextField {
            text: settings[parent.settingKey] || ""
            onTextChanged: settings[parent.settingKey] = text
            Layout.fillWidth: true
        }
        Button {
            text: "Browse…"
            onClicked: {
                folderDialog.settingKey = parent.settingKey
                folderDialog.open()
            }
        }
    }

    FolderDialog {
        id: folderDialog
        property string settingKey: ""
        title: "Select Directory"
        onAccepted: {
            settings[settingKey] = selectedFolder.toString().replace("file://", "")
        }
    }

    function collectSettings() {
        return settings
    }
}
