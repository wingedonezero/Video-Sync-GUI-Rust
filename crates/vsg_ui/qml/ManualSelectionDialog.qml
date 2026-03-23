// ManualSelectionDialog.qml — 1:1 port of vsg_qt/manual_selection_dialog/ui.py
// Track layout selection with source lists and final layout list.

import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import com.vsg.ui 1.0

Dialog {
    id: root
    title: "Manual Track Selection"
    width: 1200
    height: 700
    modal: true
    standardButtons: Dialog.Ok | Dialog.Cancel

    property string trackInfoJson: "{}"
    property string previousLayoutJson: "[]"
    property string previousAttachmentsJson: "[]"
    property string previousSourceSettingsJson: "{}"

    ManualSelectionLogic {
        id: logic
        onLayoutChanged: refreshFinalList()
    }

    Component.onCompleted: {
        logic.initialize(trackInfoJson, previousLayoutJson, previousAttachmentsJson, previousSourceSettingsJson)
        populateSources()
    }

    SplitView {
        anchors.fill: parent
        orientation: Qt.Horizontal

        // Left pane: Source tracks
        ScrollView {
            SplitView.preferredWidth: 450
            SplitView.minimumWidth: 300

            ColumnLayout {
                width: parent.width
                spacing: 4

                Label {
                    text: "Available Tracks"
                    font.bold: true
                    font.pixelSize: 14
                }

                Repeater {
                    id: sourceRepeater
                    model: ListModel { id: sourceModel }

                    delegate: GroupBox {
                        title: model.sourceKey
                        Layout.fillWidth: true

                        ListView {
                            width: parent.width
                            height: contentHeight
                            interactive: false
                            model: ListModel { id: trackModel }

                            Component.onCompleted: {
                                var tracks = JSON.parse(logic.get_source_tracks(model.sourceKey))
                                for (var i = 0; i < tracks.length; i++) {
                                    var t = tracks[i]
                                    trackModel.append({
                                        trackIndex: i,
                                        sourceKey: model.sourceKey,
                                        display: "[" + t.type + "-" + (t.id || 0) + "] " + (t.codec_id || "") + " [" + (t.lang || "und") + "] " + (t.name || "")
                                    })
                                }
                            }

                            delegate: ItemDelegate {
                                width: parent ? parent.width : 200
                                text: model.display
                                onDoubleClicked: logic.add_track_to_layout(model.sourceKey, model.trackIndex)
                            }
                        }
                    }
                }

                // Attachment sources
                GroupBox {
                    title: "Attachment Sources"
                    Layout.fillWidth: true

                    ColumnLayout {
                        Repeater {
                            model: sourceModel
                            delegate: CheckBox {
                                text: model.sourceKey
                                onCheckedChanged: logic.toggle_attachment_source(model.sourceKey)
                            }
                        }
                    }
                }
            }
        }

        // Right pane: Final layout
        ColumnLayout {
            SplitView.fillWidth: true
            spacing: 4

            Label {
                text: "Final Track Order (" + logic.layout_track_count + " tracks)"
                font.bold: true
                font.pixelSize: 14
            }

            ListView {
                id: finalList
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                model: ListModel { id: finalModel }

                delegate: Rectangle {
                    width: finalList.width
                    height: 48
                    color: ListView.isCurrentItem ? palette.highlight : (index % 2 === 0 ? palette.base : palette.alternateBase)

                    RowLayout {
                        anchors.fill: parent
                        anchors.margins: 4
                        spacing: 4

                        Label {
                            text: model.display || ""
                            Layout.fillWidth: true
                            elide: Text.ElideRight
                        }
                        Label {
                            text: model.badges || ""
                            color: "orange"
                            font.bold: true
                        }
                        Button {
                            text: "×"
                            flat: true
                            onClicked: logic.remove_track_from_layout(index)
                        }
                    }

                    MouseArea {
                        anchors.fill: parent
                        acceptedButtons: Qt.LeftButton
                        onClicked: finalList.currentIndex = index
                        z: -1
                    }
                }
            }

            RowLayout {
                Layout.fillWidth: true
                Button {
                    text: "Move Up"
                    enabled: finalList.currentIndex > 0
                    onClicked: logic.move_track(finalList.currentIndex, finalList.currentIndex - 1)
                }
                Button {
                    text: "Move Down"
                    enabled: finalList.currentIndex >= 0 && finalList.currentIndex < logic.layout_track_count - 1
                    onClicked: logic.move_track(finalList.currentIndex, finalList.currentIndex + 1)
                }
                Item { Layout.fillWidth: true }
                Button {
                    text: "Remove"
                    enabled: finalList.currentIndex >= 0
                    onClicked: logic.remove_track_from_layout(finalList.currentIndex)
                }
            }
        }
    }

    function populateSources() {
        var keys = JSON.parse(logic.get_source_keys())
        sourceModel.clear()
        for (var i = 0; i < keys.length; i++) {
            sourceModel.append({sourceKey: keys[i]})
        }
    }

    function refreshFinalList() {
        finalModel.clear()
        for (var i = 0; i < logic.layout_track_count; i++) {
            var track = JSON.parse(logic.get_layout_track(i))
            var type = track.type || "?"
            var codec = track.codec_id || ""
            var lang = track.lang || "und"
            var source = track.source || ""
            finalModel.append({
                display: "[" + source + "] " + type + ": " + codec + " [" + lang + "]",
                badges: ""
            })
        }
    }

    function getResult() {
        return logic.get_result()
    }
}
