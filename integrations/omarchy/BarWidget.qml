import QtQuick
import Quickshell
import Quickshell.Io
import qs.Ui

BarWidget {
    id: root

    moduleName: "io.github.modernbyte.orbitfm"

    readonly property string orbitfm: "@ORBITFM@"
    property var status: ({})

    readonly property string stationName: status.station ? status.station.name : ""
    readonly property string idleLabel: status.ui ? status.ui.nothing_playing : "OrbitFM"
    readonly property string displayText: stationName.length > 0 ? stationName : idleLabel
    readonly property int volumePercent: Math.round((status.volume || 0) * 100)

    function runCommand(name) {
        action.command = [root.orbitfm, name]
        action.running = true
    }

    WidgetButton {
        id: button

        bar: root.bar
        text: root.displayText
        tooltipText: root.stationName.length > 0
            ? root.stationName + "\n" + root.volumePercent + "%"
            : root.displayText

        onPressed: function(buttonCode) {
            if (buttonCode === Qt.LeftButton)
                root.runCommand("playpause")
            else if (buttonCode === Qt.RightButton)
                root.runCommand("panel")
        }
    }

    Process {
        id: status

        command: [root.orbitfm, "status", "--json"]
        running: true

        stdout: SplitParser {
            onRead: line => {
                if (line.length === 0)
                    return
                try {
                    root.status = JSON.parse(line)
                } catch (error) {
                    root.status = ({})
                }
            }
        }

        onExited: function(exitCode) {
            if (exitCode !== 0)
                root.status = ({})
        }
    }

    Process {
        id: action

        command: [root.orbitfm]
        running: false
    }

    Timer {
        interval: 2000
        repeat: true
        running: true

        onTriggered: {
            if (!status.running)
                status.running = true
        }
    }
}
