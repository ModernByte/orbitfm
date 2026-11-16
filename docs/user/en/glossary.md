# Glossary

**English** · [Русский](../ru/glossary.md)

← [Home](index.md)

Project terms in plain language, with a note of where they appear: in the interface (UI),
in commands (CLI), in files or in the code. This is about **what** a thing is and how to use it.

## Window and modes

| Term | In plain words | Where it appears |
|---|---|---|
| **Focus** | The full "orbital desk" layout: 1100×800 (minimum 720×480) — a large globe, the station panel, the transport, the visualizer and the audio ring. This is the default mode on every launch. | UI: Settings → Window → Layout; CLI: `orbitfm mode focus` |
| **Overlay** | A compact 500×620 capsule (minimum 360×480) with no frame or shadow: the HUD row with a 16-bar visualizer, a mini globe, favorites (up to 8 stations), volume, Play/Stop and the Open Focus layout button. | UI: the layout switcher; CLI: `orbitfm overlay` (a toggle for the CLI and scripts) or `orbitfm mode overlay`; window title `OrbitFM · Overlay` |
| **Panel** | A drop-down panel 400×640 (minimum 360×480) with no frame or shadow: a header with search, modes, volume and a 16-bar strip, the Open Focus layout button and a gear, the station list, and the transport footer. This is the surface of a bar widget, not a desk layout. | Clicking the Noctalia bar widget; CLI: `orbitfm panel` (a toggle) or `orbitfm mode panel`; window title `OrbitFM · Panel` |
| **Listening** | The densest mode: a single "now playing" row 420×152 (minimum 360×120) with the broadcast pulse. Settings do not fit in it — they open in Overlay. | UI: Layout; CLI: `orbitfm mode listening` |
| **WindowMode** | The programmatic name of a window mode: Focus / Overlay / Listening / Panel. In the interface, Layout offers the first three; Panel opens with the `orbitfm panel` command or a click on the bar widget. The mode is not saved in `settings.json`. | UI: the Window category; CLI: `orbitfm mode`, `orbitfm panel`; code: `WindowMode` |
| **LayoutDensity** | Content density tied to the mode: `Full` (Focus), `Compact` (Overlay), `Minimal` (Listening), `Panel` (Panel). It decides which zones are drawn in the frame at all. | Code: `src/ui/layout.rs`; UI indirectly — through Layout |
| **Orbital Desk** | The "orbital desk" — the whole Focus composition: the globe as the bottom layer and glass zones (HUD, panel, transport) on top of it. | UI: Focus |
| **hide ≠ quit** | Closing the window with the X only hides it: the process, playback, the tray and commands keep working. A real exit is Quit in the tray or `orbitfm quit`. On macOS the X and `Cmd-W` hide the window through a cancelled `close_requested`, while `Cmd-Q` terminates the process. | UI: Hide window; CLI: `orbitfm hide`, `orbitfm quit` |
| **park, parked loop** | The application idling without a window: the loop waits for IPC commands, MPRIS, tray commands, audio events and the background roulette's reply, executes them and wakes the window if `show` was requested. On macOS it is not used normally: a hidden viewport keeps running the run loop, so Now Playing and media keys are delivered; park remains an emergency path. | Code: `Session::park` in `src/session/park.rs` |

## Interface

| Term | In plain words | Where it appears |
|---|---|---|
| **settings window** | A separate Settings window (not a desk modal): three automatic sizes — Compact 420×560, Medium 760×544, Wide 1040×720; the variant is in the title for WM rules. Categories on the left: Appearance, Language, Audio, Motion & accessibility, Window, Statistics, Integrations, About; on the right, the cards of the selected category. | UI: the gear, `Ctrl+,` (macOS — `Cmd+,`), `Esc` — close |
| **first-launch wizard** | The modal "Welcome to OrbitFM" wizard with three steps: language and theme, your desktop, integrations. It is shown only in a clean profile — when `settings.json` does not exist yet; Skip saves a marker and never shows it again. For Hyprland it can apply window rules to the current session over IPC, for niri it can copy a ready-made KDL block; it does not edit other people's configs. | UI: first launch; file: `onboarding_done` in `settings.json` |
| **built-in fonts** | Four Noto fonts inside the binary (~3.5 MiB): Latin with Cyrillic, Arabic, a CJK subset with the interface strings, and a subset of flag emoji. They are attached with the lowest priority and fill in only missing glyphs; Hindi (Devanagari) stays a system font. | UI: all interface languages; code: `src/ui/fonts.rs`, `assets/fonts/` |
| **scrim** | A dimming layer under the first-launch wizard modal and in the embedded fallback, when the backend cannot do separate windows. The regular Settings window has no scrim — it is a surface of its own. | UI: first launch and the fallback |
| **HUD** | The top row of the desk: the Global Internet Radio subtitle, the Search stations… station search, Search country… and the Country / Genre switch. | UI: Focus and Overlay; `Enter` in the field performs the search |
| **toast** | A pop-up notification: it stays for 3 seconds and fades on its own (180 ms in, 200 ms out), the stack holds no more than three layers, and repeating the same text adds no layer. This is how the UI shows a Notice. | UI: over the scene; code: `src/ui/toast.rs` |
| **Notice** | A typed core message for the user: the core names only the reason (`RouletteStarted`, `PlaybackFailed`, `HistoryCleared`…), and the presentation turns it into a catalog string in the current language. | Code: `src/session/notice.rs`; UI — as a toast |
| **station marker** | A dot on the globe at the station's coordinates; a left click on it starts the station and highlights and scrolls its row in the list. Markers are built from the same response as the list: stations with invalid or zero coordinates stay in the list without a marker, so the globe shows a subset of the rows rather than a separate result set. | UI: the globe in Focus; a worldwide genre search |
| **heatmap, country map** | A fill on every country by the selected metric: density is computed from Radio Browser's global counters, while genres, quality, type and rating come from the panel sample or the world top. A rainbow legend with Low and High marks and a source caption explains the scale. | UI: the globe and the legend; README: "heatmap fill" |
| **fly-to** | A soft camera flight to a point: a click on a country turns the globe, the roulette flies to the random station's country. | UI: globe gestures; code: `GlobeCamera::fly_to` |
| **orbit with inertia** | Rotating the globe by right-dragging: the camera keeps spinning on inertia. The wheel zooms, a left click picks a country or a station. | UI: gestures; Settings → Show mouse controls |

## Geo: how a click becomes a country

| Term | In plain words | Where it appears |
|---|---|---|
| **raycasting** | A ray from the cursor position through the inverse camera matrix crosses the sphere, and the intersection point is converted to latitude and longitude. That is how the application knows where you clicked. | Code: `src/geo/`; UI: a click on the globe |
| **bounding box** | The bounding rectangle of a country. First, a country whose rectangle the ray touched becomes a candidate — this is a cheap check. | Code: `src/geo/`, the index in the R-Tree |
| **R-Tree** | A spatial index of countries by bounding box: it quickly discards the obviously unsuitable ones, leaving a handful of candidates. It is built in advance from the embedded borders, without a network. | Code: `GeoService` with a prebuilt R-Tree (`rstar`) |
| **Point-in-Polygon** | The exact check: whether the point lies inside the country's polygon (`geo::contains`) — after the R-Tree filter. Polygons crossing the antimeridian are normalized in advance. | Code: `src/geo/`; UI: picking a country with a click |

## Audio motion

| Term | In plain words | Where it appears |
|---|---|---|
| **reactivity** | How much the broadcast deforms the globe: Off, Low, Normal, High (Normal by default). In Overlay the ceiling is Low. | UI: Motion & accessibility → Reactivity; file: `motion` in `settings.json` |
| **Reduce motion** | The master accessibility toggle: it removes squash, ripple and camera sway and cuts the interface transitions (the final state appears at once instead of an animation). It does not touch the sound or the Listening row's pulse: the pulse comes from audio analysis, which depends on the visualizer and reactivity. | UI: Motion & accessibility; file: `reduce_motion` |
| **spring-squash** | A springy squash-and-stretch of the globe on the beat while preserving volume: the amplitude is limited to fractions of a unit, and after Stop the target smoothly returns. | UI: the globe in Focus; code: `MotionMixer` |
| **onset, beat** | An onset is the moment a beat begins, found from the bass energy relative to its baseline; the beat phase is an accumulator 0..1 reset on each onset. They feed the motion styles and the flips. | UI: the motion Style; code: `src/render/motion.rs` |
| **camera flips** | A rare musical roll: a flip starts only on a strong onset and only after a pause in motion. The Allow flips checkbox is off by default and unavailable with Off or Reduce motion. | UI: Motion & accessibility; file: `allow_flips` |
| **audio ring** | An orbital ring around the globe that lives off the broadcast. It is drawn only in Focus, only with the visualizer on, and only while the station is playing, buffering or reconnecting. | UI: Focus |
| **cava-like visualizer** | Spectrum bars in the style of terminal cava: decaying bars above the current levels. | UI: Focus (the bar count is Bars 16/24/32), the transport and 16 fixed bars in the HUD row of the Overlay capsule; switched on with Appearance → Visualizer |
| **bar (16/24/32)** | The number of visualizer bars: Bars → 16, 24 or 32. A value outside the list is brought to the nearest allowed one when the file is loaded. | UI: Appearance → Bars; file: `viz_bars` |
| **three-band analysis** | Bass / Mid / High levels (20–150, 150–2000, 2000–20000 Hz). They are computed from a copy of the signal inside one libmpv lavfi graph, so the output sound matches the source. | UI: reactivity and motion styles; code: the `af` filter `@spectrum` |
| **PCM-tap** | Tapping the already decoded PCM with a separate consumer — the usual way to count bars. OrbitFM deliberately does not use it: the analysis runs on a copy of the signal inside one libmpv lavfi graph. | Absent in the code |

## Sound and the station catalog

| Term | In plain words | Where it appears |
|---|---|---|
| **libmpv** | The audio core: playback, volume, state events, reconnect. The native build needs a system `libmpv`; the AppImage/MSI/DMG carry it along. | Code: `src/audio/`; UI: the transport |
| **reconnect** | Reconnecting after the stream drops: the station does not fall over but tries to come back; when the attempts are exhausted, it stops. | UI: state; CLI: `orbitfm status` → `playing`, `buffering`, `reconnecting`, `error` |
| **offline snapshot** | A local cache of up to 500 stations with coordinates, written after a successful worldwide request and read before the network at startup. While Radio Browser is unreachable, the globe, the counters and the country/genre/search lists are built from it; the legend carries a "snapshot" mark. The file is compact, capped at 512 KiB, lives up to 24 hours without an update and is cleared by a button in About. | File: `$XDG_DATA_HOME/orbitfm/cache/stations-snapshot.json` |
| **economy mode, ECO** | A mode for a bad network: after the second reconnect or three seconds of buffering, libmpv gets a larger buffer (256 MiB, 60 s), and OrbitFM looks once per station for a duplicate at a lower bitrate. A successful swap is marked with `ECO` next to the bitrate and an Economy stream toast; choosing another station clears the mark. | UI: the transport, the toast; CLI: `orbitfm status` |
| **sleep timer** | A one-off countdown after which playback stops. It is set in Settings (Audio → 15/30/45/60/90 min) or with the `orbitfm sleep <1–720>` command; it lives in the session, so it works with the window closed but is reset by `orbitfm quit`. The remainder is exposed as the `sleep` field in `status --json`. | UI: Settings → Audio; CLI: `orbitfm sleep`, `orbitfm sleep clear` |
| **queue and wrap** | "Queue" is the currently visible station list that Next/Previous step through; there is no permanent playlist. `queue_wrap` (on by default) closes the edges: from the last station to the first. With wrap off, stepping past the edge does nothing, and the buttons and `can` in the status go dim. | UI: Settings → Audio → Loop queue; file: `settings.json` |
| **network profile** | The network behavior mode: Unlimited — the full catalog, a 1000-station snapshot and 8 icon downloads; Metered — `bitrateMax=128` in discovery, a 250-station snapshot and 2 icon downloads; Offline-first — background loading is off, the map and covers live off the offline snapshot, and the network is used only on a user action. Playback and the library are not limited by the profile. | UI: Settings → Audio → Network; CLI: `orbitfm network`; file: `settings.json` |
| **listening statistics** | Local minute counters: only time in the active `Playing` state of the active station counts, so pause, buffering, reconnect, error and stop add no seconds. Accumulation continues with the window closed, and data is written to disk at a 30-second threshold and on boundary events (leaving playback, switching the station, turning counting off, clearing, process exit). The limits are 500 stations and 730 UTC days; an external stream gets a separate station under the synthetic key `external:<address hash>`, and the address itself is never written to the file. Counting is on by default and is turned off with a toggle, but the file is not deleted when it is off — only clearing erases it. No server, no account and no telemetry; the counters are not part of the library export. | UI: Settings → Statistics; CLI: `orbitfm stats`, `stats --json`, `stats clear`; file: `stats.json` |
| **Radio Browser** | The open catalog of internet stations — the source of names, countries, tags, bitrate, coordinates and favicons. The station list needs a network; the country borders do not. | CLI/file: station data |
| **server mirror** | One of the api.radio-browser.info mirrors. On network errors and `5xx`, `429`, `408` responses the player tries the mirrors in turn, so "the list did not load" is usually temporary. | Code: `src/services/radio_browser.rs` |
| **DNS discovery** | Finding the mirrors through the `all.api.radio-browser.info` DNS records at startup — instead of one hard-coded server. | Code: discovery + `tokio` DNS; README: "servers are found over DNS" |
| **click-tracking** | A message to the catalog that the station has started playing (a `/json/url/<stationuuid>` request), so that listening statistics stay honest. Sent when playback starts. | Code: `send_click`; UI: invisible |
| **stationuuid** | The unique station identifier in Radio Browser. Favorites, history and launching by command are tied to it. | CLI: `orbitfm play <stationuuid>`, visible in `orbitfm status`; file: `library.json` |
| **favicon** | The station icon — also the cover in system media metadata. It loads asynchronously into its own cache and only after the address is checked (see "artwork policy"); the original bytes are not stored, responses larger than 512 KiB and images with a side over 512 px are rejected, and the cache holds no more than 256 entries and eight downloads at a time (two in the Metered profile, none in Offline-first). If it fails the check, a procedural cover is drawn instead of a grey square, and the sound is unaffected. | UI: the station row, MPRIS/SMTC/Now Playing |
| **procedural cover** | An abstract 64×64 picture assembled deterministically from the station's UUID, genre tags, country, votes and bitrate. It lives in the same cache as favicons, never goes to the network and does not replace a real logo: if the favicon passes the check, that is what appears. A pulse is drawn on top of the finished texture and is switched off together with motion. | UI: the station row, the transport, the capsules; code: `src/ui/procedural.rs` |
| **artwork policy** | Two checks before a cover loads: **tier 1** — syntactic, no network (scheme, private and service addresses, `localhost`, `*.local`, `*.internal`, single-label hosts, userinfo), and **tier 2** — against the actual DNS addresses: all of them must be public, every redirect is checked again, and only `https` works by default (HTTP is an explicit owner toggle). The same gate decides whether the address reaches MPRIS/SMTC/Now Playing: the tier 2 verdict is computed in the background and cached for 60 seconds. It is needed because `favicon` comes from someone else's catalog and its content goes into the image decoder. | Code: `src/services/ssrf.rs`, `src/services/artwork.rs`, `src/session/publish_gate.rs` |
| **SSRF** | Substituting an "internal" address: a link like `http://169.254.169.254/` or `http://127.0.0.1/` formally looks like an ordinary HTTP address but leads to cloud metadata, your router or the machine itself. OrbitFM rejects such targets before connecting (tier 1) and again after the resolve (tier 2). | Term from audits; the protection is the artwork policy |

## System integrations

| Term | In plain words | Where it appears |
|---|---|---|
| **SNI, StatusNotifierItem** | The tray icon protocol for Linux. It needs an SNI host — Noctalia, KDE, Hyprland, Sway/waybar; in GNOME, an extension such as AppIndicator. Without a host there is no icon, and that is a normal degradation. On macOS the icon lives in the menu bar through a separate backend, and on Windows there is no tray at all. | UI: the tray; CLI: control stays available |
| **dbusmenu** | `com.canonical.dbusmenu` — the tray icon's context menu: Play/Pause, Stop, Previous, Next, Radio Roulette, Volume Up, Volume Down, Settings…, Quit, with "Now playing: …" as the first line. | UI: the tray menu; code: `src/tray/` |
| **MPRIS** | The D-Bus media player interface on Linux: the name `org.mpris.MediaPlayer2.OrbitFM`, so the player is visible to `playerctl`, media keys and system widgets — including with the window hidden. | CLI: `playerctl -p OrbitFM play`; UI: widgets |
| **SMTC** | System Media Transport Controls on Windows — the same controls and metadata, through a separate hidden owner window. The runtime check has not been performed yet. | OS: Windows; the integration is built and verified by the build |
| **Now Playing** | The current station's metadata in macOS (menu bar and widgets): the station name, "state, country" or tags in the artist field, the favicon cover. Since 0.13.0-rc.1 a hidden window does not park the run loop, so Now Playing and media keys keep controlling the player; a live check on a real Mac is still ahead. | OS: macOS |
| **status --json** | A machine-readable state snapshot over IPC for scripts, widgets and shell plugins (schema 1): `state`, `station` (or `null`), `volume`, `window`, `theme`, `language`, `network`, `can`, `sleep` (an active timer or `null`) and the localized `ui` labels. New fields are added without changing `schema`. Inside, `station.favicon` is the raw catalog value: the snapshot describes the station rather than authorizing a download, so a client that intends to use it applies the artwork policy itself. | CLI: `orbitfm status --json` |
| **CST config edit** | Editing the Waybar JSONC tree while keeping comments and formatting. Not regular expressions and not a rewrite through `serde_json`: the result is parsed again before writing, the first copy of the file stays in `config.jsonc.orbitfm.bak`, and an atomic write protects against an interruption. Only `modules-right` is edited; symlinks are left alone. | UI: Integrations → Waybar → Enable in config; code: `src/integration/waybar.rs` |
| **native host** | The bridge between the browser extension and OrbitFM: a manifest in the browser profile directory with an absolute path to the binary, plus the host script itself. It is installed together with the Chrome/Firefox target; the browser reads manifests at startup, so it must be restarted after installation. | UI: Integrations → Browsers; browser profile files |
| **bar reload** | Refreshing the panel without `killall`/`pkill`: first `waybar-msg cmd reload` or `polybar-msg cmd restart`, then a targeted `SIGUSR2` only to the `waybar` process found in `/proc`; if both IPC and the signal are unavailable, the application honestly asks you to restart the bar by hand. | UI: Integrations → Reload; code: `src/integration/reload.rs` |

## Process, files, paths

| Term | In plain words | Where it appears |
|---|---|---|
| **single-instance** | One process. Running the binary again does not start a second instance but sends a command to the live one: `orbitfm toggle`, for example, simply shows the window. | CLI: any `orbitfm …` launch; code: `src/ipc/` |
| **IPC socket** | The transport for CLI commands: `$XDG_RUNTIME_DIR/orbitfm.sock` on Linux, `\\.\pipe\orbitfm-<profile hash>` on Windows, `$TMPDIR/orbitfm-<uid>/orbitfm.sock` on macOS. Commands: `toggle`, `show`, `hide`, `status`, `status --json`, `events`, `mode`, `play`, `playpause`, `next`, `previous`, `roulette`, `stop`, `search`, `library`, `favorite`, `play-result`, `play-library`, `play-url`, `volume`, `volume-up`, `volume-down`, `sleep`, `network`, `library-export`, `library-import`, `stats`, `stats --json`, `stats clear`, `theme`, `settings`, `panel`, `overlay`, `quit`. | CLI: `orbitfm volume 0.4`; code: `src/ipc/` |
| **settings.json** | Look and behavior: theme, accent, language, `viz_bars`, volume, `visualizer_enabled`, `artwork_allow_http`, `help_visible`, `onboarding_done`, reactivity, motion style, `allow_flips`, `reduce_motion`, `queue_wrap`, `network_profile`, `stats_enabled`. The window mode and the sleep timer are not there. Saved automatically. | File: `$XDG_CONFIG_HOME/orbitfm/settings.json` |
| **library.json** | Favorites and history (the last 20 stations). It opens without a network: this is your data, not a catalog cache. It is moved between installations with the `library-export`/`library-import` commands in JSON schema 1. | File: `$XDG_DATA_HOME/orbitfm/library.json` |
| **stats.json** | Local listening statistics: schema 1 JSON (`schema`, `updated_at`, `stations[]`, `days` by UTC day). Written atomically via `stats.json.tmp` → `rename`; a missing file means empty statistics, and an unparsable one stays untouched until the first successful write. It is not moved and is not part of the library export. | File: `$XDG_DATA_HOME/orbitfm/stats.json`; CLI: `orbitfm stats` |
| **XDG paths** | The file placement logic is the same on every OS: first `$XDG_CONFIG_HOME`, `$XDG_DATA_HOME`, `$XDG_RUNTIME_DIR`, otherwise the home directory. | Files and the socket; CLI: `orbitfm status` |

## Project terms

| Term | In plain words | Where it appears |
|---|---|---|
| **0.x pre-release** | Before 1.0.0, versions in the `0.x` line: the application already works, but individual interface details and file formats may change without a compatibility promise. | File: `orbitfm --version`, README: pre-release status |

## See also

- [Window modes](window-modes.md) — Focus / Overlay / Listening / Panel with their sizes
- [Settings and language](settings.md) — every category of the Settings window
- [Tray and MPRIS](tray-and-mpris.md) — SNI, dbusmenu and life with the window closed
- [Use cases](use-cases.md) — the same terms in action
- [Troubleshooting](troubleshooting.md)

---

Back: [Use cases](use-cases.md)
