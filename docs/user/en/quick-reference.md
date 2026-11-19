# Quick reference

**English** · [Русский](../ru/quick-reference.md)

← [Home](index.md)

Everything you reach for daily on one page: window modes, commands, gestures,
files and numbers. Each item links to the page that explains it in full.

## Window modes

| Mode | Size | How to open |
|---|---|---|
| Focus | 1100×800 | `orbitfm show`, `orbitfm mode focus`, the tray icon |
| Overlay | 500×620 | `orbitfm overlay`, `orbitfm mode overlay` |
| Listening | 420×152 | `orbitfm mode listening` |
| Panel | 400×640 | `orbitfm panel` — the drop-down for a bar widget |

Closing the window only hides it: playback, the tray, MPRIS and commands keep
running. `orbitfm quit` is the only real exit. Details — [Window modes](window-modes.md).

## Commands

Without arguments `orbitfm` works as `toggle`. Window commands create the
process if it is not running; playback, sleep, statistics and library commands
need an already running player.

| Task | Command |
|---|---|
| Show / hide / quit | `orbitfm show`, `orbitfm hide`, `orbitfm quit` |
| Switch layout | `orbitfm mode focus\|overlay\|listening\|panel`, `orbitfm mode` (ask) |
| Panel and capsule | `orbitfm panel`, `orbitfm overlay` |
| State for scripts | `orbitfm status`, `orbitfm status --json`, `orbitfm events` |
| Play a catalog station | `orbitfm play <uuid>` |
| Play an external stream | `orbitfm play-url <url>` — `.m3u`, `.pls`, a direct link or a page with audio |
| Transport | `orbitfm playpause`, `orbitfm next`, `orbitfm previous`, `orbitfm roulette`, `orbitfm stop` |
| Volume | `orbitfm volume 0.0–1.0`, `orbitfm volume-up`, `orbitfm volume-down` |
| Sleep timer | `orbitfm sleep <1–720>`, `orbitfm sleep clear` |
| Sleep behavior | `orbitfm sleep action nothing\|suspend\|power-off`, `orbitfm sleep fade 0\|60\|120\|180\|300`, `orbitfm sleep inhibit on\|off`, `orbitfm sleep warning on\|off` |
| Network profile | `orbitfm network unlimited\|metered\|offline-first` |
| Theme | `orbitfm theme system\|dark\|light` |
| Favorites | `orbitfm favorite`, `orbitfm favorite <uuid>` |
| Library transfer | `orbitfm library-export <file>`, `orbitfm library-import <file>` |
| Listening statistics | `orbitfm stats`, `orbitfm stats --json`, `orbitfm stats clear` |
| Manual station address | `orbitfm alias <uuid> [<url>\|remove]`, `orbitfm alias list` |
| Settings window | `orbitfm settings` |
| Integrations | `orbitfm integration status\|install\|uninstall <target>` |

The full list with explanations — [Controls and hotkeys](controls.md).

## Mouse

| Gesture | Result |
|---|---|
| Left click on a country | Select it: camera turn, highlight, station list |
| Left click on a marker | Start that station and scroll the list to its row |
| Right-button drag | Rotate the globe; a sharp flick keeps it spinning |
| Wheel | Zoom; over a slider — change the value by 5 % per notch |
| Hover over a country | Outline and a card with the flag, name and station count |
| Click outside countries | Clear the country selection |

## Keyboard

Keys work while the OrbitFM window is focused; there are no system-wide hotkeys.

| Key | Action |
|---|---|
| `Tab` / `Shift+Tab` | Move through controls |
| `↑` / `↓` | Move through the station list |
| `Space` or `Enter` | Start the selected station; `Space` with nothing selected — play/pause |
| `/` or `Ctrl+F` (`Cmd+F`) | Jump to the search field |
| `Enter` | Run the search, or pick the first suggestion |
| `Esc` | Close Settings, then the mouse-controls card |
| `Ctrl+,` (`Cmd+,`) | Open or close Settings |

Media keys and the system player widget work through MPRIS — [Tray and MPRIS](tray-and-mpris.md).

## Where the files are

| Path | What |
|---|---|
| `~/.config/orbitfm/settings.json` | Theme, language, volume, motion, sleep behavior, network profile |
| `~/.local/share/orbitfm/library.json` | Favorites and history |
| `~/.local/share/orbitfm/stats.json` | Listening minutes and top lists |
| `~/.local/share/orbitfm/cache/stations-snapshot.json` | Offline map snapshot |
| `~/.local/share/orbitfm/integrations/<target>.json` | Receipts of installed integrations |
| `$XDG_RUNTIME_DIR/orbitfm.sock` | Local command channel (mode `0600`) |
| `~/.local/bin/orbitfm` | AppImage installed by the one-command installer |

On macOS the socket lives in `$TMPDIR/orbitfm-<uid>/`, on Windows commands go
through the pipe `\\.\pipe\orbitfm-<profile hash>`. What is stored and what goes
online — [Security and privacy](privacy.md).

## Numbers and defaults

| Value | Meaning |
|---|---|
| 17 | Interface languages: “System” plus 16 |
| 70 % | Default volume; wheel and arrows step by 5 %, `volume-up`/`volume-down` by 10 % |
| 16 / 24 / 32 | Visualizer bars; 24 by default |
| 1–720 min | Sleep timer from the command line; in the interface 15 / 30 / 45 / 60 / 90 |
| 0–300 s | Fade before the stop; 300 s by default |
| 500 | Stations in the offline snapshot |
| 1000 / 250 | World sample for the map: unlimited / metered profile |
| 800 | Coordinate stations loaded into the country map layer |
| 100 / 200 / 100 | Catalog page: country / genre / name search |
| 20 / 8 | History entries / favorite stations in the Overlay capsule |
| 10 | Stations, countries and genres in the statistics top lists |

## See also

- [Installation](install.md) and [first launch](first-launch.md)
- [Settings and language](settings.md)
- [Troubleshooting](troubleshooting.md)
- [Glossary](glossary.md)
