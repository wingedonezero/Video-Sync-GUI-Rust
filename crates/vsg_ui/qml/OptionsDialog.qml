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
    width: 860
    height: 680
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
        var allSettings = collectSettings()
        root.settingsJson = JSON.stringify(allSettings)
    }

    // ── Folder / File dialogs ──

    FolderDialog {
        id: folderDialog
        property string settingKey: ""
        title: "Select Directory"
        onAccepted: {
            settings[settingKey] = selectedFolder.toString().replace("file://", "")
            settingsChanged()
        }
    }

    FileDialog {
        id: fileDialog
        property string settingKey: ""
        title: "Select File"
        onAccepted: {
            settings[settingKey] = selectedFile.toString().replace("file://", "")
            settingsChanged()
        }
    }

    signal settingsChanged()

    // ── Reusable settings components ──

    component SettingsCheckBox: CheckBox {
        property string settingKey: ""
        property string label: ""
        text: label
        checked: root.settings[settingKey] || false
        onCheckedChanged: root.settings[settingKey] = checked
        ToolTip.text: ""
        ToolTip.visible: hovered && ToolTip.text !== ""
        ToolTip.delay: 500
    }

    component SettingsCombo: RowLayout {
        property string settingKey: ""
        property string label: ""
        property var model: []
        spacing: 8
        Layout.fillWidth: true
        ToolTip.text: ""
        ToolTip.visible: hovered && ToolTip.text !== ""
        ToolTip.delay: 500

        Label {
            text: parent.label
            Layout.preferredWidth: 200
        }
        ComboBox {
            id: comboBox
            Layout.fillWidth: true
            model: {
                var m = parent.model
                if (m.length > 0 && typeof m[0] === "object") {
                    return m.map(function(item) { return item.text })
                }
                return m
            }
            Component.onCompleted: {
                var key = parent.settingKey
                var val = root.settings[key]
                var m = parent.model
                if (m.length > 0 && typeof m[0] === "object") {
                    for (var i = 0; i < m.length; i++) {
                        if (m[i].value === val || m[i].value == val) {
                            currentIndex = i
                            return
                        }
                    }
                } else {
                    var idx = m.indexOf(val)
                    if (idx >= 0) currentIndex = idx
                }
            }
            onCurrentIndexChanged: {
                var m = parent.model
                if (m.length > 0 && typeof m[0] === "object") {
                    if (currentIndex >= 0 && currentIndex < m.length)
                        root.settings[parent.settingKey] = m[currentIndex].value
                } else {
                    if (currentIndex >= 0 && currentIndex < m.length)
                        root.settings[parent.settingKey] = m[currentIndex]
                }
            }
        }
    }

    component SettingsSpinBox: RowLayout {
        property string settingKey: ""
        property string label: ""
        property int from: 0
        property int to: 100
        property int stepSize: 1
        property string suffix: ""
        spacing: 8
        Layout.fillWidth: true
        ToolTip.text: ""
        ToolTip.visible: hovered && ToolTip.text !== ""
        ToolTip.delay: 500

        Label {
            text: parent.label
            Layout.preferredWidth: 200
        }
        SpinBox {
            id: spinBox
            from: parent.from
            to: parent.to
            stepSize: parent.stepSize
            value: root.settings[parent.settingKey] || 0
            onValueChanged: root.settings[parent.settingKey] = value
            Layout.fillWidth: true
            textFromValue: function(value, locale) {
                return value + parent.suffix
            }
            valueFromText: function(text, locale) {
                return parseInt(text)
            }
        }
    }

    component SettingsDoubleSpinBox: RowLayout {
        property string settingKey: ""
        property string label: ""
        property real from: 0.0
        property real to: 100.0
        property real stepSize: 0.1
        property int decimals: 1
        property string suffix: ""
        spacing: 8
        Layout.fillWidth: true
        ToolTip.text: ""
        ToolTip.visible: hovered && ToolTip.text !== ""
        ToolTip.delay: 500

        readonly property int multiplier: Math.pow(10, decimals)

        Label {
            text: parent.label
            Layout.preferredWidth: 200
        }
        SpinBox {
            id: dblSpinBox
            from: Math.round(parent.from * parent.multiplier)
            to: Math.round(parent.to * parent.multiplier)
            stepSize: Math.max(1, Math.round(parent.stepSize * parent.multiplier))
            value: Math.round((root.settings[parent.settingKey] || 0) * parent.multiplier)
            onValueChanged: root.settings[parent.settingKey] = value / parent.multiplier
            Layout.fillWidth: true
            textFromValue: function(value, locale) {
                var real = value / parent.multiplier
                return real.toFixed(parent.decimals) + parent.suffix
            }
            valueFromText: function(text, locale) {
                return Math.round(parseFloat(text) * parent.multiplier)
            }
        }
    }

    component SettingsTextField: RowLayout {
        property string settingKey: ""
        property string label: ""
        property string placeholderText: ""
        spacing: 8
        Layout.fillWidth: true
        ToolTip.text: ""
        ToolTip.visible: hovered && ToolTip.text !== ""
        ToolTip.delay: 500

        Label {
            text: parent.label
            Layout.preferredWidth: 200
        }
        TextField {
            text: root.settings[parent.settingKey] || ""
            placeholderText: parent.placeholderText
            onTextChanged: root.settings[parent.settingKey] = text
            Layout.fillWidth: true
        }
    }

    component SettingsPathRow: RowLayout {
        property string settingKey: ""
        property string label: ""
        spacing: 8
        Layout.fillWidth: true
        ToolTip.text: ""
        ToolTip.visible: hovered && ToolTip.text !== ""
        ToolTip.delay: 500

        Label {
            text: parent.label
            Layout.preferredWidth: 200
        }
        TextField {
            text: root.settings[parent.settingKey] || ""
            onTextChanged: root.settings[parent.settingKey] = text
            Layout.fillWidth: true
        }
        Button {
            text: "Browse\u2026"
            onClicked: {
                folderDialog.settingKey = parent.settingKey
                folderDialog.open()
            }
        }
    }

    component SettingsFileRow: RowLayout {
        property string settingKey: ""
        property string label: ""
        spacing: 8
        Layout.fillWidth: true
        ToolTip.text: ""
        ToolTip.visible: hovered && ToolTip.text !== ""
        ToolTip.delay: 500

        Label {
            text: parent.label
            Layout.preferredWidth: 200
        }
        TextField {
            text: root.settings[parent.settingKey] || ""
            onTextChanged: root.settings[parent.settingKey] = text
            Layout.fillWidth: true
        }
        Button {
            text: "Browse\u2026"
            onClicked: {
                fileDialog.settingKey = parent.settingKey
                fileDialog.open()
            }
        }
    }

    // Bold section header label
    component SectionHeader: Label {
        font.bold: true
        font.pixelSize: 14
        topPadding: 8
        bottomPadding: 4
    }

    // ── Tab bar ──

    TabBar {
        id: tabBar
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right

        TabButton { text: "Storage && Tools" }
        TabButton { text: "Analysis" }
        TabButton { text: "Stepping Correction" }
        TabButton { text: "Subtitles" }
        TabButton { text: "Chapters" }
        TabButton { text: "OCR" }
        TabButton { text: "Merge Behavior" }
        TabButton { text: "Logging" }
    }

    // ── Tab content ──

    StackLayout {
        anchors.top: tabBar.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 8
        currentIndex: tabBar.currentIndex

        // ════════════════════════════════════════════════════════════════
        // Tab 0: Storage & Tools
        // ════════════════════════════════════════════════════════════════
        ScrollView {
            ColumnLayout {
                width: parent.width
                spacing: 6

                GroupBox {
                    title: "Paths"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsPathRow {
                            label: "Output Directory:"
                            settingKey: "output_folder"
                            ToolTip.text: "The default directory where final merged files will be saved."
                        }
                        SettingsPathRow {
                            label: "Temporary Directory:"
                            settingKey: "temp_root"
                            ToolTip.text: "The root directory for storing temporary files during processing."
                        }
                        SettingsPathRow {
                            label: "Reports Directory:"
                            settingKey: "logs_folder"
                            ToolTip.text: "Directory for batch report files."
                        }
                        SettingsFileRow {
                            label: "VideoDiff Path (optional):"
                            settingKey: "videodiff_path"
                            ToolTip.text: "Optional. Full path to 'videodiff' executable."
                        }
                        SettingsFileRow {
                            label: "OCR Custom Wordlist:"
                            settingKey: "ocr_custom_wordlist_path"
                            ToolTip.text: "Path to custom wordlist file for OCR."
                        }
                    }
                }

                GroupBox {
                    title: "Config Maintenance"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        Button {
                            text: "Remove Invalid Config Entries"
                            onClicked: logic.remove_invalid_config()
                        }
                    }
                }
            }
        }

        // ════════════════════════════════════════════════════════════════
        // Tab 1: Analysis
        // ════════════════════════════════════════════════════════════════
        ScrollView {
            ColumnLayout {
                width: parent.width
                spacing: 6

                // ── Step 1: Audio Pre-Processing ──
                GroupBox {
                    title: "Step 1: Audio Pre-Processing"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCombo {
                            label: "Source Separation Mode:"
                            settingKey: "source_separation_mode"
                            model: [
                                {text: "None (Use Original Audio)", value: "none"},
                                {text: "Instrumental (No Vocals)", value: "instrumental"},
                                {text: "Vocals Only", value: "vocals"}
                            ]
                            ToolTip.text: "Audio source separation mode for pre-processing."
                        }
                        SettingsTextField {
                            label: "Source Separation Model:"
                            settingKey: "source_separation_model"
                            placeholderText: "Model name"
                            ToolTip.text: "Name of the source separation model to use."
                        }
                        SettingsPathRow {
                            label: "Model Directory:"
                            settingKey: "source_separation_model_dir"
                            ToolTip.text: "Directory where source separation models are stored."
                        }
                        Button {
                            text: "Manage Models\u2026"
                            ToolTip.text: "Open the model manager to download or remove source separation models."
                            ToolTip.visible: hovered
                            ToolTip.delay: 500
                        }
                        SettingsCombo {
                            label: "Filtering Method:"
                            settingKey: "filtering_method"
                            model: ["None", "Low-Pass Filter", "Dialogue Band-Pass Filter"]
                            ToolTip.text: "Audio filtering method applied before analysis."
                        }
                        SettingsSpinBox {
                            label: "Audio Bandlimit:"
                            settingKey: "audio_bandlimit_hz"
                            from: 0; to: 22000
                            suffix: " Hz"
                            visible: root.settings.filtering_method === "Low-Pass Filter"
                            ToolTip.text: "Low-pass filter cutoff frequency in Hz."
                        }
                    }
                }

                // ── Step 2: Core Analysis Engine ──
                GroupBox {
                    title: "Step 2: Core Analysis Engine"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCombo {
                            label: "Correlation Method:"
                            settingKey: "correlation_method"
                            model: [
                                "Standard Correlation (SCC)",
                                "Phase Correlation (GCC-PHAT)",
                                "Onset Detection",
                                "GCC-SCOT",
                                "Whitened Cross-Correlation",
                                "Spectrogram Correlation",
                                "VideoDiff"
                            ]
                            ToolTip.text: "Primary correlation method for audio analysis."
                        }
                        SettingsCombo {
                            label: "Correlation (Separated):"
                            settingKey: "correlation_method_source_separated"
                            model: [
                                "Standard Correlation (SCC)",
                                "Phase Correlation (GCC-PHAT)",
                                "Onset Detection",
                                "GCC-SCOT",
                                "Whitened Cross-Correlation",
                                "Spectrogram Correlation"
                            ]
                            ToolTip.text: "Correlation method used when source separation is active."
                        }
                        SettingsDoubleSpinBox {
                            label: "Dense Window:"
                            settingKey: "dense_window_s"
                            from: 2.0; to: 60.0; decimals: 1
                            suffix: " s"
                            ToolTip.text: "Duration of each sliding window in seconds."
                        }
                        SettingsDoubleSpinBox {
                            label: "Dense Hop:"
                            settingKey: "dense_hop_s"
                            from: 0.5; to: 30.0; decimals: 1
                            suffix: " s"
                            ToolTip.text: "Hop size between consecutive windows in seconds."
                        }
                        SettingsDoubleSpinBox {
                            label: "Silence Threshold:"
                            settingKey: "dense_silence_threshold_db"
                            from: -120.0; to: 0.0; decimals: 1
                            suffix: " dB"
                            ToolTip.text: "Windows below this dB threshold are treated as silence and skipped."
                        }
                        SettingsDoubleSpinBox {
                            label: "Outlier Threshold:"
                            settingKey: "dense_outlier_threshold_ms"
                            from: 5.0; to: 500.0; decimals: 1
                            suffix: " ms"
                            ToolTip.text: "Delay values deviating by more than this are flagged as outliers."
                        }
                        SettingsDoubleSpinBox {
                            label: "Min Match %:"
                            settingKey: "min_match_pct"
                            from: 0.1; to: 100.0; decimals: 1; stepSize: 1.0
                            ToolTip.text: "Minimum percentage of matching windows required for a valid result."
                        }
                        SettingsCombo {
                            label: "Delay Selection Mode:"
                            settingKey: "delay_selection_mode"
                            model: [
                                "Mode (Most Common)",
                                "Mode (Clustered)",
                                "Mode (Early Cluster)",
                                "First Stable",
                                "Average"
                            ]
                            ToolTip.text: "Method for selecting the final delay value from matching windows."
                        }
                        SettingsCombo {
                            label: "Delay Mode (Separated):"
                            settingKey: "delay_selection_mode_source_separated"
                            model: [
                                "Mode (Most Common)",
                                "Mode (Clustered)",
                                "Mode (Early Cluster)",
                                "First Stable",
                                "Average"
                            ]
                            ToolTip.text: "Delay selection mode used when source separation is active."
                        }
                        SettingsDoubleSpinBox {
                            label: "First Stable Early %:"
                            settingKey: "first_stable_early_pct"
                            from: 5.0; to: 75.0; decimals: 1
                            suffix: " %"
                            visible: root.settings.delay_selection_mode === "First Stable"
                            ToolTip.text: "Percentage of the timeline to consider as 'early' for First Stable mode."
                        }
                        SettingsDoubleSpinBox {
                            label: "Early Cluster Early %:"
                            settingKey: "early_cluster_early_pct"
                            from: 5.0; to: 75.0; decimals: 1
                            suffix: " %"
                            visible: root.settings.delay_selection_mode === "Mode (Early Cluster)"
                            ToolTip.text: "Percentage of the timeline considered 'early' for Early Cluster mode."
                        }
                        SettingsDoubleSpinBox {
                            label: "Early Cluster Min Presence %:"
                            settingKey: "early_cluster_min_presence_pct"
                            from: 1.0; to: 50.0; decimals: 1
                            suffix: " %"
                            visible: root.settings.delay_selection_mode === "Mode (Early Cluster)"
                            ToolTip.text: "Minimum presence percentage required for an early cluster to be valid."
                        }
                    }
                }

                // ── Multi-Correlation Comparison (Analyze Only) ──
                GroupBox {
                    title: "Multi-Correlation Comparison (Analyze Only)"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCheckBox {
                            label: "Enable Multi-Correlation Comparison"
                            settingKey: "multi_correlation_enabled"
                            ToolTip.text: "Run multiple correlation methods and compare their results."
                        }
                        ColumnLayout {
                            visible: root.settings.multi_correlation_enabled || false
                            Layout.fillWidth: true
                            Layout.leftMargin: 20
                            SettingsCheckBox {
                                label: "Standard Correlation (SCC)"
                                settingKey: "multi_corr_scc"
                                ToolTip.text: "Include Standard Correlation in multi-method comparison."
                            }
                            SettingsCheckBox {
                                label: "Phase Correlation (GCC-PHAT)"
                                settingKey: "multi_corr_gcc_phat"
                                ToolTip.text: "Include Phase Correlation in multi-method comparison."
                            }
                            SettingsCheckBox {
                                label: "Onset Detection"
                                settingKey: "multi_corr_onset"
                                ToolTip.text: "Include Onset Detection in multi-method comparison."
                            }
                            SettingsCheckBox {
                                label: "GCC-SCOT"
                                settingKey: "multi_corr_gcc_scot"
                                ToolTip.text: "Include GCC-SCOT in multi-method comparison."
                            }
                            SettingsCheckBox {
                                label: "Whitened Cross-Correlation"
                                settingKey: "multi_corr_gcc_whiten"
                                ToolTip.text: "Include Whitened Cross-Correlation in multi-method comparison."
                            }
                            SettingsCheckBox {
                                label: "Spectrogram Correlation"
                                settingKey: "multi_corr_spectrogram"
                                ToolTip.text: "Include Spectrogram Correlation in multi-method comparison."
                            }
                        }
                    }
                }

                // ── Step 3: Advanced Filtering & Scan Controls ──
                GroupBox {
                    title: "Step 3: Advanced Filtering & Scan Controls"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsDoubleSpinBox {
                            label: "Scan Start %:"
                            settingKey: "scan_start_percentage"
                            from: 0.0; to: 99.0; decimals: 1
                            suffix: " %"
                            ToolTip.text: "Start scanning from this percentage of the file."
                        }
                        SettingsDoubleSpinBox {
                            label: "Scan End %:"
                            settingKey: "scan_end_percentage"
                            from: 1.0; to: 100.0; decimals: 1
                            suffix: " %"
                            ToolTip.text: "Stop scanning at this percentage of the file."
                        }
                        SettingsDoubleSpinBox {
                            label: "Bandpass Low Cut:"
                            settingKey: "filter_bandpass_lowcut_hz"
                            from: 20.0; to: 10000.0; decimals: 1
                            suffix: " Hz"
                            ToolTip.text: "Low cutoff frequency for the bandpass filter."
                        }
                        SettingsDoubleSpinBox {
                            label: "Bandpass High Cut:"
                            settingKey: "filter_bandpass_highcut_hz"
                            from: 100.0; to: 22000.0; decimals: 1
                            suffix: " Hz"
                            ToolTip.text: "High cutoff frequency for the bandpass filter."
                        }
                        SettingsSpinBox {
                            label: "Bandpass Order:"
                            settingKey: "filter_bandpass_order"
                            from: 1; to: 10
                            ToolTip.text: "Order of the Butterworth bandpass filter."
                        }
                        SettingsSpinBox {
                            label: "Low-Pass Taps:"
                            settingKey: "filter_lowpass_taps"
                            from: 11; to: 501; stepSize: 2
                            ToolTip.text: "Number of FIR filter taps for low-pass filtering (odd values only)."
                        }
                    }
                }

                // ── Step 4: Audio Track Selection ──
                GroupBox {
                    title: "Step 4: Audio Track Selection"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsTextField {
                            label: "Source 1 Language:"
                            settingKey: "analysis_lang_source1"
                            placeholderText: "e.g., eng"
                            ToolTip.text: "Preferred audio language for Source 1 (reference). Use ISO 639-2/B codes."
                        }
                        SettingsTextField {
                            label: "Other Sources Language:"
                            settingKey: "analysis_lang_others"
                            placeholderText: "e.g., jpn"
                            ToolTip.text: "Preferred audio language for other sources. Use ISO 639-2/B codes."
                        }
                    }
                }

                // ── Step 5: Timing Sync Mode ──
                GroupBox {
                    title: "Step 5: Timing Sync Mode"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCombo {
                            label: "Sync Mode:"
                            settingKey: "sync_mode"
                            model: ["positive_only", "allow_negative"]
                            ToolTip.text: "Whether to allow negative delay values (source plays before reference)."
                        }
                    }
                }

                // ── Step 6: Advanced Tweaks & Diagnostics ──
                GroupBox {
                    title: "Step 6: Advanced Tweaks & Diagnostics"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCheckBox {
                            label: "Use High-Quality Resampling (SoXR)"
                            settingKey: "use_soxr"
                            ToolTip.text: "Use the SoX Resampler library for higher quality audio resampling."
                        }
                        SettingsCheckBox {
                            label: "Enable Sub-Sample Peak Fitting (SCC only)"
                            settingKey: "audio_peak_fit"
                            ToolTip.text: "Refine peak location with sub-sample interpolation. Only applies to SCC."
                        }
                        SettingsCheckBox {
                            label: "Log Audio Drift Metric"
                            settingKey: "log_audio_drift"
                            ToolTip.text: "Log additional drift metrics for diagnosing audio sync issues."
                        }
                    }
                }
            }
        }

        // ════════════════════════════════════════════════════════════════
        // Tab 2: Stepping Correction
        // ════════════════════════════════════════════════════════════════
        ScrollView {
            ColumnLayout {
                width: parent.width
                spacing: 6

                GroupBox {
                    title: "Stepping Correction"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent

                        // ── Detection Settings ──
                        SectionHeader { text: "Detection Settings" }

                        SettingsCheckBox {
                            label: "Enable stepping correction"
                            settingKey: "stepping_enabled"
                            ToolTip.text: "Enable automatic detection and correction of stepped audio segments."
                        }
                        SettingsDoubleSpinBox {
                            label: "DBSCAN Epsilon:"
                            settingKey: "detection_dbscan_epsilon_ms"
                            from: 5.0; to: 100.0; decimals: 1
                            suffix: " ms"
                            ToolTip.text: "Maximum distance between points in a DBSCAN cluster."
                        }
                        SettingsDoubleSpinBox {
                            label: "DBSCAN Min Samples %:"
                            settingKey: "detection_dbscan_min_samples_pct"
                            from: 0.5; to: 10.0; decimals: 1
                            suffix: "%"
                            ToolTip.text: "Minimum number of samples as a percentage for DBSCAN core points."
                        }
                        SettingsSpinBox {
                            label: "Triage Std Dev:"
                            settingKey: "stepping_triage_std_dev_ms"
                            from: 10; to: 200
                            suffix: " ms"
                            ToolTip.text: "Standard deviation threshold in ms for triaging stepped vs uniform delays."
                        }
                        SettingsDoubleSpinBox {
                            label: "Drift R\u00b2 Threshold:"
                            settingKey: "drift_detection_r2_threshold"
                            from: 0.5; to: 1.0; decimals: 2
                            ToolTip.text: "R-squared threshold for detecting linear drift in delay values."
                        }
                        SettingsDoubleSpinBox {
                            label: "Drift R\u00b2 (Lossless):"
                            settingKey: "drift_detection_r2_threshold_lossless"
                            from: 0.5; to: 1.0; decimals: 2
                            ToolTip.text: "R-squared threshold for drift detection in lossless audio sources."
                        }
                        SettingsDoubleSpinBox {
                            label: "Drift Slope (Lossy):"
                            settingKey: "drift_detection_slope_threshold_lossy"
                            from: 0.1; to: 5.0; decimals: 1
                            suffix: " ms/s"
                            ToolTip.text: "Maximum acceptable drift slope for lossy audio sources."
                        }
                        SettingsDoubleSpinBox {
                            label: "Drift Slope (Lossless):"
                            settingKey: "drift_detection_slope_threshold_lossless"
                            from: 0.1; to: 5.0; decimals: 1
                            suffix: " ms/s"
                            ToolTip.text: "Maximum acceptable drift slope for lossless audio sources."
                        }

                        // ── Quality Validation ──
                        SectionHeader { text: "Quality Validation" }

                        SettingsCombo {
                            label: "Correction Mode:"
                            settingKey: "stepping_correction_mode"
                            model: ["full", "filtered", "strict", "disabled"]
                            ToolTip.text: "How aggressively to apply stepping correction."
                        }
                        SettingsCombo {
                            label: "Quality Mode:"
                            settingKey: "stepping_quality_mode"
                            model: ["strict", "normal", "lenient", "custom"]
                            ToolTip.text: "Preset quality thresholds for stepping correction validation."
                        }
                        SettingsCombo {
                            label: "Filtered Fallback:"
                            settingKey: "stepping_filtered_fallback"
                            model: ["nearest", "interpolate", "uniform", "skip", "reject"]
                            ToolTip.text: "Fallback strategy when filtered stepping mode cannot find a good match."
                        }
                        SettingsDoubleSpinBox {
                            label: "Min Cluster %:"
                            settingKey: "stepping_min_cluster_percentage"
                            from: 1.0; to: 50.0; decimals: 1
                            suffix: " %"
                            ToolTip.text: "Minimum percentage of windows a cluster must contain to be valid."
                        }
                        SettingsDoubleSpinBox {
                            label: "Min Cluster Duration:"
                            settingKey: "stepping_min_cluster_duration_s"
                            from: 0.0; to: 120.0; decimals: 1
                            suffix: " s"
                            ToolTip.text: "Minimum duration in seconds for a cluster to be accepted."
                        }
                        SettingsDoubleSpinBox {
                            label: "Min Match Quality %:"
                            settingKey: "stepping_min_match_quality_pct"
                            from: 50.0; to: 100.0; decimals: 1
                            suffix: " %"
                            ToolTip.text: "Minimum match quality percentage required for stepping correction."
                        }
                        SettingsSpinBox {
                            label: "Min Total Clusters:"
                            settingKey: "stepping_min_total_clusters"
                            from: 1; to: 10
                            ToolTip.text: "Minimum number of detected clusters required to apply stepping correction."
                        }

                        // ── Boundary Refinement ──
                        SectionHeader { text: "Boundary Refinement" }

                        SettingsCheckBox {
                            label: "Enable speech protection (VAD)"
                            settingKey: "stepping_vad_enabled"
                            ToolTip.text: "Use Voice Activity Detection to avoid placing boundaries during speech."
                        }
                        SettingsSpinBox {
                            label: "VAD Aggressiveness:"
                            settingKey: "stepping_vad_aggressiveness"
                            from: 0; to: 3
                            ToolTip.text: "VAD aggressiveness level (0=least, 3=most aggressive filtering)."
                        }
                        SettingsDoubleSpinBox {
                            label: "Silence Search Window:"
                            settingKey: "stepping_silence_search_window_s"
                            from: 0.5; to: 15.0; decimals: 1
                            suffix: " s"
                            ToolTip.text: "Window size in seconds to search for silence around boundaries."
                        }
                        SettingsDoubleSpinBox {
                            label: "Silence Threshold:"
                            settingKey: "stepping_silence_threshold_db"
                            from: -60.0; to: -20.0; decimals: 1
                            suffix: " dB"
                            ToolTip.text: "Audio level below which is considered silence."
                        }
                        SettingsDoubleSpinBox {
                            label: "Silence Min Duration:"
                            settingKey: "stepping_silence_min_duration_ms"
                            from: 50.0; to: 1000.0; decimals: 0
                            suffix: " ms"
                            ToolTip.text: "Minimum duration of silence to qualify as a boundary candidate."
                        }
                        SettingsSpinBox {
                            label: "Fusion Weight (Silence):"
                            settingKey: "stepping_fusion_weight_silence"
                            from: 0; to: 20
                            ToolTip.text: "Weight given to silence proximity in boundary scoring."
                        }
                        SettingsSpinBox {
                            label: "Fusion Weight (Duration):"
                            settingKey: "stepping_fusion_weight_duration"
                            from: 0; to: 20
                            ToolTip.text: "Weight given to segment duration balance in boundary scoring."
                        }
                        SettingsCheckBox {
                            label: "Avoid transients when picking splice points"
                            settingKey: "stepping_transient_detection_enabled"
                            ToolTip.text: "Detect audio transients and avoid placing splice points on them."
                        }
                        SettingsDoubleSpinBox {
                            label: "Transient Threshold:"
                            settingKey: "stepping_transient_threshold"
                            from: 3.0; to: 20.0; decimals: 1
                            suffix: " dB"
                            ToolTip.text: "dB threshold above which a spectral change is considered a transient."
                        }
                        SettingsCheckBox {
                            label: "Snap boundaries to video keyframes"
                            settingKey: "stepping_snap_to_video_frames"
                            ToolTip.text: "Align stepping boundaries to the nearest video keyframe."
                        }
                        SettingsDoubleSpinBox {
                            label: "Video Snap Max Offset:"
                            settingKey: "stepping_video_snap_max_offset_s"
                            from: 0.1; to: 10.0; decimals: 1
                            suffix: " s"
                            ToolTip.text: "Maximum offset in seconds when snapping to video keyframes."
                        }

                        // ── Quality Assurance ──
                        SectionHeader { text: "Quality Assurance" }

                        SettingsDoubleSpinBox {
                            label: "QA Threshold:"
                            settingKey: "stepping_qa_threshold"
                            from: 50.0; to: 99.0; decimals: 1
                            suffix: "%"
                            ToolTip.text: "Overall quality threshold percentage for stepping correction results."
                        }
                        SettingsDoubleSpinBox {
                            label: "QA Min Accepted %:"
                            settingKey: "stepping_qa_min_accepted_pct"
                            from: 50.0; to: 100.0; decimals: 1
                            suffix: "%"
                            ToolTip.text: "Minimum percentage of segments that must pass QA."
                        }

                        // ── Audio Processing ──
                        SectionHeader { text: "Audio Processing" }

                        SettingsCombo {
                            label: "Resample Engine:"
                            settingKey: "segment_resample_engine"
                            model: ["aresample", "atempo", "rubberband"]
                            ToolTip.text: "FFmpeg audio filter used for time-stretching corrected segments."
                        }
                        GroupBox {
                            title: "Rubberband Settings"
                            Layout.fillWidth: true
                            visible: root.settings.segment_resample_engine === "rubberband"
                            ColumnLayout {
                                anchors.fill: parent
                                SettingsCheckBox {
                                    label: "Enable Pitch Correction"
                                    settingKey: "segment_rb_pitch_correct"
                                    ToolTip.text: "Correct pitch shift introduced by time-stretching."
                                }
                                SettingsCombo {
                                    label: "Transients:"
                                    settingKey: "segment_rb_transients"
                                    model: ["crisp", "mixed", "smooth"]
                                    ToolTip.text: "Rubberband transient handling mode."
                                }
                                SettingsCheckBox {
                                    label: "Enable Phase Smoothing"
                                    settingKey: "segment_rb_smoother"
                                    ToolTip.text: "Enable phase smoothing for more natural sounding results."
                                }
                                SettingsCheckBox {
                                    label: "Enable High-Quality Pitch Algorithm"
                                    settingKey: "segment_rb_pitchq"
                                    ToolTip.text: "Use the higher quality pitch shifting algorithm."
                                }
                            }
                        }

                        // ── Track Naming ──
                        SectionHeader { text: "Track Naming" }

                        SettingsTextField {
                            label: "Corrected Track Label:"
                            settingKey: "stepping_corrected_track_label"
                            placeholderText: "Leave empty for no label"
                            ToolTip.text: "Label applied to the stepping-corrected audio track in the output."
                        }
                        SettingsTextField {
                            label: "Preserved Track Label:"
                            settingKey: "stepping_preserved_track_label"
                            placeholderText: "Leave empty for no label"
                            ToolTip.text: "Label applied to the preserved (original) audio track in the output."
                        }

                        // ── Subtitle Adjustment ──
                        SectionHeader { text: "Subtitle Adjustment" }

                        SettingsCheckBox {
                            label: "Adjust subtitle timestamps for stepped sources"
                            settingKey: "stepping_adjust_subtitles"
                            ToolTip.text: "Apply segment-level timing corrections to subtitle tracks."
                        }
                        SettingsCheckBox {
                            label: "Apply stepping to subtitles when no audio is merged"
                            settingKey: "stepping_adjust_subtitles_no_audio"
                            ToolTip.text: "Apply stepping corrections to subtitles even when audio is not being merged."
                        }
                        SettingsCombo {
                            label: "Boundary Mode:"
                            settingKey: "stepping_boundary_mode"
                            model: ["start", "majority", "midpoint"]
                            ToolTip.text: "How to assign subtitles to segments when they span a boundary."
                        }

                        // ── Diagnostics ──
                        SectionHeader { text: "Diagnostics" }

                        SettingsCheckBox {
                            label: "Enable detailed cluster diagnostics"
                            settingKey: "stepping_diagnostics_verbose"
                            ToolTip.text: "Log detailed diagnostic information about stepping clusters."
                        }
                    }
                }
            }
        }

        // ════════════════════════════════════════════════════════════════
        // Tab 3: Subtitles
        // ════════════════════════════════════════════════════════════════
        ScrollView {
            ColumnLayout {
                width: parent.width
                spacing: 6

                GroupBox {
                    title: "Sync Mode"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCombo {
                            label: "Subtitle Sync Mode:"
                            settingKey: "subtitle_sync_mode"
                            model: ["time-based", "video-verified"]
                            ToolTip.text: "Method for synchronizing subtitles: time-based uses audio delay, video-verified uses frame matching."
                        }
                    }
                }

                GroupBox {
                    title: "Output Settings"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCombo {
                            label: "Subtitle Rounding:"
                            settingKey: "subtitle_rounding"
                            model: ["floor", "round", "ceil"]
                            ToolTip.text: "Rounding method for subtitle timestamps after applying delay."
                        }
                    }
                }

                GroupBox {
                    title: "Time-Based Settings"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCheckBox {
                            label: "Apply delay directly to subtitle file"
                            settingKey: "time_based_use_raw_values"
                            enabled: root.settings.subtitle_sync_mode === "time-based"
                            ToolTip.text: "Use the raw delay value directly instead of rounding to the nearest frame."
                        }
                    }
                }

                GroupBox {
                    title: "Video-Verified Settings"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCombo {
                            label: "Verification Method:"
                            settingKey: "video_verified_method"
                            model: [
                                {text: "Classic (phash/SSIM/MSE)", value: "classic"},
                                {text: "Neural (ISC Features)", value: "neural"}
                            ]
                            ToolTip.text: "Frame matching method for video-verified subtitle sync."
                        }

                        // ── Classic settings ──
                        ColumnLayout {
                            visible: root.settings.subtitle_sync_mode === "video-verified" && root.settings.video_verified_method === "classic"
                            Layout.fillWidth: true
                            SettingsCombo {
                                label: "Hash Algorithm:"
                                settingKey: "frame_hash_algorithm"
                                model: ["dhash", "phash", "average_hash", "whash"]
                                ToolTip.text: "Perceptual hash algorithm for frame comparison."
                            }
                            SettingsCombo {
                                label: "Hash Size:"
                                settingKey: "frame_hash_size"
                                model: [
                                    {text: "4", value: 4},
                                    {text: "8", value: 8},
                                    {text: "16", value: 16}
                                ]
                                ToolTip.text: "Size of the perceptual hash (larger = more sensitive)."
                            }
                            SettingsSpinBox {
                                label: "Hash Threshold:"
                                settingKey: "frame_hash_threshold"
                                from: 0; to: 50
                                ToolTip.text: "Maximum Hamming distance for frames to be considered matching."
                            }
                            SettingsSpinBox {
                                label: "Window Radius:"
                                settingKey: "frame_window_radius"
                                from: 1; to: 10
                                ToolTip.text: "Number of frames to check around the expected position."
                            }
                            SettingsCombo {
                                label: "Comparison Method:"
                                settingKey: "frame_comparison_method"
                                model: ["hash", "ssim", "mse"]
                                ToolTip.text: "Method for comparing video frames."
                            }
                            SettingsSpinBox {
                                label: "SSIM Threshold:"
                                settingKey: "frame_ssim_threshold"
                                from: 1; to: 50
                                ToolTip.text: "Minimum SSIM score (scaled) for frames to be considered matching."
                            }
                            SettingsSpinBox {
                                label: "Zero-Check Frames:"
                                settingKey: "video_verified_zero_check_frames"
                                from: 1; to: 10
                                ToolTip.text: "Number of frames to check when verifying zero-offset alignment."
                            }
                            SettingsDoubleSpinBox {
                                label: "Min Quality Advantage:"
                                settingKey: "video_verified_min_quality_advantage"
                                from: 0.0; to: 0.5; decimals: 2; stepSize: 0.05
                                ToolTip.text: "Minimum quality improvement needed to prefer a non-zero offset."
                            }
                            SettingsSpinBox {
                                label: "Num Checkpoints:"
                                settingKey: "video_verified_num_checkpoints"
                                from: 3; to: 10
                                ToolTip.text: "Number of checkpoint positions to sample across the video."
                            }
                            SettingsSpinBox {
                                label: "Search Range (frames):"
                                settingKey: "video_verified_search_range_frames"
                                from: 1; to: 10
                                ToolTip.text: "Number of frames to search in each direction at checkpoints."
                            }
                            SettingsSpinBox {
                                label: "Sequence Length:"
                                settingKey: "video_verified_sequence_length"
                                from: 5; to: 30
                                ToolTip.text: "Number of consecutive frames in each verification sequence."
                            }
                            SettingsCheckBox {
                                label: "Use PTS Precision"
                                settingKey: "video_verified_use_pts_precision"
                                ToolTip.text: "Use presentation timestamps for frame-accurate seeking."
                            }
                        }

                        // ── Shared video-verified settings ──
                        ColumnLayout {
                            visible: root.settings.subtitle_sync_mode === "video-verified"
                            Layout.fillWidth: true
                            SettingsCheckBox {
                                label: "Frame Alignment Audit"
                                settingKey: "video_verified_frame_audit"
                                ToolTip.text: "Run a post-verification audit of frame alignment accuracy."
                            }
                            SettingsCheckBox {
                                label: "Visual Frame Verify"
                                settingKey: "video_verified_visual_verify"
                                ToolTip.text: "Generate visual comparison images for frame verification results."
                            }
                        }

                        // ── Neural settings ──
                        ColumnLayout {
                            visible: root.settings.subtitle_sync_mode === "video-verified" && root.settings.video_verified_method === "neural"
                            Layout.fillWidth: true
                            SettingsSpinBox {
                                label: "Window (seconds):"
                                settingKey: "neural_window_seconds"
                                from: 5; to: 30
                                ToolTip.text: "Duration of the sliding window for neural feature extraction."
                            }
                            SettingsSpinBox {
                                label: "Slide Range (seconds):"
                                settingKey: "neural_slide_range_seconds"
                                from: 1; to: 15
                                ToolTip.text: "Range in seconds to slide the window for best match."
                            }
                            SettingsSpinBox {
                                label: "Num Positions:"
                                settingKey: "neural_num_positions"
                                from: 3; to: 15
                                ToolTip.text: "Number of positions to evaluate within the slide range."
                            }
                            SettingsSpinBox {
                                label: "Batch Size:"
                                settingKey: "neural_batch_size"
                                from: 1; to: 128
                                ToolTip.text: "Batch size for neural network inference."
                            }
                            SettingsCheckBox {
                                label: "Run in Subprocess"
                                settingKey: "neural_run_in_subprocess"
                                ToolTip.text: "Run neural inference in a separate process to avoid GPU memory issues."
                            }
                            SettingsCheckBox {
                                label: "Neural Debug Report"
                                settingKey: "neural_debug_report"
                                ToolTip.text: "Generate a detailed debug report for neural verification."
                            }
                        }
                    }
                }
            }
        }

        // ════════════════════════════════════════════════════════════════
        // Tab 4: Chapters
        // ════════════════════════════════════════════════════════════════
        ScrollView {
            ColumnLayout {
                width: parent.width
                spacing: 6

                GroupBox {
                    title: "Chapter Processing"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCheckBox {
                            label: "Rename to 'Chapter NN'"
                            settingKey: "rename_chapters"
                            ToolTip.text: "Rename all chapter titles to a standardized 'Chapter NN' format."
                        }
                        SettingsCheckBox {
                            label: "Snap chapter timestamps to nearest keyframe"
                            settingKey: "snap_chapters"
                            ToolTip.text: "Align chapter timestamps to the nearest video keyframe for clean seeking."
                        }
                        SettingsCombo {
                            label: "Snap Mode:"
                            settingKey: "snap_mode"
                            model: [
                                {text: "previous", value: "previous"},
                                {text: "nearest", value: "nearest"}
                            ]
                            ToolTip.text: "Whether to snap to the previous keyframe or the nearest one."
                        }
                        SettingsSpinBox {
                            label: "Snap Threshold:"
                            settingKey: "snap_threshold_ms"
                            from: 0; to: 5000
                            suffix: " ms"
                            ToolTip.text: "Maximum distance in ms to snap to a keyframe. Beyond this, no snap occurs."
                        }
                        SettingsCheckBox {
                            label: "Only snap chapter start times"
                            settingKey: "snap_starts_only"
                            ToolTip.text: "Only snap the start timestamps of chapters, not the end timestamps."
                        }
                    }
                }
            }
        }

        // ════════════════════════════════════════════════════════════════
        // Tab 5: OCR
        // ════════════════════════════════════════════════════════════════
        ScrollView {
            ColumnLayout {
                width: parent.width
                spacing: 6

                GroupBox {
                    title: "OCR Settings"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCheckBox {
                            label: "Enable OCR for image-based subtitles"
                            settingKey: "ocr_enabled"
                            ToolTip.text: "Enable optical character recognition for image-based subtitle formats (VobSub, PGS)."
                        }
                        SettingsCombo {
                            label: "OCR Engine:"
                            settingKey: "ocr_engine"
                            model: [
                                {text: "Tesseract (Traditional)", value: "tesseract"},
                                {text: "EasyOCR (Deep Learning)", value: "easyocr"},
                                {text: "PaddleOCR (State-of-Art)", value: "paddleocr"}
                            ]
                            ToolTip.text: "OCR engine to use for text recognition."
                        }
                        SettingsCombo {
                            label: "OCR Language:"
                            settingKey: "ocr_language"
                            model: [
                                {text: "English", value: "eng"},
                                {text: "Japanese", value: "jpn"},
                                {text: "Spanish", value: "spa"},
                                {text: "French", value: "fra"},
                                {text: "German", value: "deu"},
                                {text: "Chinese (Simplified)", value: "chi_sim"},
                                {text: "Chinese (Traditional)", value: "chi_tra"},
                                {text: "Korean", value: "kor"}
                            ]
                            ToolTip.text: "Primary language for OCR recognition."
                        }
                        SettingsTextField {
                            label: "Character Blacklist:"
                            settingKey: "ocr_char_blacklist"
                            placeholderText: "Characters to exclude from OCR"
                            ToolTip.text: "Characters that should never appear in OCR output."
                        }
                        SettingsSpinBox {
                            label: "Max Workers:"
                            settingKey: "ocr_max_workers"
                            from: 1; to: 24
                            suffix: " workers"
                            ToolTip.text: "Maximum number of parallel OCR worker threads."
                        }
                    }
                }

                GroupBox {
                    title: "Preprocessing"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCheckBox {
                            label: "Auto-detect optimal settings"
                            settingKey: "ocr_preprocess_auto"
                            ToolTip.text: "Automatically determine the best preprocessing pipeline for each subtitle image."
                        }
                        SettingsCheckBox {
                            label: "Force binarization"
                            settingKey: "ocr_force_binarization"
                            ToolTip.text: "Always convert images to black-and-white before OCR."
                        }
                        SettingsSpinBox {
                            label: "Upscale Threshold:"
                            settingKey: "ocr_upscale_threshold"
                            from: 20; to: 100
                            suffix: " px"
                            ToolTip.text: "Images smaller than this height in pixels will be upscaled before OCR."
                        }
                    }
                }

                GroupBox {
                    title: "Post-Processing"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCheckBox {
                            label: "Enable OCR text cleanup"
                            settingKey: "ocr_cleanup_enabled"
                            ToolTip.text: "Apply text cleanup rules to fix common OCR errors."
                        }
                        SettingsDoubleSpinBox {
                            label: "Low Confidence Threshold:"
                            settingKey: "ocr_low_confidence_threshold"
                            from: 0.0; to: 100.0; decimals: 1
                            suffix: " %"
                            ToolTip.text: "Lines with confidence below this threshold are flagged for review."
                        }
                        Button {
                            text: "Edit Dictionaries\u2026"
                            ToolTip.text: "Open the dictionary editor for OCR post-processing rules."
                            ToolTip.visible: hovered
                            ToolTip.delay: 500
                        }
                    }
                }

                GroupBox {
                    title: "Output"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCombo {
                            label: "Output Format:"
                            settingKey: "ocr_output_format"
                            model: [
                                {text: "ASS (recommended)", value: "ass"},
                                {text: "SRT", value: "srt"}
                            ]
                            ToolTip.text: "Output subtitle format for OCR results."
                        }
                        SettingsDoubleSpinBox {
                            label: "Font Size Ratio:"
                            settingKey: "ocr_font_size_ratio"
                            from: 3.00; to: 10.00; decimals: 2; stepSize: 0.05
                            suffix: " %"
                            ToolTip.text: "Font size as a percentage of video height for ASS output."
                        }
                        SettingsCheckBox {
                            label: "Preserve subtitle positions (non-bottom only)"
                            settingKey: "ocr_preserve_positions"
                            ToolTip.text: "Keep original subtitle positions for non-bottom subtitles in ASS output."
                        }
                        SettingsDoubleSpinBox {
                            label: "Bottom Threshold:"
                            settingKey: "ocr_bottom_threshold"
                            from: 50.0; to: 95.0; decimals: 1
                            suffix: " %"
                            ToolTip.text: "Y-position threshold (as % of height) above which subtitles are considered 'bottom'."
                        }
                        SettingsCheckBox {
                            label: "Generate detailed OCR report"
                            settingKey: "ocr_generate_report"
                            ToolTip.text: "Generate a detailed report with per-line confidence and timing info."
                        }
                        SettingsCheckBox {
                            label: "Save debug images"
                            settingKey: "ocr_save_debug_images"
                            ToolTip.text: "Save preprocessed subtitle images to disk for debugging."
                        }
                        SettingsCheckBox {
                            label: "Debug VobSub OCR"
                            settingKey: "ocr_debug_output"
                            ToolTip.text: "Enable verbose debug output for VobSub OCR processing."
                        }
                    }
                }
            }
        }

        // ════════════════════════════════════════════════════════════════
        // Tab 6: Merge Behavior
        // ════════════════════════════════════════════════════════════════
        ScrollView {
            ColumnLayout {
                width: parent.width
                spacing: 6

                GroupBox {
                    title: "General"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCheckBox {
                            label: "Remove dialog normalization gain (AC3/E-AC3)"
                            settingKey: "apply_dialog_norm_gain"
                            ToolTip.text: "Apply the inverse of dialog normalization gain to restore original audio levels."
                        }
                        SettingsCheckBox {
                            label: "Disable track statistics tags"
                            settingKey: "disable_track_statistics_tags"
                            ToolTip.text: "Do not write track statistics tags during muxing."
                        }
                        SettingsCheckBox {
                            label: "Disable header removal compression"
                            settingKey: "disable_header_compression"
                            ToolTip.text: "Disable Matroska header removal compression for all tracks."
                        }
                    }
                }

                GroupBox {
                    title: "Post-Merge Finalization"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCheckBox {
                            label: "Rebase timestamps to fix thumbnails (requires FFmpeg)"
                            settingKey: "post_mux_normalize_timestamps"
                            ToolTip.text: "Run a post-mux FFmpeg pass to normalize timestamps and fix thumbnail generation."
                        }
                        SettingsCheckBox {
                            label: "Strip ENCODER tag added by FFmpeg (requires mkvpropedit)"
                            settingKey: "post_mux_strip_tags"
                            ToolTip.text: "Remove the ENCODER tag that FFmpeg adds during timestamp normalization."
                        }
                    }
                }
            }
        }

        // ════════════════════════════════════════════════════════════════
        // Tab 7: Logging
        // ════════════════════════════════════════════════════════════════
        ScrollView {
            ColumnLayout {
                width: parent.width
                spacing: 6

                GroupBox {
                    title: "Log Output"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCheckBox {
                            label: "Use compact logging"
                            settingKey: "log_compact"
                            ToolTip.text: "Use a more compact log format with less whitespace."
                        }
                        SettingsCheckBox {
                            label: "Auto-scroll log view during jobs"
                            settingKey: "log_autoscroll"
                            ToolTip.text: "Automatically scroll to the bottom of the log view as new entries appear."
                        }
                        SettingsSpinBox {
                            label: "Progress Step:"
                            settingKey: "log_progress_step"
                            from: 1; to: 100
                            suffix: "%"
                            ToolTip.text: "Log a progress message every N percent."
                        }
                        SettingsSpinBox {
                            label: "Error Tail Lines:"
                            settingKey: "log_error_tail"
                            from: 0; to: 1000
                            suffix: " lines"
                            ToolTip.text: "Number of trailing lines to show from stderr when a subprocess fails."
                        }
                        SettingsCheckBox {
                            label: "Show mkvmerge options in log (pretty text)"
                            settingKey: "log_show_options_pretty"
                            ToolTip.text: "Log the mkvmerge command options in a human-readable format."
                        }
                        SettingsCheckBox {
                            label: "Show mkvmerge options in log (raw JSON)"
                            settingKey: "log_show_options_json"
                            ToolTip.text: "Log the raw JSON representation of mkvmerge options."
                        }
                    }
                }

                GroupBox {
                    title: "Sync Stability Detection"
                    Layout.fillWidth: true
                    ColumnLayout {
                        anchors.fill: parent
                        SettingsCheckBox {
                            label: "Enable sync stability detection"
                            settingKey: "sync_stability_enabled"
                            ToolTip.text: "Monitor delay variance across windows to detect unstable sync."
                        }
                        SettingsDoubleSpinBox {
                            label: "Variance Threshold:"
                            settingKey: "sync_stability_variance_threshold"
                            from: 0.0; to: 10.0; decimals: 3; stepSize: 0.001
                            suffix: " ms"
                            ToolTip.text: "Maximum acceptable variance in delay values before flagging as unstable."
                        }
                        SettingsSpinBox {
                            label: "Min Windows:"
                            settingKey: "sync_stability_min_windows"
                            from: 2; to: 30
                            ToolTip.text: "Minimum number of windows required for stability analysis."
                        }
                        SettingsCombo {
                            label: "Outlier Mode:"
                            settingKey: "sync_stability_outlier_mode"
                            model: [
                                {text: "Any Variance", value: "any"},
                                {text: "Custom Threshold", value: "threshold"}
                            ]
                            ToolTip.text: "Method for detecting outlier windows in stability analysis."
                        }
                        SettingsDoubleSpinBox {
                            label: "Outlier Threshold:"
                            settingKey: "sync_stability_outlier_threshold"
                            from: 0.001; to: 100.0; decimals: 3; stepSize: 0.1
                            suffix: " ms"
                            ToolTip.text: "Custom threshold for outlier detection in ms."
                        }
                    }
                }
            }
        }
    }

    function collectSettings() {
        return settings
    }
}
