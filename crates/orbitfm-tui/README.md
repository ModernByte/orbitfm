# orbitfm-tui

A lightweight terminal client for the **OrbitFM** internet radio player.

`orbitfm-tui` does **not** contain an audio engine. It is an IPC client to a
running `orbitfm` core: the core owns libmpv, MPRIS, the tray, the Radio Browser
catalogue and `library.json`. The TUI only reads state and sends commands over
the public CLI/IPC contract.

## Install

```bash
cargo install --path crates/orbitfm-tui
```

A running `orbitfm` core is required in `PATH`, or set `ORBITFM_BIN` to its
path. The TUI never starts the core and never opens a GUI.

## Usage

```bash
orbitfm-tui            # start the client
orbitfm-tui --version  # dynamic version: semver + git + build date
orbitfm-tui --help
```

Keys:

| Key | Action |
|-----|--------|
| `j`/`k`, `↑`/`↓` | move in a list |
| `g`/`G` | first / last |
| `enter` | play the selected station |
| `p` / `space` | play / pause |
| `s` | stop |
| `[` / `]` | previous / next |
| `r` | radio roulette |
| `+` / `-` | volume |
| `/` | search |
| `f` / `h` | favorites / history |
| `a` | toggle favorite |
| `t` / `T` | sleep timer / clear |
| `v` / `V` | next visualizer / on-off |
| `l` | next layout preset |
| `M` | motion: full → reduced → off |
| `Ctrl+R` | reset layout to `desk` |
| `R` | reconnect to the core |
| `?` | help |
| `q` | quit the TUI (the radio keeps playing) |
| `Q` | quit the core (with confirmation) |

`q` never stops playback; the footer says so. `Q` asks for `y`/`n` first.

## Configuration

`$XDG_CONFIG_HOME/orbitfm/tui.toml` (fallback `~/.config/orbitfm/tui.toml`),
written atomically with a 500 ms debounce. The core never reads it.

```toml
schema = 1

[ui]
layout = "desk"        # desk|focus|library|minimal|dashboard
motion = "full"        # full|reduced|off
glyphs = "auto"        # auto|unicode|ascii
border = "rounded"     # rounded|square|none
theme  = "orbit-dark"  # orbit-dark|ice|amber|green|magenta|mono
frame_fps = 15

[visualizer]
kind = "bars"          # bars|ring|scope|pulse
bars = 32
```

Unknown fields are ignored, invalid values are clamped, and a broken file falls
back to defaults without being overwritten.

## Versioning

The version is dynamic: the semver base lives in `Cargo.toml`, while the git
state (`sha`, `sha-dirty`) and build date (`SOURCE_DATE_EPOCH`) are embedded at
build time. `--version` also reports core compatibility (`ev: 1`,
`status.schema: 1`, core `>= 0.18.0-rc.1`). See `docs/tui/versioning.md`.

## Compatibility

The TUI is an independent binary. It requires a core that speaks `ev: 1` and
`status.schema: 1`. Unknown frames are ignored; an incompatible core shows a
clear error and blocks command handling.

## License

MIT — see the repository `LICENSE`.
