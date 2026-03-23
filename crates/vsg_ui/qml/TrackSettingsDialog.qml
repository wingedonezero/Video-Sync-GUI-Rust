// TrackSettingsDialog.qml — 1:1 port of vsg_qt/track_settings_dialog/ui.py
// Per-track settings: language, name, OCR, conversion, sync exclusion.

import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import com.vsg.ui 1.0

Dialog {
    id: root
    title: "Track Settings"
    width: 500
    height: 400
    modal: true
    standardButtons: Dialog.Ok | Dialog.Cancel

    property string trackJson: "{}"

    TrackSettingsLogic {
        id: logic
        onOpenSyncExclusion: syncExclusionDialog.open()
        onOpenStyleEditor: {} // TODO: launch subtitle editor
        onOpenFontReplacements: {} // TODO: launch font manager
    }

    Component.onCompleted: logic.initialize(trackJson)

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 8
        spacing: 8

        // Language
        RowLayout {
            Label { text: "Language:"; Layout.preferredWidth: 120 }
            ComboBox {
                id: langCombo
                Layout.fillWidth: true
                model: {
                    var langs = JSON.parse(logic.get_languages())
                    return langs.map(function(l) { return l.display + " (" + l.code + ")" })
                }
                onCurrentIndexChanged: {
                    var langs = JSON.parse(logic.get_languages())
                    if (currentIndex >= 0 && currentIndex < langs.length)
                        logic.custom_lang = langs[currentIndex].code
                }
            }
        }

        // Custom name
        RowLayout {
            Label { text: "Custom Name:"; Layout.preferredWidth: 120 }
            TextField {
                text: logic.custom_name
                onTextChanged: logic.custom_name = text
                Layout.fillWidth: true
            }
        }

        // Subtitle-specific options
        GroupBox {
            title: "Subtitle Options"
            Layout.fillWidth: true
            visible: logic.track_type === "subtitles"

            ColumnLayout {
                anchors.fill: parent
                CheckBox {
                    text: "Perform OCR"
                    checked: logic.perform_ocr
                    onCheckedChanged: logic.perform_ocr = checked
                    enabled: logic.ocr_available
                }
                CheckBox {
                    text: "Convert to ASS"
                    checked: logic.convert_to_ass
                    onCheckedChanged: logic.convert_to_ass = checked
                    enabled: logic.convert_available
                }
                CheckBox {
                    text: "Rescale"
                    checked: logic.rescale
                    onCheckedChanged: logic.rescale = checked
                }
                RowLayout {
                    visible: logic.rescale
                    Label { text: "Size Multiplier:" }
                    SpinBox {
                        from: 10; to: 1000; stepSize: 10
                        value: logic.size_multiplier * 100
                        onValueChanged: logic.size_multiplier = value / 100.0
                        textFromValue: function(v) { return (v / 100.0).toFixed(2) }
                    }
                }
                Button {
                    text: "Sync Exclusion Zones..."
                    enabled: logic.sync_exclusion_available
                    onClicked: logic.openSyncExclusion()
                }
            }
        }
    }

    SyncExclusionDialog {
        id: syncExclusionDialog
        trackJson: root.trackJson
    }

    function getResult() {
        return logic.get_result()
    }
}
