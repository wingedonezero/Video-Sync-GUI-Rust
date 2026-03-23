// SubtitleEditorWindow.qml — 1:1 port of vsg_qt/subtitle_editor/editor_window.py
// Main subtitle editor with video panel, events table, and tab panel.

import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import com.vsg.ui 1.0

ApplicationWindow {
    id: root
    title: logic.window_title
    width: 1200
    height: 800

    property string subtitlePath: ""
    property string videoPath: ""

    SubtitleEditorLogic {
        id: logic
        onFileLoaded: {
            eventsModel.clear()
            var events = JSON.parse(logic.get_events())
            for (var i = 0; i < events.length; i++) {
                eventsModel.append(events[i])
            }
            stylesModel.clear()
            var styles = JSON.parse(logic.get_styles())
            for (var j = 0; j < styles.length; j++) {
                stylesModel.append(styles[j])
            }
        }
    }

    Component.onCompleted: {
        if (subtitlePath.length > 0)
            logic.open_file(subtitlePath)
    }

    menuBar: MenuBar {
        Menu {
            title: "File"
            Action { text: "Save"; shortcut: "Ctrl+S"; onTriggered: logic.save_file() }
            Action { text: "Save As..."; shortcut: "Ctrl+Shift+S"; onTriggered: saveAsDialog.open() }
            MenuSeparator {}
            Action { text: "Close"; onTriggered: root.close() }
        }
        Menu {
            title: "Edit"
            Action { text: "Undo"; shortcut: "Ctrl+Z"; onTriggered: logic.undo() }
            Action { text: "Redo"; shortcut: "Ctrl+Shift+Z"; onTriggered: logic.redo() }
        }
    }

    SplitView {
        anchors.fill: parent
        orientation: Qt.Vertical

        // Top: Video + Tabs
        SplitView {
            SplitView.preferredHeight: 350
            orientation: Qt.Horizontal

            // Video panel
            Rectangle {
                SplitView.preferredWidth: root.width * 0.4
                color: "black"
                Label {
                    anchors.centerIn: parent
                    text: videoPath.length > 0 ? "Video: " + videoPath : "No video loaded"
                    color: "gray"
                }
            }

            // Tab panel (styles, filtering, fonts)
            ColumnLayout {
                SplitView.fillWidth: true

                ComboBox {
                    id: tabSelector
                    Layout.fillWidth: true
                    model: ["Styles", "Filtering", "Fonts"]
                }

                StackLayout {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    currentIndex: tabSelector.currentIndex

                    // Styles tab
                    ScrollView {
                        ColumnLayout {
                            width: parent.width
                            spacing: 4
                            Label { text: "Style Editor"; font.bold: true }
                            ComboBox {
                                id: styleSelector
                                Layout.fillWidth: true
                                model: stylesModel
                                textRole: "name"
                            }
                            Label { text: logic.style_count + " styles loaded" }
                        }
                    }

                    // Filtering tab
                    ScrollView {
                        ColumnLayout {
                            width: parent.width
                            spacing: 4
                            Label { text: "Event Filtering"; font.bold: true }
                            Label { text: logic.event_count + " events" }
                        }
                    }

                    // Fonts tab
                    ScrollView {
                        ColumnLayout {
                            width: parent.width
                            spacing: 4
                            Label { text: "Font Replacements"; font.bold: true }
                        }
                    }
                }
            }
        }

        // Bottom: Events table
        ColumnLayout {
            SplitView.fillHeight: true

            Label {
                text: "Events (" + logic.event_count + ")"
                font.bold: true
                Layout.margins: 4
            }

            ListView {
                id: eventsTable
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                model: ListModel { id: eventsModel }

                header: Rectangle {
                    width: eventsTable.width; height: 28
                    color: palette.mid
                    RowLayout {
                        anchors.fill: parent; anchors.margins: 2; spacing: 4
                        Label { text: "#"; Layout.preferredWidth: 40; font.bold: true }
                        Label { text: "Start"; Layout.preferredWidth: 100; font.bold: true }
                        Label { text: "End"; Layout.preferredWidth: 100; font.bold: true }
                        Label { text: "Style"; Layout.preferredWidth: 100; font.bold: true }
                        Label { text: "Text"; Layout.fillWidth: true; font.bold: true }
                    }
                }

                delegate: Rectangle {
                    width: eventsTable.width; height: 24
                    color: model.is_comment ? "#333" : (index % 2 === 0 ? palette.base : palette.alternateBase)

                    RowLayout {
                        anchors.fill: parent; anchors.margins: 2; spacing: 4
                        Label { text: String(model.index); Layout.preferredWidth: 40; color: model.is_comment ? "gray" : palette.text }
                        Label { text: formatTime(model.start_ms); Layout.preferredWidth: 100 }
                        Label { text: formatTime(model.end_ms); Layout.preferredWidth: 100 }
                        Label { text: model.style; Layout.preferredWidth: 100 }
                        Label { text: model.text; Layout.fillWidth: true; elide: Text.ElideRight }
                    }

                    MouseArea {
                        anchors.fill: parent
                        onClicked: eventsTable.currentIndex = index
                    }
                }
            }
        }
    }

    ListModel { id: stylesModel }

    FileDialog {
        id: saveAsDialog
        title: "Save As"
        fileMode: FileDialog.SaveFile
        onAccepted: logic.save_file_as(selectedFile.toString().replace("file://", ""))
    }

    function formatTime(ms) {
        if (ms === undefined) return "0:00:00.00"
        var totalCs = Math.floor(ms / 10)
        var cs = totalCs % 100
        var totalS = Math.floor(ms / 1000)
        var s = totalS % 60
        var totalM = Math.floor(totalS / 60)
        var m = totalM % 60
        var h = Math.floor(totalM / 60)
        return h + ":" + String(m).padStart(2, '0') + ":" + String(s).padStart(2, '0') + "." + String(cs).padStart(2, '0')
    }
}
