import QtQuick
import Quickshell
import Quickshell.Io
import qs.Ui

BarWidget {
    id: root

    moduleName: "io.github.modernbyte.orbitfm"

    readonly property string orbitfm: "@ORBITFM@"
    readonly property int maximumTextLength: 28

    property var status: ({})

    readonly property string stationName: status.station ? plain(status.station.name) : ""
    readonly property string idleLabel: status.ui ? status.ui.nothing_playing : "OrbitFM"
    readonly property int volumePercent: Math.round((status.volume || 0) * 100)

    // Имя и страна приходят из общей базы станций: угловые скобки не должны
    // трактоваться как разметка ни в надписи, ни в подсказке.
    function plain(value) {
        if (value === undefined || value === null)
            return ""
        return String(value).replace(/</g, "‹").replace(/>/g, "›")
    }

    readonly property string stateGlyph: {
        switch (status.state || "stopped") {
        case "playing":
            return "▶"
        case "paused":
            return "⏸"
        case "buffering":
        case "reconnecting":
            return "…"
        case "error":
            return "⚠"
        default:
            return "◍"
        }
    }

    readonly property string displayText: {
        var text = stationName.length > 0 ? stationName : (idleLabel || "OrbitFM")
        if (text.length > maximumTextLength)
            text = text.slice(0, maximumTextLength - 1) + "…"
        return stateGlyph + " " + text
    }

    readonly property string stationDetails: {
        if (stationName.length === 0)
            return displayText
        var parts = []
        if (status.station.country)
            parts.push(plain(status.station.country))
        if (status.station.bitrate > 0)
            parts.push(status.station.bitrate + " kbps")
        parts.push(volumePercent + "%")
        return parts.join(" · ")
    }

    function runCommand(name) {
        action.command = [root.orbitfm, name]
        action.running = true
    }

    WidgetButton {
        id: button

        bar: root.bar
        text: root.displayText
        tooltipText: stationName.length > 0
            ? stationName + "\n" + stationDetails
            : displayText

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
