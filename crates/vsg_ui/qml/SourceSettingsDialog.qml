// SourceSettingsDialog.qml — 1:1 port of vsg_qt/source_settings_dialog/dialog.py
// Per-source audio track settings (correlation track, source separation).

import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import com.vsg.ui 1.0

Dialog {
    id: root
    title: "Source Settings"
    width: 500
    height: 300
    modal: true
    standardButtons: Dialog.Ok | Dialog.Cancel | Dialog.Reset

    property string sourceJson: "{}"

    SourceSettingsLogic { id: logic }

    Component.onCompleted: logic.initialize(sourceJson)

    onReset: logic.reset_to_defaults()

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 8
        spacing: 8

        Label {
            text: "Settings for " + logic.source_key
            font.bold: true
            font.pixelSize: 14
        }

        // Audio track selector
        RowLayout {
            Label {
                text: logic.source_key === "Source 1" ? "Reference Audio Track:" : "Correlation Audio Track:"
                Layout.preferredWidth: 180
            }
            ComboBox {
                id: trackCombo
                Layout.fillWidth: true
                model: {
                    var tracks = JSON.parse(logic.get_audio_tracks())
                    return tracks.map(function(t, i) {
                        return "Track " + (t.id || i) + " - " + (t.codec_id || "?") + " [" + (t.lang || "und") + "]"
                    })
                }
                currentIndex: logic.selected_track
                onCurrentIndexChanged: logic.selected_track = currentIndex
            }
        }

        // Source separation toggle (Source 2/3 only)
        CheckBox {
            text: "Use Source Separation"
            visible: logic.source_key !== "Source 1"
            checked: logic.use_source_separation
            onCheckedChanged: logic.use_source_separation = checked
        }
    }

    function getResult() { return logic.get_result() }
}
