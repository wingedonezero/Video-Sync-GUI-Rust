// OCRDictionaryDialog.qml — 1:1 port of vsg_qt/ocr_dictionary_dialog/ui.py
// OCR dictionary editor with tabs for replacements, word lists, SE config.

import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import com.vsg.ui 1.0

Dialog {
    id: root
    title: "OCR Dictionary Editor"
    width: 900
    height: 650
    modal: true
    standardButtons: Dialog.Close

    property string configDir: ""

    OCRDictionaryLogic {
        id: logic
        onDataChanged: refreshCurrentTab()
    }

    Component.onCompleted: logic.initialize(configDir)

    TabBar {
        id: tabBar
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        TabButton { text: "Replacements" }
        TabButton { text: "User Dictionary" }
        TabButton { text: "Names" }
        TabButton { text: "SubtitleEdit" }
    }

    StackLayout {
        anchors.top: tabBar.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 8
        currentIndex: tabBar.currentIndex

        // Tab 0: Replacements
        ColumnLayout {
            spacing: 6

            ListView {
                id: replacementsList
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                model: ListModel { id: replacementsModel }
                delegate: ItemDelegate {
                    width: replacementsList.width
                    text: model.pattern + " → " + model.replacement
                }
            }

            RowLayout {
                TextField { id: patternField; placeholderText: "Pattern"; Layout.fillWidth: true }
                TextField { id: replacementField; placeholderText: "Replacement"; Layout.fillWidth: true }
                Button {
                    text: "Add"
                    onClicked: {
                        logic.add_replacement(patternField.text, replacementField.text, false)
                        patternField.text = ""
                        replacementField.text = ""
                        refreshReplacements()
                    }
                }
                Button {
                    text: "Save"
                    onClicked: logic.save_replacements(JSON.stringify(getReplacementRules()))
                }
            }
        }

        // Tab 1: User Dictionary
        ColumnLayout {
            spacing: 6

            TextField {
                id: wordFilter
                placeholderText: "Filter words..."
                Layout.fillWidth: true
            }

            ListView {
                id: wordsList
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                model: ListModel { id: wordsModel }
                delegate: ItemDelegate {
                    width: wordsList.width
                    text: model.word
                }
            }

            RowLayout {
                TextField { id: newWordField; placeholderText: "New word"; Layout.fillWidth: true }
                Button {
                    text: "Add"
                    onClicked: {
                        var msg = logic.add_user_word(newWordField.text)
                        newWordField.text = ""
                        refreshWords()
                    }
                }
            }
        }

        // Tab 2: Names
        ColumnLayout {
            spacing: 6

            ListView {
                id: namesList
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                model: ListModel { id: namesModel }
                delegate: ItemDelegate {
                    width: namesList.width
                    text: model.name
                }
            }

            RowLayout {
                TextField { id: newNameField; placeholderText: "New name"; Layout.fillWidth: true }
                Button {
                    text: "Add"
                    onClicked: {
                        logic.add_name(newNameField.text)
                        newNameField.text = ""
                        refreshNames()
                    }
                }
            }
        }

        // Tab 3: SubtitleEdit Config
        ColumnLayout {
            spacing: 6

            Label { text: "SubtitleEdit Dictionary Configuration"; font.bold: true }

            Button {
                text: "Reload Dictionaries"
                onClicked: logic.reload()
            }

            Label {
                text: "SE dictionary config is loaded from the config directory.\nEdit via the JSON config to enable/disable dictionary types."
                wrapMode: Text.Wrap
            }
        }
    }

    function refreshCurrentTab() {
        switch (tabBar.currentIndex) {
            case 0: refreshReplacements(); break
            case 1: refreshWords(); break
            case 2: refreshNames(); break
        }
    }

    function refreshReplacements() {
        var rules = JSON.parse(logic.get_replacements())
        replacementsModel.clear()
        for (var i = 0; i < rules.length; i++) {
            replacementsModel.append({
                pattern: rules[i].pattern || "",
                replacement: rules[i].replacement || ""
            })
        }
    }

    function refreshWords() {
        var words = JSON.parse(logic.get_user_words())
        wordsModel.clear()
        for (var i = 0; i < words.length; i++) {
            wordsModel.append({word: words[i]})
        }
    }

    function refreshNames() {
        var names = JSON.parse(logic.get_names())
        namesModel.clear()
        for (var i = 0; i < names.length; i++) {
            namesModel.append({name: names[i]})
        }
    }

    function getReplacementRules() {
        var rules = []
        for (var i = 0; i < replacementsModel.count; i++) {
            rules.push(replacementsModel.get(i))
        }
        return rules
    }
}
