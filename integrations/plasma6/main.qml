import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.plasma.components as PlasmaComponents3
import org.kde.plasma.plasma5support as Plasma5Support
import org.kde.plasma.plasmoid

PlasmoidItem {
    id: root

    readonly property string orbitfm: "@ORBITFM@"
    readonly property string statusCommand: orbitfm + " status --json"
    readonly property int maximumTextWidth: 220

    property var status: ({})
    property string lastError: ""

    readonly property string playbackState: status.state || "stopped"
    readonly property string stationName: status.station ? status.station.name : ""
    readonly property string idleLabel: status.ui ? status.ui.nothing_playing : "OrbitFM"
    readonly property string displayText: stationName.length > 0 ? stationName : idleLabel
    readonly property int volumePercent: Math.round((status.volume || 0) * 100)

    toolTipMainText: "OrbitFM"
    toolTipSubText: {
        if (stationName.length === 0)
            return displayText
        return displayText + "\n" + playbackState + " · " + volumePercent + "%"
    }

    compactRepresentation: MouseArea {
        id: compact

        implicitWidth: Math.min(label.implicitWidth, root.maximumTextWidth) + 16
        implicitHeight: label.implicitHeight + 8
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        hoverEnabled: true

        onClicked: function(mouse) {
            if (mouse.button === Qt.LeftButton)
                root.runCommand("playpause")
            else if (mouse.button === Qt.RightButton)
                root.runCommand("panel")
        }

        onWheel: function(wheel) {
            if (wheel.angleDelta.y > 0)
                root.runCommand("volume-up")
            else if (wheel.angleDelta.y < 0)
                root.runCommand("volume-down")
        }

        Text {
            id: label

            anchors.centerIn: parent
            width: Math.min(implicitWidth, root.maximumTextWidth)
            text: root.displayText
            color: Kirigami.Theme.textColor
            elide: Text.ElideRight
            horizontalAlignment: Text.AlignHCenter
            font.pixelSize: Kirigami.Theme.defaultFont.pixelSize
        }
    }

    fullRepresentation: ColumnLayout {
        Layout.preferredWidth: 260
        Layout.preferredHeight: 190
        spacing: 6

        Text {
            Layout.fillWidth: true
            text: "OrbitFM"
            color: Kirigami.Theme.textColor
            font.bold: true
            font.pixelSize: Kirigami.Theme.defaultFont.pixelSize * 1.2
        }

        Text {
            Layout.fillWidth: true
            text: root.displayText
            color: Kirigami.Theme.textColor
            elide: Text.ElideRight
        }

        Text {
            Layout.fillWidth: true
            visible: root.stationName.length > 0
            text: root.playbackState + " · " + root.volumePercent + "%"
            color: Kirigami.Theme.disabledTextColor
            font.pixelSize: Kirigami.Theme.smallFont.pixelSize
        }

        RowLayout {
            Layout.topMargin: 8

            PlasmaComponents3.Button {
                text: root.status.ui ? root.status.ui.play_pause : "..."
                onClicked: root.runCommand("playpause")
            }

            PlasmaComponents3.Button {
                text: root.status.ui ? root.status.ui.stop : "..."
                onClicked: root.runCommand("stop")
            }

            PlasmaComponents3.Button {
                text: root.status.ui ? root.status.ui.settings : "..."
                onClicked: root.runCommand("settings")
            }
        }

        Item {
            Layout.fillHeight: true
        }
    }

    Plasma5Support.DataSource {
        id: pollSource

        engine: "executable"
        connectedSources: []

        onNewData: function(source, data) {
            if (source !== root.statusCommand)
                return
            var code = data["exit code"]
            if (typeof code === "number" && code !== 0) {
                root.status = ({})
                return
            }
            var out = data["stdout"] || ""
            if (out.length === 0)
                return
            try {
                root.status = JSON.parse(out)
                root.lastError = ""
            } catch (error) {
                root.lastError = String(error)
            }
        }
    }

    Plasma5Support.DataSource {
        id: actionSource

        engine: "executable"
        connectedSources: []

        onNewData: function(source, data) {
            actionSource.disconnectSource(source)
        }
    }

    function runCommand(name) {
        var command = root.orbitfm + " " + name
        actionSource.disconnectSource(command)
        actionSource.connectSource(command, 0)
    }

    Component.onCompleted: pollSource.connectSource(root.statusCommand, 2000)
    Component.onDestruction: pollSource.disconnectSource(root.statusCommand)
}
