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
    readonly property int maximumTextWidth: 160

    property var status: ({})
    property string lastError: ""
    property int pendingVolume: -1
    // Слайдер живёт во `fullRepresentation`, поэтому ссылка на него
    // передаётся наружу явно: у встроенного компонента своя область id.
    property var volumeControl: null

    readonly property string playbackState: status.state || "stopped"
    readonly property string stationName: status.station ? status.station.name : ""
    readonly property string idleLabel: status.ui && status.ui.nothing_playing ? status.ui.nothing_playing : "OrbitFM"
    readonly property string displayText: stationName.length > 0 ? stationName : idleLabel
    readonly property int volumePercent: Math.round((status.volume || 0) * 100)
    readonly property bool hasStation: status.station !== undefined && status.station !== null
    readonly property bool isFavorite: hasStation && Boolean(status.station.favorite)
    readonly property string themeMode: status.theme || "system"
    readonly property color accentTone: accentFor(status.accent)
    readonly property color stateTone: stateFor(playbackState)

    readonly property string stationMeta: {
        if (!hasStation)
            return ""
        var parts = []
        if (status.station.source === "stream")
            parts.push("stream")
        else if (status.station.country)
            parts.push(status.station.country)
        if (status.station.bitrate > 0)
            parts.push(status.station.bitrate + " kbps")
        return parts.join(" · ")
    }

    readonly property string stationTags: {
        if (!hasStation || !status.station.tags)
            return ""
        return status.station.tags.split(",")
            .map(function(tag) { return tag.trim() })
            .filter(function(tag) { return tag.length > 0 })
            .slice(0, 2)
            .join(" · ")
    }

    readonly property string monogram: {
        if (!hasStation)
            return "◍"
        var name = stationName.trim()
        return name.length > 0 ? name.charAt(0).toUpperCase() : "◍"
    }

    toolTipMainText: "OrbitFM"
    toolTipSubText: stationName.length > 0
        ? stationName + "\n" + uiText("volume") + " · " + volumePercent + "%"
        : displayText

    function accentFor(name) {
        switch (name) {
        case "green":
            return "#68d391"
        case "amber":
            return "#ffbe5c"
        case "violet":
            return "#b294ff"
        case "rose":
            return "#ff8aa5"
        case "cyan":
            return "#5cd6e0"
        default:
            return "#6cb0ff"
        }
    }

    function stateFor(state) {
        switch (state) {
        case "playing":
            return "#34d399"
        case "paused":
            return "#ffbe5c"
        case "buffering":
        case "reconnecting":
            return "#b294ff"
        case "error":
            return "#ff8aa5"
        default:
            return "#969eb0"
        }
    }

    function uiText(key) {
        if (status.ui && status.ui[key])
            return status.ui[key]
        var fallback = {
            "theme": "Theme",
            "theme_system": "System",
            "theme_dark": "Dark",
            "theme_light": "Light",
            "show_window": "Show window",
            "hide_window": "Hide window"
        }
        return fallback[key] || "OrbitFM"
    }

    function runCommand(args) {
        var command = root.orbitfm + " " + args
        actionSource.disconnectSource(command)
        actionSource.connectSource(command, 0)
    }

    compactRepresentation: MouseArea {
        id: compact

        implicitWidth: pillRow.implicitWidth + 18
        implicitHeight: 28
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

        Rectangle {
            anchors.fill: parent
            radius: height / 2
            color: Qt.rgba(Kirigami.Theme.backgroundColor.r,
                           Kirigami.Theme.backgroundColor.g,
                           Kirigami.Theme.backgroundColor.b, 0.72)
            border.width: 1
            border.color: Qt.rgba(root.accentTone.r, root.accentTone.g,
                                  root.accentTone.b, 0.35)

            RowLayout {
                id: pillRow

                anchors.fill: parent
                anchors.leftMargin: 8
                anchors.rightMargin: 9
                spacing: 6

                Kirigami.Icon {
                    source: "audio-x-generic-symbolic"
                    implicitWidth: 16
                    implicitHeight: 16
                    Layout.alignment: Qt.AlignVCenter
                }

                Text {
                    Layout.fillWidth: true
                    Layout.maximumWidth: root.maximumTextWidth
                    text: root.displayText
                    color: Kirigami.Theme.textColor
                    elide: Text.ElideRight
                    font.pixelSize: Kirigami.Theme.defaultFont.pixelSize
                }

                Rectangle {
                    Layout.alignment: Qt.AlignVCenter
                    width: 6
                    height: 6
                    radius: 3
                    color: root.stateTone
                }
            }
        }
    }

    fullRepresentation: ColumnLayout {
        Layout.preferredWidth: 300
        Layout.preferredHeight: 336
        spacing: 8

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 78
            radius: 14
            color: Qt.rgba(Kirigami.Theme.alternateBackgroundColor.r,
                           Kirigami.Theme.alternateBackgroundColor.g,
                           Kirigami.Theme.alternateBackgroundColor.b, 0.65)
            border.width: 1
            border.color: Qt.rgba(root.accentTone.r, root.accentTone.g,
                                  root.accentTone.b, 0.2)

            RowLayout {
                id: cardRow

                anchors.fill: parent
                anchors.margins: 10
                spacing: 10

                Rectangle {
                    width: 44
                    height: 44
                    radius: 14
                    color: Qt.rgba(root.accentTone.r, root.accentTone.g,
                                   root.accentTone.b, 0.18)
                    border.width: 1
                    border.color: Qt.rgba(root.accentTone.r, root.accentTone.g,
                                          root.accentTone.b, 0.32)

                    Text {
                        anchors.centerIn: parent
                        text: root.monogram
                        color: root.accentTone
                        font.bold: true
                        font.pixelSize: 18
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 1

                    Text {
                        Layout.fillWidth: true
                        text: root.hasStation ? root.stationName : root.idleLabel
                        color: Kirigami.Theme.textColor
                        font.bold: true
                        font.pixelSize: Kirigami.Theme.defaultFont.pixelSize * 1.15
                        elide: Text.ElideRight
                    }

                    Text {
                        Layout.fillWidth: true
                        visible: text.length > 0
                        text: root.stationMeta
                        color: Kirigami.Theme.disabledTextColor
                        font.pixelSize: Kirigami.Theme.smallFont.pixelSize
                        elide: Text.ElideRight
                    }

                    Text {
                        Layout.fillWidth: true
                        visible: text.length > 0
                        text: root.stationTags
                        color: Kirigami.Theme.disabledTextColor
                        font.pixelSize: Kirigami.Theme.smallFont.pixelSize
                        elide: Text.ElideRight
                    }
                }

                PlasmaComponents3.ToolButton {
                    Layout.alignment: Qt.AlignVCenter
                    implicitWidth: 32
                    implicitHeight: 32
                    enabled: root.hasStation
                    checkable: true
                    checked: root.isFavorite
                    icon.name: "emblem-favorite-symbolic"
                    onClicked: root.runCommand("favorite")
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 4

            PlasmaComponents3.ToolButton {
                Layout.fillWidth: true
                icon.name: "media-skip-backward-symbolic"
                enabled: root.status.can ? root.status.can.previous : false
                onClicked: root.runCommand("previous")
            }

            PlasmaComponents3.ToolButton {
                Layout.fillWidth: true
                icon.name: root.playbackState === "playing"
                    ? "media-playback-pause-symbolic"
                    : "media-playback-start-symbolic"
                enabled: root.hasStation
                onClicked: root.runCommand("playpause")
            }

            PlasmaComponents3.ToolButton {
                Layout.fillWidth: true
                icon.name: "media-playback-stop-symbolic"
                enabled: root.hasStation
                onClicked: root.runCommand("stop")
            }

            PlasmaComponents3.ToolButton {
                Layout.fillWidth: true
                icon.name: "media-skip-forward-symbolic"
                enabled: root.status.can ? root.status.can.next : false
                onClicked: root.runCommand("next")
            }

            PlasmaComponents3.ToolButton {
                Layout.fillWidth: true
                icon.name: "media-playlist-shuffle-symbolic"
                onClicked: root.runCommand("roulette")
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            Kirigami.Icon {
                source: "audio-volume-high-symbolic"
                implicitWidth: 16
                implicitHeight: 16
                Layout.alignment: Qt.AlignVCenter
            }

            PlasmaComponents3.Slider {
                id: volumeSlider

                Layout.fillWidth: true
                from: 0
                to: 100
                stepSize: 1
                value: root.volumePercent

                onMoved: root.pendingVolume = Math.round(value)
                onPressedChanged: {
                    if (!pressed && root.pendingVolume >= 0) {
                        root.runCommand("volume " + (root.pendingVolume / 100).toFixed(2))
                        root.pendingVolume = -1
                    }
                }

                Component.onCompleted: root.volumeControl = volumeSlider
                Component.onDestruction: {
                    if (root.volumeControl === volumeSlider)
                        root.volumeControl = null
                }
            }

            Text {
                Layout.preferredWidth: 34
                text: root.volumePercent + "%"
                color: Kirigami.Theme.textColor
                horizontalAlignment: Text.AlignRight
                font.pixelSize: Kirigami.Theme.smallFont.pixelSize
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 4

            Text {
                Layout.maximumWidth: 74
                text: root.uiText("theme")
                color: Kirigami.Theme.disabledTextColor
                elide: Text.ElideRight
                font.pixelSize: Kirigami.Theme.smallFont.pixelSize
            }

            Repeater {
                model: ["system", "dark", "light"]

                delegate: PlasmaComponents3.ToolButton {
                    Layout.fillWidth: true
                    text: root.uiText("theme_" + modelData)
                    checkable: true
                    checked: root.themeMode === modelData
                    font.pixelSize: Kirigami.Theme.smallFont.pixelSize
                    onClicked: root.runCommand("theme " + modelData)
                }
            }
        }

        Item {
            Layout.fillHeight: true
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 6

            PlasmaComponents3.Button {
                Layout.fillWidth: true
                text: "OrbitFM"
                icon.name: "multimedia-player"
                onClicked: root.runCommand("panel")
            }

            PlasmaComponents3.ToolButton {
                icon.name: root.status.window && root.status.window.visible
                    ? "view-restore-symbolic"
                    : "view-fullscreen-symbolic"
                onClicked: root.runCommand("toggle")
            }

            PlasmaComponents3.ToolButton {
                icon.name: "preferences-system-symbolic"
                onClicked: root.runCommand("settings")
            }
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
                if (root.volumeControl && !root.volumeControl.pressed)
                    root.volumeControl.value = root.volumePercent
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

    Component.onCompleted: pollSource.connectSource(root.statusCommand, 2000)
    Component.onDestruction: pollSource.disconnectSource(root.statusCommand)
}
