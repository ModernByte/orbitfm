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
with `orbitfm toggle`, so the application must be available on the same machine.

Noctalia v5 with plugin API 3 or newer is required. On Linux the application
uses a Unix socket in `$XDG_RUNTIME_DIR` for single-instance IPC.

## Usage

Add the **OrbitFM** widget from the Add-widget picker. Clicking it runs
`orbitfm toggle`: a running player appears or hides, and a stopped player starts
with a visible window. The first click may take a moment while the application
starts.

## Notes

- The plugin contains no audio logic and keeps no player state; playback,
  favorites, history and appearance settings live in the OrbitFM process.
- It starts a local process and does not access the network itself.
- OrbitFM is single-instance: repeated clicks reuse the running process through
  the session IPC socket.
