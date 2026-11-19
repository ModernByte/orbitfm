# Troubleshooting

**English** · [Русский](../ru/troubleshooting.md)

← [Home](index.md)

This catalog is structured as **symptom → why → what to do**. Everything below describes the `main` branch after
OrbitFM 0.23.0-rc.3; if your build behaves differently, check the version first.

## Four commands that answer half the questions

```bash
pgrep -ax orbitfm     # is the process alive
orbitfm status        # state, station, volume, window, mode
orbitfm status --json # the same as machine-readable JSON
ls -l "$XDG_RUNTIME_DIR/orbitfm.sock"
```

`orbitfm status` answers with a set of lines; after `#` are the possible values, which are not present in the real output:

```text
status playing            # stopped | playing | paused | buffering | reconnecting | error
stationuuid 960cf833-…
name SomaFM Groove Salad (128k MP3)
volume 0.85
window visible            # visible | hidden
mode focus                # focus | overlay | listening | panel
```

The `stationuuid` and `name` lines disappear when the state is `stopped` or `error`. If the process is not
running, `toggle`, `show`, `settings`, `panel`, `overlay` and `mode <mode>` start a new instance, while any
other command (including `playpause`, `next`, `previous`, `roulette`, `volume-up`, `volume-down`, `theme`, `stats`, `alias`
and `status`) prints `OrbitFM не запущен` and returns code 1 — control commands do not start the GUI silently.

---

## 1. Launch and graphics

| Symptom | Why | What to do |
|---|---|---|
| `не удалось запустить GUI` | The window is drawn through wgpu: the graphics stack or the session is not working | Check that the Wayland/X11 session can open windows at all; try running under X11 (XWayland) |
| The background is black or grey instead of translucent "glass" | On Windows and macOS, wgpu transparency has known limitations | On Linux (Wayland) transparency works; on Windows/macOS this is known behaviour and does not affect functionality |
| Launching again does not create a new window | OrbitFM is a single process: a second launch sends a command to the existing one | This is normal; if you need a new process, run `orbitfm quit` first |
| Closed the window with the X button, but the music keeps playing | Closing the window = hide, not quit | `orbitfm show` or click the [panel widget](panels.md); to quit completely, `orbitfm quit` |
| macOS: `Cmd-W` hides the window, `Cmd-Q` quits | By design: the X button and `Cmd-W` hide it while Now Playing stays alive, and you quit only via `Cmd-Q`, the menu and `orbitfm quit` | To control the hidden window, use the menu bar, the media keys or `orbitfm show` |
| macOS: "The application cannot be opened… is not supported on this type of Mac" | The DMG was built for another architecture (arm64 on an Intel Mac), or the system is older than 13.0 | `uname -m`: `arm64` — [Apple Silicon](install-macos-apple-silicon.md), `x86_64` — [Intel](install-macos-intel.md); you need the matching DMG and macOS 13+ |
| macOS: the window is there but the globe is not — it is black or missing | The renderer did not initialize or the adapter could not meet the limits; live acceptance on Intel UHD Graphics 617 has not been done yet | Capture `orbitfm-render.log` (section 9) and attach it to the issue — that line shows which adapter was chosen |
| macOS: the app crashed immediately, there was no window | dyld could not build the bundle's dependency graph | `orbitfm --version` from a terminal; on abort or "Symbol not found" attach the output in full — the build gate `smoke-launch.sh` catches this class of defect |
| `bash: orbitfm: command not found` | The binary is not in `PATH` | Symlink it into `~/.local/bin/orbitfm`, check `echo $PATH`; see [Install](install.md) |
| AppImage: `Permission denied` or complaints about FUSE | The file is not executable, or `libfuse` is missing/blocked | Run the integration without root and without `chmod` on the AppImage itself: `bash install-appimage.sh ./OrbitFM-…AppImage` (the script from the release or `packaging/linux/`); manually — `chmod +x OrbitFM-…AppImage`, and if needed `APPIMAGE_EXTRACT_AND_RUN=1 ./OrbitFM-…AppImage` |
| The first-launch wizard did not appear | It is shown only in a clean profile: there is no `settings.json` and `onboarding_done` is not set; with an existing config the wizard does not run | With the process stopped, rename `~/.config/orbitfm/settings.json` and launch again; see [First launch](first-launch.md) |
| The "Set up window" button in the wizard reported an error | Hyprland did not answer `hyprctl keyword` (wrong compositor, IPC unavailable, session variables not passed through) | Copy the snippet from the wizard window and add it to `hyprland.conf` manually, or use the `orbitfm integration install hyprland` target — it writes `~/.config/hypr/orbitfm.conf` and applies the rules to the session; the rule without editing files works only on a live Hyprland session |
| The window rules disappeared after a Hyprland restart | The wizard applies rules only to the current session and does not edit configs | `orbitfm integration install hyprland` writes `~/.config/hypr/orbitfm.conf`; you add the line `source = ~/.config/hypr/orbitfm.conf` to your own `hyprland.conf` yourself. For niri — `orbitfm integration install niri`, which also appends the single `include` line |

## 2. Audio and libmpv

| Symptom | Why | What to do |
|---|---|---|
| Startup fails with `не удалось инициализировать аудио-ядро`, there is no sound | The native build needs the system `libmpv` | Debian/Ubuntu — `libmpv-dev`, Fedora — `mpv-libs-devel`, Arch — `mpv`. The AppImage, MSI and DMG carry `libmpv` with them |
| It plays, then goes silent, the status shows `reconnecting` | The stream drops and OrbitFM reconnects | Wait: the status will go back to `playing` by itself; `error` means the attempts are exhausted and playback has stopped |
| "Playback failed" on a particular station | Usually the broadcaster: a server that is down or broken, a geo-block, a stream that moved | Check a neighbouring station or "Radio Roulette": if everything except one station plays, the problem is on the broadcaster's side; if the catalog address is stale, a manual link helps — `orbitfm alias` (section 3) |
| The volume changes on the slider but "resets" after a restart | While dragging, the slider only changes the sound and remembers the value: it is written to the file on release, click or a keyboard step | Release the slider fully and check `volume` in `orbitfm status` |

## 3. Stations and Radio Browser

The station catalog is the external Radio Browser: first OrbitFM finds servers via DNS
(`all.api.radio-browser.info`), then tries mirrors on network errors and `5xx` /
`429` / `408` responses. There is no hard-wired single server in the code.

| Symptom | Why | What to do |
|---|---|---|
| "No stations found" | Most often the network or DNS, or an empty/too narrow query | If a snapshot already exists, the list may fill from it; otherwise check the network and name resolution (`getent hosts all.api.radio-browser.info`), change the query, try later |
| The globe is drawn, but there are no countries or stations in the list | Country borders are built in and drawn without the network; the list comes from Radio Browser, and if there is no network, from the local offline snapshot. There has not been a snapshot yet | The API response did not arrive, and there is no snapshot on disk: check the network/DNS; after the first successful launch the snapshot appears by itself (`~/.local/share/orbitfm/cache/stations-snapshot.json`) |
| The map legend says "snapshot" and the country counters are small | OrbitFM is showing the offline snapshot: it is not the whole database but up to 500 saved stations | This is normal degradation without a network; when Radio Browser responds, the map and counters are replaced with live data. More in [Use cases](use-cases.md) |
| The country/genre/search list is empty even though there is a network | The live response did not arrive or the query is too narrow; the offline snapshot covers only saved stations | Repeat the query, check DNS; the snapshot does not replace the full catalog, it only insures against an empty screen |
| Stations are found but do not start on click | An `error` response from the audio core | See section 2; start with `orbitfm status` |
| A station from favorites, history or a list stopped playing while neighbouring ones play | The catalog `url_resolved` has burned out: the broadcaster moved to another host, changed the port or dropped `.mp3` from the path; a Radio Browser entry takes days to fix, if it is fixed at all | Substitute a working address for the station manually: `orbitfm alias <uuid> <url>`. The station remains itself — the same `stationuuid`, favorites, history and statistics minutes. To check — `orbitfm alias <uuid>` and `orbitfm alias list`, to remove — `orbitfm alias <uuid> remove`; while the link is manual, the station row and the current-station card show the "manual link · host" mark. The `alias` commands are not available to the browser extension |
| A station marked "manual link" gives `error` again | The alias address burns out too; the app does not silently fall back to the catalog link, otherwise the point of a manual link would be lost | Check the address with the broadcaster and set a new one with the same `alias <uuid> <url>`; to go back to playing from the catalog — `orbitfm alias <uuid> remove` (the current broadcast finishes over the alias, the next launch will use `url_resolved`) |
| An `ECO` label appeared next to the bitrate | Economy mode turned on: the network cannot carry the stream, libmpv got a bigger buffer and OrbitFM found the same station at a lower bitrate | This is expected behaviour on a poor network. Pick another station — the label goes away; when the connection returns, it is easier to pick the original station manually |
| The sound after `ECO` is quieter or worse | The economy stream is deliberately at a lower bitrate | Pick a station with a higher bitrate or wait for the network to recover; there is no automatic switch back |
| Instead of artwork there is an abstract drawing, not the logo | The favicon is missing or failed the address check; the artwork cache draws a deterministic procedural cover from the genre, country and votes | Normal behaviour: sound and metadata are unaffected. The logo appears if the favicon passes the check; "Allow HTTP artwork" helps stations with an HTTP-only icon |
| Instead of artwork there are the initials of the title | The procedural cover has not been computed yet or the texture was evicted from the cache (256 entries, LRU) | Scroll the list — the cover is recreated; the reason the favicon was rejected is visible in `RUST_LOG=debug` |
| The artwork disappeared for a station that was playing with a picture | The image lives in a cache of up to 256 entries and 8 concurrent downloads; with a lot of scrolling old covers are evicted | Scroll the list back — the cover loads again |
| There are fewer stations and icons in the "Metered" profile | The network profile adds `bitrateMax=128` to catalog queries, shrinks the world snapshot to 250 stations and keeps at most two icon downloads | This is expected; switch back to "Unlimited" with `orbitfm network unlimited` or in the "Audio" category of Settings |
| In the "Offline-first" profile the map is empty or stale | The background world query is off; the map and covers come from the offline snapshot, and new icons are not requested | Start a search or a station — user actions still go to the network; for a live map, enable "Unlimited" |

### Importing external streams

`orbitfm play-url` parses `.m3u`/`.pls`, a direct link and an HTML page with audio.
The error is returned as a single line, without a stack trace or an error code; here is what the messages mean:

| Message | Why | What to do |
|---|---|---|
| `поддерживаются только http:// и https://` | The `file:`, `ftp:` and similar schemes are forbidden | Give an `http://` or `https://` address |
| `HLS-потоки (.m3u8) не поддерживаются` | HLS needs a separate engine; `.m3u8`, `application/vnd.apple.mpegurl` and a body starting with `#EXT-X-` are rejected | Take a direct audio stream (MP3/AAC/OGG) or a `.m3u`/`.pls` playlist that links to one |
| `страница или плейлист больше 1 МиБ` | The body is read with a hard cap of 1 MiB | Give a direct stream link instead of a heavy page |
| `аудиопоток не найден` | The page has no `og:audio`, `<audio src>` or `<source src>` with a suitable address | Open the page in a browser and copy the audio link |
| `плейлист не содержит воспроизводимых ссылок` | The `.m3u`/`.pls` contains only comments, local paths or broken entries | Check the file; at least one `http(s)` link is required |
| `слишком много редиректов` / `слишком много уровней плейлиста` / `цикл в плейлисте или странице` | Protection against endless redirects and self-referencing playlists | Give the final stream address |
| `превышен таймаут импорта` | The whole parse is limited to 8 seconds | Retry later or check that the server is reachable |
| `HTTP 404`, `HTTP 403` … | The server answered with an error | Check the link; geo-blocks and authorization are not bypassed |
| `сеть: …` | DNS, TLS or the connection | Check the network; local-network addresses are allowed on an explicit import |

Import is started only manually — by the command or by the "Play this page" button in the popup;
OrbitFM does not request external addresses by itself. The station gets the identifier
`url:…` and then behaves like a normal one: history, favorites, MPRIS.

### Manual station link

`orbitfm alias <uuid> <url>` substitutes the broadcast address of a catalog station:
the station remains itself — the same `stationuuid`, name, country, tags, place in favorites
and history, accumulated statistics minutes — but plays from the address you gave.
The substitution is single and applies to any launch: from lists, favorites, history,
the `next`/`previous` queue, roulette, `orbitfm play <uuid>`, MPRIS and the browser popup.
A rejected address answers `error <message>`, and the file does not change; here is what the
messages mean:

| Message | Why | What to do |
|---|---|---|
| `алиас отклонён: приватный или служебный адрес` | Tier 1 does not let through local-network, loopback, link-local and multicast addresses: an alias has none of the relaxations that an explicit `play-url` gets | Use a public broadcast address |
| `алиас отклонён: схема не http/https` | Only `http` and `https` are accepted | Give an `http://` or `https://` link |
| `алиас отклонён: одноярлыковый хост` / `локальный или служебный домен` | `http://radio/stream` and `localhost` are rejected by the same public-address check | Specify the full domain name of the stream |
| `алиас отклонён: адрес не разобран` / `хост не указан` / `порт 0` / `userinfo в ссылке` | The link was copied badly: it is not a URL at all, has no host, a zero port, or a username and password | Copy the address in full, from `http://` or `https://`, without credentials |
| `алиас отклонён: адрес длиннее 2048 байт` | The address length is limited to 2048 bytes | Keep only the direct stream link, without a long tail of parameters |
| `алиас отклонён: внешнему потоку алиас не нужен` | The `stationuuid` starts with `url:` — this is a synthetic `play-url` station, its address is itself | Fix the link with another `orbitfm play-url <address>` |
| `алиас отклонён: stationuuid пустой` / `адрес пустой` | One of the arguments came out empty (for example, `orbitfm alias "" https://…`) | Provide both the station UUID and the address; the UUID comes from the `stationuuid` line in `orbitfm status` or from `library.json` |
| `алиас отклонён: алиасов уже 200: снимите ненужный — alias <uuid> remove` | The ceiling is 200 entries; there is no eviction — silently losing a fixed station is worse than refusing | Remove unneeded ones with `orbitfm alias <uuid> remove`, then set the new one |
| `некорректный аргумент команды: alias: ожидается stationuuid или list` / `stationuuid не содержит пробелов` / `лишний аргумент: …` | The command did not parse: no UUID given, a space in it, or more arguments than needed (the words `list` and `remove` are distinguished by position) | The form is one of four: `alias <uuid>`, `alias <uuid> <url>`, `alias <uuid> remove`, `alias list`; the address contains no spaces |

The answer `alias none` is not an error: the station has no manual link (`alias list` answers the same
on an empty list). Removing a non-existent alias is idempotent:
`ok alias removed`, and `library.json` is not rewritten. An alias can also be set for a
station that is not currently in favorites, history or search results: it may come back
to the catalog later, and the link will already be waiting for it.

The easiest way to check that the substitution is in place is the "manual link · host" mark in the
station row and in the current-station card: a mismatch between the catalog and the broadcast must
be visible. The full address is not shown in the interface — only the host. Such a station will not
have an `ECO` label: the economy substitution changes the `stationuuid`, so no light stream is looked
up for alias stations. If the alias address dies too, that is an ordinary playback error (section 2):
the app does not silently fall back to the catalog link, a new `alias <uuid> <url>` or
`alias <uuid> remove` is needed.

Aliases live in the `aliases` field of `library.json` and stay only on your machine: they are
not in `library-export`/`library-import` (schema 1) or in `status --json`, and the `alias` command
is entirely forbidden to the browser native host. The `alias` commands are served by a running
instance; they do not start the GUI themselves.

## 4. Window, layouts and WM rules

Sizes: Focus **1100×800** (minimum 720×480, title `OrbitFM`), Overlay **500×620** (360×480,
`OrbitFM · Overlay`), Listening **420×152** (360×120, `OrbitFM · Listening`), Panel **400×640**
(360×480, `OrbitFM · Panel`), Settings **420×560 / 760×544 / 1040×720** (`OrbitFM · Settings ·
Compact|Medium|Wide`), the sleep timer warning **420×320** (`OrbitFM · Sleep Warning`).

Rules match the pair "`app-id` + title", and the app-id is the same for all windows — `org.modernbyte.OrbitFM`.

| Symptom | Why | What to do |
|---|---|---|
| Overlay/Listening/Panel stretch over the whole tile | The window rules for the tiling WM are not installed | Apply the installer target: `orbitfm integration install niri` (a rules file plus one `include` line, with a `config.kdl.orbitfm.bak` copy) or `orbitfm integration install hyprland` (the file `~/.config/hypr/orbitfm.conf`; you add the `source =` line yourself); with the button — on the "Settings → Integrations" page. See [Panels and widgets](panels.md) and [Window modes](window-modes.md) |
| The window appears in the middle of the screen instead of under the bar | Niri centers floating windows | In the niri rules the anchor is `top-right` and `y=48`; the `x` offset for your bar layout has to be tuned by hand |
| The rule does not apply | The `title` did not match: each layout has its own title, and it is a service string that is not translated | `NIRI_SOCKET=<socket> niri msg windows` — the `Tile size`, `Window size`, `Workspace-view position` fields; compare the actual size with the expected one |
| Settings or the warning window stretches over the whole tile | The window has an empty `App ID` — that is how it was in 0.21.0-rc.2/rc.3: child windows were born without a class, and a rule with a title did not find them | Update the build to **0.21.0-rc.4** or newer and reinstall the `niri`/`hyprland` target; to check — `niri msg windows`, the `App ID` field must be `org.modernbyte.OrbitFM` |
| Hyprland complains about the config after an update | `windowrulev2` is marked deprecated in 0.53+ (it works in 0.52 and in compatibility mode) | Migrate to `windowrule { … }`, consulting the Hyprland documentation |
| Switched layout — the window is still hidden | A hidden window also has a layout: it will be applied on the next show | `orbitfm show`; in the status the `mode` line comes after `window` |

### Integrations and editing configs

The "Settings → Integrations" page edits structurally, without regular expressions: Waybar comments and
formatting are preserved, and the first copy of the file stays next to it with the `.orbitfm.bak` suffix.
Exactly two places in other configs are edited: Waybar's `modules-right` via the "Enable in config" button
and one line `include "./cfg/orbitfm.kdl"` in `~/.config/niri/config.kdl` when the `niri` target is
installed (before the first edit — a copy `config.kdl.orbitfm.bak`). The other targets only lay out
their own files, and the installer does not edit `hyprland.conf`.
No `killall`/`pkill`: reloading goes through IPC, a signal or D-Bus, otherwise a
manual hint is shown. Details — [Integrations](integrations.md).

| Symptom | Why | What to do |
|---|---|---|
| "Enable in config" did not work | The Waybar config was not found, did not parse as JSONC, has no `modules-right`, or the file is a symbolic link (editing is not supported) | The status line shows the path and the reason; fix the config by hand, the backup is untouched. With a symlink, edit the target file yourself |
| The module was added but is not in the bar | Waybar did not re-read the config automatically | Press "Reload"; if IPC is unavailable, restart Waybar the usual way for your session (without `killall`/`pkill`) |
| Editing the config lost comments | This should not happen: the edit is structural | Restore `.orbitfm.bak` and report the bug — losing comments counts as a defect |
| I want to roll back the edit | The first copy of the original file is right next to it | Rename `config.jsonc.orbitfm.bak` back; the backup is created once and is not overwritten by later edits |
| Chrome/Firefox does not see the native host | The manifest is installed in the profile directory, and the browser reads such manifests at startup; for Firefox the extension ID also matters | Restart the browser; installation from the GUI writes an absolute path to `orbitfm`, so `PATH` is not needed here |
| The extension is installed but does not connect | The extension ID did not match or the manifest points to a removed binary | Reinstall the native host from "Settings → Integrations" and check the extension from the release |

## 5. Language, fonts and flags

| Symptom | Why | What to do |
|---|---|---|
| Country flags are not drawn | The built-in subset covers only regional indicators; if there are no flags at all, the build is damaged or the font did not load | Update OrbitFM; a full system **Noto Color Emoji** (if found) also provides the remaining emoji, but it is no longer required for flags |
| 中文 / 한국어 / 日本語 are drawn as tofu | The built-in CJK subset covers all catalog interface strings; empty squares are possible in **station names** with rare characters outside the subset | Install a system font with CJK (for example, Noto Sans CJK from your distribution) — it is picked up with higher priority |
| हिन्दी is drawn as squares | Devanagari is the only interface language that is not in the built-in set and is taken from the system | Install a font such as Noto Sans Devanagari; the other 16 languages are covered by the built-in fonts |
| العربية is drawn as squares | The built-in Noto Sans Arabic should cover Arabic script; squares mean the font did not load | Update OrbitFM and check `RUST_LOG=debug`; the system Noto Sans Arabic remains a fallback |
| The Arabic interface is not mirrored | egui 0.37 shapes Arabic text but does not do bidi mirroring; the first-launch wizard flips the steps right-to-left manually | A deliberate pre-release limitation; the text is readable, the order of Settings columns stays left-to-right |
| The interface is in English although Russian is needed | In "System" mode `LC_ALL` → `LC_MESSAGES` → `LANG` are read; `C`/`POSIX` and an unknown locale give English | Choose the language explicitly in "Settings → Language" — it applies immediately, no restart needed; see [Settings and language](settings.md) |
| Station, country and genre names are not translated | They come from Radio Browser and stay as they are, as do service strings (CLI, IPC protocol, window titles) | Normal behaviour |

## 6. Tray and MPRIS

| Symptom | Why | What to do |
|---|---|---|
| There is no tray icon | On Linux the tray works via StatusNotifierItem and requires an SNI host: Noctalia, KDE, Hyprland, Sway/waybar; in GNOME — an extension such as AppIndicator/StatusNotifier | Without a host there is no icon — normal degradation, control it via `orbitfm …`; details in [Tray and MPRIS](tray-and-mpris.md) |
| The icon disappears after the music stops with the window hidden | That is how builds before 0.16.0-rc.2 behaved: at `stopped` the SNI item went to `Passive`, and Noctalia v5 hid it | Update OrbitFM: since 0.16.0-rc.2 the status is always `Active` while the process is alive |
| There is no tray on Windows | The tray module there is deliberately a no-op | On Windows, control it via SMTC and the CLI |
| On macOS there is no icon in the menu bar | AppKit did not create `TrayIcon`; in that case the player writes a `warn!` to the log and works without a tray | Check `RUST_LOG=debug`, control it via `orbitfm …` and Now Playing; the icon appears after a restart if the backend recovers |
| On macOS the media keys and Control Center do not see the player | That was the case before 0.13.0-rc.1: a hidden window parked the run loop, and `MPRemoteCommandCenter` did not deliver commands | Update OrbitFM; in 0.13 hiding is `orderOut` with a live run loop. Live acceptance on a real Mac is still ahead, so report any discrepancy in an issue |
| The tray menu says "Nothing playing" | The process is alive, but playback has not started or the stream did not load | Start a station from the window or "Radio Roulette" in the tray menu, check `orbitfm status` |
| The player is not visible in `playerctl` and widgets | The MPRIS name is `org.mpris.MediaPlayer2.OrbitFM`; it is missing only if the process is not running | `pgrep -ax orbitfm`, then `playerctl -l` and `playerctl -p OrbitFM status` |
| Control from SMTC (Windows) / Now Playing (macOS) behaves strangely | SMTC has only been checked by the build and tests; Now Playing in 0.13 was rewritten for a live run loop, but there have been no live checks on a real Mac yet | Use the CLI/IPC (`orbitfm next`, `status --json`); report any discrepancy in an issue |

## 7. Settings, library and files

```text
$XDG_CONFIG_HOME/orbitfm/settings.json              (usually ~/.config/orbitfm/settings.json)
$XDG_DATA_HOME/orbitfm/library.json                 (usually ~/.local/share/orbitfm/library.json; favorites, history and manual links — the aliases field)
$XDG_DATA_HOME/orbitfm/stats.json                   (local listening statistics)
$XDG_DATA_HOME/orbitfm/cache/stations-snapshot.json (offline snapshot; usually ~/.local/share/...)
IPC: $XDG_RUNTIME_DIR/orbitfm.sock (Linux), \\.\pipe\orbitfm-<profile hash> (Windows), $TMPDIR/orbitfm-<uid>/orbitfm.sock (macOS)
```

| Symptom | Why | What to do |
|---|---|---|
| A warning about damaged settings, everything is back to defaults | With a damaged `settings.json` the program warns and runs the session on default values **without rewriting** the file | Fix or delete the file by hand and restart; deleting it also brings back the first-launch wizard |
| The first-launch wizard appeared again | It is shown only when there is no `settings.json`; so the file was renamed, deleted, or the profile is new | Finish the wizard or press "Skip" — the `onboarding_done` mark is saved |
| Theme, language, volume were not saved | The session may have been running on default values (the row above) | Check the contents of `settings.json` after `orbitfm quit` |
| I need to start from scratch | The config and the library are ordinary JSON files | With the process stopped, rename them: `orbitfm quit`, then `mv ~/.config/orbitfm/settings.json ~/.config/orbitfm/settings.json.bak` and the same for `library.json` |
| Favorites and history are gone | The library is separate from the settings: favorites and history live in `library.json` | Restore the `.bak` file; the appearance is not affected |
| The offline snapshot is not updated or needs to be deleted | The snapshot is written only after a live catalog response and no more than once a day; a damaged file is simply ignored | "Settings → About → Clear offline snapshot"; a new one appears after the next successful Radio Browser response |
| The snapshot is empty after clearing | Clearing disables writing until the end of the session and erases the file | Restart OrbitFM or wait for the next successful catalog response in a new session |
| The sleep timer did not fire | The timer lives in the session and is reset by `orbitfm quit`; after a restart it has to be set again | Check `orbitfm status --json` → `sleep`; set the timer again with `orbitfm sleep <minutes>` |
| `library-import` refused | The file is over 2 MiB/5000 stations, is not schema 1 JSON, cannot be read, or the import was started without a running player | Check the file size and `schema`, the permissions, and that OrbitFM is running; the import does not require stopping playback |
| `library-import` returned `skipped` above zero | Stations with local addresses (`play-url` → `source = stream`, e.g. `http://127.0.0.1:…`) are exported as they are, but the import skips anything that fails the public URL check | This is not a porting error: such stations have to be added again with `orbitfm play-url <address>`; for a library with only public stations a repeated import returns zeros |
| `library-export` did not create a file | There is no write permission for the directory, the path points to a directory, or the player is not running | Specify an existing directory with write permission and a running OrbitFM; the export writes the file atomically with `0600` permissions |
| The notification "Listening stats could not be read — counters started from zero" | `stats.json` did not parse: the file is truncated, was edited by hand, contains an unknown `schema`, or cannot be read at all. Counters start from zero, and this file is **not touched** until the first successful write; playback is unaffected | The notification is shown once per launch. If you need the file — `orbitfm quit`, then fix or remove `~/.local/share/orbitfm/stats.json` and launch again. If you do not — just keep listening: the first successful write replaces it in the usual way |
| Statistics is empty or does not grow | Counting is off (`stats_enabled = false` — the "Count listening time" toggle in "Settings → Statistics"), or the broadcast is not in the `playing` state: pause, buffering, reconnect, error and stop do not add seconds | Check `orbitfm stats` — the line `stats enabled false` means counting is off; turn the toggle on and make sure `orbitfm status` gives `status playing`. Turning counting off does not delete the file: previous counters remain |
| Past minutes cannot be brought back after "Clear" or `stats clear` | "Clear" and `orbitfm stats clear` write an empty schema 1 to disk (the command is idempotent and answers `ok stats cleared` even with zero counters), and the duration history is not stored anywhere | This is not data loss but normal behaviour. Copy the file before experimenting: `orbitfm quit`, then `cp ~/.local/share/orbitfm/stats.json ~/.local/share/orbitfm/stats.json.bak` |
| The "Setting not saved" toast while statistics accumulate (the details contain the error text) | Writing `stats.json` failed (no permission for the data directory, the disk is full, the file belongs to another user). Counters stay in memory and try to write again at the next convenient moment | Fix the permissions on `~/.local/share/orbitfm/` — for example `chmod u+rwX ~/.local/share/orbitfm`; process exit also tries to write what has accumulated, so after fixing it is better to restart OrbitFM |

## 8. Noctalia widget

| Symptom | Why | What to do |
|---|---|---|
| Clicking the widget does nothing | `orbitfm` is not in the panel environment's `PATH` | `command -v orbitfm`; the widget needs a native build — AppImage or sources |
| The notification "Failed to run the orbitfm command" | The plugin could not launch an external command from `dependencies` | See the row above |
| The player closed itself 5 seconds after the click | Builds before 0.9.0-rc.3: launching with a callback was limited by the command timeout | Update OrbitFM, then `noctalia msg plugins update orbitfm` |
| The widget is not in the "Add widget" list | The plugin is not installed or not enabled; `noctalia msg plugins` has no `install` command | `orbitfm integration install noctalia` — the target registers the git source `orbitfm` and enables the widget with shell commands; the same steps manually are `source add` → `update` → `enable` on [Panels and widgets](panels.md) |
| `enable` was run but nothing is in the bar | `enable` only allows the plugin, it does not add the widget to the bar | bar settings → "Add widget" → **OrbitFM** |
| The panel from the widget stretches over the whole screen | The widget opens the panel with the `orbitfm panel` command; compact layouts are not positioned without WM rules | Window rules, section 4: `orbitfm integration install niri` or `orbitfm integration install hyprland` |

## 9. How to collect logs

OrbitFM writes logs to **stderr**, the level is set by the `RUST_LOG` variable (tracing-subscriber
env-filter, default `orbitfm=info`); ANSI colors are enabled only when stderr is a terminal.

The key point: **the process that started the GUI writes the logs.** The `orbitfm …` command from a
terminal is a client, it will not show the logs of the existing process. To collect them:

```bash
orbitfm quit
RUST_LOG=debug orbitfm show 2> ~/orbitfm.log
```

Reproduce the problem and attach `~/orbitfm.log`. If the GUI is started by a plugin or autostart,
redirect stderr to a file there, otherwise the logs of that session are lost.

### macOS: log of the selected GPU

On the first frame the app writes one line `orbitfm render: …` to stderr — which video adapter
wgpu picked, its driver and limits. This is the only way to see what draws the globe on a
particular machine, and it is exactly what is needed for Intel acceptance:

```bash
orbitfm quit
orbitfm 2>&1 | tee orbitfm-render.log
```

Close the window (or `orbitfm quit` from another terminal) and attach
`orbitfm-render.log` in full. The line is diagnostics, not an interface: its format may
change between releases, so it cannot be pieced together by fields from an issue, only attached
whole. The output format and what has already been checked on different Macs are on the installation pages
[Apple Silicon](install-macos-apple-silicon.md) and [Intel](install-macos-intel.md).

## 10. What to attach to an issue

The "Bug" template from `.github/ISSUE_TEMPLATE/bug.yml` asks for:

1. **Summary** — what went wrong.
2. **Steps to reproduce** — a numbered list.
3. **Expected behaviour**.
4. **Actual behaviour**.
5. **Platform** — for example `Linux (Niri 25.x) / Windows / macOS`.
6. **Version / commit** — the output of `orbitfm --version` or the git SHA.
7. **Logs / stack** — the file from section 9; if the problem is graphics on macOS, separately
   `orbitfm-render.log` from the same section.

It helps to add the output of `orbitfm status`, whether the system `libmpv` is present (for a native build),
the compositor name and the Noctalia/Hyprland version if the issue is about the window or the widget.
Issue tracker: <https://github.com/ModernByte/orbitfm/issues>.

---

## What OrbitFM cannot do (this is not a bug)

| Expectation | Fact |
|---|---|
| Global system-wide hotkeys | They do not exist in the current version; media keys work via MPRIS/SMTC/Now Playing and `orbitfm …` |
| Broadcast recording and playlists | Not implemented; there is a sleep timer — one-shot, `orbitfm sleep <minutes>` |
| Packages in AUR, Flathub, Homebrew | There are none: the Linux path is an AppImage or a build from source |
| Sync, account, telemetry | Settings, library and listening statistics are stored locally in `settings.json`, `library.json` and `stats.json` and are not sent anywhere; only requests go out to the Radio Browser catalog (including click-tracking), to the selected station's stream and to the artwork address — details in the "Security and privacy" section of the README |
| Moving manual links between machines | Aliases are the `aliases` field of the local `library.json`; they are not in `library-export` and `library-import` (schema 1), so on another machine the manual link is set again — `orbitfm alias <uuid> <url>`. There is no separate solution for porting them |
| `http` artwork | By default they are deliberately not loaded: plain HTTP would allow substituting the bytes going to the decoder on the way. The owner can enable them with the "Allow HTTP artwork" toggle in Settings — then such stations get an icon, and substitution along the way becomes an accepted risk of that choice |
| Tray on Windows | The module is deliberately a no-op; on macOS the menu bar has existed since 0.13.0-rc.1 |
| Player control from the Noctalia widget | The widget is a single show/hide panel button, the rest is in the window and the tray |
| SMTC (Windows) and Now Playing (macOS) as verified | SMTC has only been checked by the build; Now Playing was rewritten for a live run loop in 0.13.0-rc.1, but there has been no live acceptance on a real Mac |
| A promise of smooth rendering on Intel UHD Graphics 617 | There is none: the x86_64 build is compiled, linted and tested on an Intel runner and passes launch in CI, but the first frame on this GPU has not been captured yet — see `orbitfm-render.log` above |
| Remembering the window layout between launches | It is not remembered, a new launch starts with the default layout |
| Resizing the Overlay/Listening capsule or the Panel by hand | Compact layouts are fixed: the default size and the minimum are set by the layout, and position and size on a tiling WM are set by window rules (section 4) |

Next: [Glossary](glossary.md) · [Controls and hotkeys](controls.md) ·
[Home](index.md)
