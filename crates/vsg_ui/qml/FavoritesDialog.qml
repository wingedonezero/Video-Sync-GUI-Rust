// FavoritesDialog.qml — 1:1 port of vsg_qt/favorites_dialog/ui.py
// Manage saved favorite colors.

import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import QtQuick.Dialogs
import com.vsg.ui 1.0

Dialog {
    id: root
    title: "Favorite Colors"
    width: 500
    height: 400
    modal: true
    standardButtons: Dialog.Close

    property string configDir: ""

    FavoritesLogic {
        id: logic
        onFavoritesChanged: refreshList()
    }

    Component.onCompleted: {
        logic.initialize(configDir)
        refreshList()
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 8
        spacing: 6

        ListView {
            id: favoritesList
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: ListModel { id: favModel }

            delegate: ItemDelegate {
                width: favoritesList.width
                height: 36
                RowLayout {
                    anchors.fill: parent
                    anchors.margins: 4
                    Rectangle {
                        width: 24; height: 24
                        color: model.hex || "#000000"
                        border.color: "gray"
                    }
                    Label { text: model.name || ""; Layout.fillWidth: true }
                    Label { text: model.hex || ""; color: "gray" }
                }
                onClicked: favoritesList.currentIndex = index
            }
        }

        RowLayout {
            Button {
                text: "Add"
                onClicked: colorDialog.open()
            }
            Button {
                text: "Delete"
                enabled: favoritesList.currentIndex >= 0
                onClicked: {
                    var item = favModel.get(favoritesList.currentIndex)
                    if (item) logic.delete_favorite(item.id)
                }
            }
        }
    }

    ColorDialog {
        id: colorDialog
        title: "Pick a Color"
        onAccepted: logic.add_favorite("New Color", selectedColor.toString())
    }

    function refreshList() {
        var data = JSON.parse(logic.load_favorites())
        favModel.clear()
        for (var i = 0; i < data.length; i++) {
            favModel.append(data[i])
        }
    }
}
