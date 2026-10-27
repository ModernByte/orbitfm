# OrbitFM

OrbitFM is a desktop radio player with an interactive 3D globe for browsing
stations by country. This plugin adds a compact bar widget that shows or hides
the player through the application's single-instance IPC.

## Plugin

| Field | Value |
| --- | --- |
| ID | `modernbyte/orbitfm` |
| Entries | Bar widget: `launcher` |

## Requirements

Install `orbitfm` and put the binary on `PATH`. The widget launches it on demand
with `orbitfm panel`, so the application must be available on the same machine.

Noctalia v5 with plugin API 3 or newer is required. On Linux the application
uses a Unix socket in `$XDG_RUNTIME_DIR` for single-instance IPC.

## Usage

Add the **OrbitFM** widget from the Add-widget picker. Clicking it opens a
borderless dropdown panel with search, the station list and transport controls;
clicking again hides it. A stopped player starts with the panel visible, and the
first click may take a moment while the application starts. The panel has no
title bar or window border — the compositor positions it next to the widget.
Full and compact layouts remain available through the same IPC:
`orbitfm mode focus`, `orbitfm mode overlay` and `orbitfm mode listening`.

## Window rules

The panel is a frameless 400×640 surface. On tiling compositors it must float,
otherwise it is stretched to the full tile. The repository ships ready rules for
Panel, Focus, Overlay and Listening in `wm-configs/orbitfm-niri.kdl` and
`wm-configs/orbitfm-hyprland.conf` (400×640, 920×660, 420×620 and 420×152). The
niri rules also anchor the Panel window to the top-right corner so it opens below
the bar next to the widget instead of the centre of the screen; adjust `x` if
your bar layout differs. The file can be included without touching the rest of
the config:

```kdl
include "./cfg/orbitfm.kdl"
```

## Notes

- The plugin contains no audio logic and keeps no player state; playback,
  favorites, history and appearance settings live in the OrbitFM process.
- It starts a local process and does not access the network itself.
- OrbitFM is single-instance: repeated clicks reuse the running process through
  the session IPC socket.
