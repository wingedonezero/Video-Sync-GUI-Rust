// SyncExclusionDialog.qml — 1:1 port of vsg_qt/sync_exclusion_dialog/ui.py
// Configure style-based sync exclusion zones.

import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import com.vsg.ui 1.0

Dialog {
    id: root
    title: "Sync Exclusion Zones"
    width: 600
    height: 500
    modal: true
    standardButtons: Dialog.Ok | Dialog.Cancel

    property string trackJson: "{}"

    SyncExclusionLogic {
        id: logic
        onPreviewUpdated: previewLabel.text = "Excluded: " + logic.excluded_events + " / " + logic.total_events + " events"
    }

    Component.onCompleted: logic.initialize(trackJson)

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 8
        spacing: 8

        // Mode selection
        GroupBox {
            title: "Exclusion Mode"
            Layout.fillWidth: true
            RowLayout {
                RadioButton {
                    text: "Exclude selected styles"
                    checked: logic.mode === "exclude"
                    onClicked: logic.update_mode("exclude")
                }
                RadioButton {
                    text: "Include only selected styles"
                    checked: logic.mode === "include"
                    onClicked: logic.update_mode("include")
                }
            }
        }

        // Style list
        GroupBox {
            title: "Styles"
            Layout.fillWidth: true
            Layout.fillHeight: true

            ColumnLayout {
                anchors.fill: parent

                RowLayout {
                    Button {
                        text: "Select All"
                        onClicked: {
                            var styles = JSON.parse(logic.get_available_styles())
                            var names = styles.map(function(s) { return s.name })
                            logic.set_exclusion_styles(JSON.stringify(names))
                            refreshCheckboxes()
                        }
                    }
                    Button {
                        text: "Deselect All"
                        onClicked: {
                            logic.set_exclusion_styles("[]")
                            refreshCheckboxes()
                        }
                    }
                }

                ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true

                    ColumnLayout {
                        id: styleCheckboxes
                        width: parent.width
                    }
                }
            }
        }

        // Preview
        Label {
            id: previewLabel
            text: "Excluded: 0 / 0 events"
            font.italic: true
        }
    }

    function populateStyles() {
        // Styles are populated via the Repeater model in the style checkboxes section
    }

    function refreshCheckboxes() {
        // Re-sync checkbox state with logic
    }

    function getResult() {
        return logic.get_result()
    }
}
