// TrackWidget.qml — 1:1 port of vsg_qt/track_widget/ui.py
// Reusable track display component for the manual selection dialog.

import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import com.vsg.ui 1.0

Rectangle {
    id: root
    height: 56
    radius: 4
    color: palette.base
    border.color: palette.mid
    border.width: 1

    property string trackJson: "{}"

    TrackWidgetLogic {
        id: logic
        onTrackModified: root.trackModified()
    }

    signal trackModified()

    Component.onCompleted: logic.initialize(trackJson)

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 4
        spacing: 2

        // Top row: summary + badges
        RowLayout {
            Layout.fillWidth: true
            Label {
                text: logic.summary_text
                font.bold: true
                Layout.fillWidth: true
                elide: Text.ElideRight
            }
            Label {
                text: logic.badge_text
                color: "orange"
                font.bold: true
                font.pixelSize: 11
            }
        }

        // Bottom row: quick controls
        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            CheckBox {
                text: "Default"
                checked: logic.is_default
                onCheckedChanged: { logic.is_default = checked; logic.refresh_display() }
                visible: logic.track_type !== "video"
            }
            CheckBox {
                text: "Forced"
                checked: logic.is_forced
                onCheckedChanged: { logic.is_forced = checked; logic.refresh_display() }
                visible: logic.track_type === "subtitles"
            }

            Item { Layout.fillWidth: true }

            Button {
                text: "Settings…"
                flat: true
                font.pixelSize: 11
                onClicked: logic.openSettingsRequested()
            }
        }
    }

    function getConfig() { return logic.get_config() }
    function applySettings(json) { logic.apply_settings(json) }
}
