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
with `orbitfm overlay`, so the application must be available on the same machine.

Noctalia v5 with plugin API 3 or newer is required. On Linux the application
uses a Unix socket in `$XDG_RUNTIME_DIR` for single-instance IPC.

## Usage

Add the **OrbitFM** widget from the Add-widget picker. Clicking it opens the
borderless Overlay capsule — now playing with a 16-column visualizer strip, the
mini globe, favorites, volume and play/stop; clicking again hides it. A stopped
player starts with the capsule visible, and the first click may take a moment
while the application starts. The capsule has no title bar, border or shadow —
only the Focus desk keeps window decorations; the compositor positions the
capsule next to the widget. Other surfaces remain available through the same
IPC: `orbitfm mode focus`, `orbitfm mode listening` and, from the command line
only, `orbitfm panel` for the dropdown panel.

## Window rules

The capsule is a frameless 500×620 window. On tiling compositors it must float,
otherwise it is stretched to the full tile. The repository ships ready rules for
Focus, Overlay, Listening and Panel in `wm-configs/orbitfm-niri.kdl` and
`wm-configs/orbitfm-hyprland.conf` (1100×800, 500×620, 420×152 and 400×640); the
non-Focus rules also switch border and shadow off (`border { off; }` /
`shadow { off; }`, `noborder` / `noshadow`). The niri rules also anchor these
windows to the top-right corner so they open below
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
