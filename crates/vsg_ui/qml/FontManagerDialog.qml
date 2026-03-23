// FontManagerDialog.qml — 1:1 port of vsg_qt/font_manager_dialog/ui.py
// Font replacement management for subtitle tracks.

import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import com.vsg.ui 1.0

Dialog {
    id: root
    title: "Font Manager"
    width: 900
    height: 600
    modal: true
    standardButtons: Dialog.Ok | Dialog.Cancel

    property string dataJson: "{}"

    FontManagerLogic {
        id: logic
        onReplacementsChanged: refreshReplacements()
    }

    Component.onCompleted: logic.initialize(dataJson)

    SplitView {
        anchors.fill: parent
        orientation: Qt.Horizontal

        // Left: File fonts (styles in subtitle)
        GroupBox {
            title: "Styles in File"
            SplitView.preferredWidth: 300

            ListView {
                id: fileFontsList
                anchors.fill: parent
                clip: true
                model: ListModel { id: fileFontsModel }

                Component.onCompleted: {
                    var fonts = JSON.parse(logic.get_file_fonts())
                    for (var i = 0; i < fonts.length; i++)
                        fileFontsModel.append(fonts[i])
                }

                delegate: ItemDelegate {
                    width: fileFontsList.width
                    text: model.style + " → " + model.font
                    onClicked: fileFontsList.currentIndex = index
                }
            }
        }

        // Center: Replacement controls
        ColumnLayout {
            SplitView.preferredWidth: 300
            spacing: 8

            GroupBox {
                title: "Replacements (" + logic.replacement_count + ")"
                Layout.fillWidth: true
                Layout.fillHeight: true

                ListView {
                    id: replacementsList
                    anchors.fill: parent
                    clip: true
                    model: ListModel { id: replacementsModel }

                    delegate: ItemDelegate {
                        width: replacementsList.width
                        text: model.style + " → " + model.newFont
                    }
                }
            }

            RowLayout {
                Button {
                    text: "Add"
                    enabled: fileFontsList.currentIndex >= 0 && userFontsList.currentIndex >= 0
                    onClicked: {
                        var fileFont = fileFontsModel.get(fileFontsList.currentIndex)
                        var userFont = userFontsModel.get(userFontsList.currentIndex)
                        if (fileFont && userFont)
                            logic.add_replacement(fileFont.style, userFont.family, userFont.path)
                    }
                }
                Button {
                    text: "Remove"
                    enabled: replacementsList.currentIndex >= 0
                    onClicked: {
                        var item = replacementsModel.get(replacementsList.currentIndex)
                        if (item) logic.remove_replacement(item.style)
                    }
                }
                Button { text: "Clear All"; onClicked: logic.clear_all() }
            }
        }

        // Right: User fonts
        GroupBox {
            title: "Available Fonts"
            SplitView.fillWidth: true

            ListView {
                id: userFontsList
                anchors.fill: parent
                clip: true
                model: ListModel { id: userFontsModel }

                Component.onCompleted: {
                    var fonts = JSON.parse(logic.get_user_fonts())
                    for (var i = 0; i < fonts.length; i++)
                        userFontsModel.append(fonts[i])
                }

                delegate: ItemDelegate {
                    width: userFontsList.width
                    text: model.family
                    onClicked: userFontsList.currentIndex = index
                }
            }
        }
    }

    function refreshReplacements() {
        var result = JSON.parse(logic.get_result())
        replacementsModel.clear()
        for (var style in result) {
            replacementsModel.append({style: style, newFont: result[style].font_name || ""})
        }
    }

    function getResult() { return logic.get_result() }
}
