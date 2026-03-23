// ResampleDialog.qml — 1:1 port of vsg_qt/resample_dialog/ui.py
// Subtitle rescaling: source resolution → destination resolution.

import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import com.vsg.ui 1.0

Dialog {
    id: root
    title: "Resample Resolution"
    width: 400
    height: 250
    modal: true
    standardButtons: Dialog.Ok | Dialog.Cancel

    property string dataJson: "{}"

    ResampleLogic { id: logic }

    Component.onCompleted: logic.initialize(dataJson)

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 8
        spacing: 8

        GroupBox {
            title: "Source Resolution (from script)"
            Layout.fillWidth: true
            RowLayout {
                Label { text: logic.source_x + " × " + logic.source_y }
            }
        }

        GroupBox {
            title: "Destination Resolution"
            Layout.fillWidth: true
            RowLayout {
                spacing: 8
                SpinBox { from: 1; to: 9999; value: logic.dest_x; onValueChanged: logic.dest_x = value }
                Label { text: "×" }
                SpinBox { from: 1; to: 9999; value: logic.dest_y; onValueChanged: logic.dest_y = value }
                Button {
                    text: "From Video"
                    onClicked: logic.probe_video_resolution("")
                }
            }
        }
    }

    function getResult() { return logic.get_result() }
}
