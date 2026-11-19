# Integrations: browser

**English** · [Русский](../ru/browser.md)

← [Integrations](integrations.md) · [Home](index.md)

The OrbitFM panel lives in a button on the browser toolbar and mirrors the Noctalia drop-down panel: station card, transport, volume, theme, search, favorites, history and "Play this page". It does not open a second process and does not start OrbitFM itself — it controls the instance that is already running. Labels come from the app, so the panel speaks the language selected in OrbitFM.

## What you need

- **OrbitFM is already running.** If the app is closed, the panel shows "OrbitFM is not running" and offers Retry; a cold start from the browser is not allowed.
- **`orbitfm` in `PATH`** — for AppImage this is done by `install-appimage.sh` or a symlink to the image.
- Chrome, Chromium, Brave, Edge or Vivaldi — or Firefox 109+.
- The integrations installer currently works on Linux only.

## Installation

You can install the panel with a button on the **Settings → Integrations → Browsers** page or with a command. The `chrome` and `firefox` targets are independent: you can run both in a row.

### Chrome and Chromium browsers

```bash
orbitfm integration install chrome
```

The command unpacks the extension into `~/.local/share/orbitfm/integrations/chrome/` and writes the native manifest into the directories of the browsers it found. Then:

1. Open `chrome://extensions` and enable **Developer mode**.
2. Click **Load unpacked**.
3. Select the `~/.local/share/orbitfm/integrations/chrome/extension` directory.

The extension appears on the browser toolbar. Its ID is fixed by a public key (`lfhmfgjblkebbalfdlbmnalghhjplfbg`), so the native manifest keeps matching the extension after a browser restart.

### Firefox

```bash
orbitfm integration install firefox
```

1. Open `about:debugging#/runtime/this-firefox`.
2. Click **Load Temporary Add-on…**.
3. Select
   `~/.local/share/orbitfm/integrations/firefox/extension/manifest.json`.

A temporary add-on works until Firefox restarts: after a restart you have to load it again. Permanent installation requires a Mozilla signature; no signed XPI is published in this repository.

### If the browser is not found

There is no bulk install: `chrome` and `firefox` are installed separately, like any target for panels. If the browser is not on the system, the extension files are still installed but the native manifest is skipped — install the browser and run `orbitfm integration install chrome` or `orbitfm integration install firefox` again.

## What the panel can do

| Element | Action |
|---|---|
| Station card | Name, country/tags/bitrate, `stream` badge, monogram and a decorative visualizer |
| Heart | Add or remove the current station from Favorites |
| Play/Pause, Stop | Playback control |
| Previous / Next | Step through the visible list; the buttons dim when the step is unavailable |
| Radio Roulette | Random station |
| Volume | 0–100 % slider and ±5 % buttons; the value goes to the app |
| Theme | Cycles through System, Dark and Light |
| Show/Hide window | Shows or hides the OrbitFM window |
| Settings | The Settings window of the running app |
| Search | Radio Browser catalog query with a 300 ms delay, up to 20 results |
| Favorites / History | 20 entries each; playback without another network search |
| Play this page | Scans the active tab and imports the audio stream it finds |

State updates by itself: the panel subscribes to the `orbitfm events` stream, so track changes, pause and volume are visible without polling. While the panel is open it receives `snapshot` and the following `state`; `keepalive` means "nothing is changing", not an error. Search results arrive in a `search` frame only for their own generation (`id`), so a stale answer does not override a new request.

### Search, favorites and history

Search sends `search <query>`; the app answers `search <id>` and publishes the results in a `search` frame. A click on a result sends `play-result <uuid>` — only a station from the latest result set can be played. The Favorites and History tabs read the library and start a station with `play-library <uuid>` without contacting Radio Browser. The heart toggles `favorite`, and the lists are re-read after a change.

### Play this page

The button in the footer scans **the active tab and only on click**: `og:audio`, `<audio src>`, `<source src>` and the page URL itself. Relative addresses are made absolute, only `http(s)` are kept, duplicates are removed, and there are no more than 20 candidates. You pick an entry — the extension sends `play-url <url>`, and the app parses the page, the `.m3u`/`.pls` or the direct stream. HLS is not supported. Without the `scripting` permission only the tab URL remains. There is no autoplay: the import starts only after your click.

## What the panel does not do

- **Does not start OrbitFM.** All commands work only with an already running instance; `orbitfm show` from the terminal remains the way to open the window.
- **Does not end the process.** The `quit` command is forbidden: quitting goes only through the CLI, the tray or the app menu.
- **Does not start an arbitrary station by UUID.** `play` is forbidden; you can play a search result, a favorites/history entry or a URL after an explicit click.
- **Does not switch the window layout.** `mode` is forbidden; showing, hiding, the panel and the overlay are available.
- **Does not touch listening statistics.** `stats`, `stats --json` and `stats clear` are outside the native host allowlist, so the host answers them with `forbidden`: the panel does not read private counters and does not erase them. Statistics remain available from the Settings window and the CLI.
- **Does not read pages in the background.** The extension has no content scripts, no background page and no persistent site permissions. The `activeTab` and `scripting` permissions grant a one-off scan of the active tab only when you press "Play this page"; without `scripting` only the tab URL remains.

## Security

- The connection goes over native messaging: the browser starts the local `orbitfm native-host` host, which talks to the app over the same socket as the CLI. There are no new network ports.
- The native manifest allows exactly one extension ID; there is no wildcard.
- The host accepts only a safe set of commands and limits messages to 1 MiB in both directions.
- `activeTab` and `scripting` give no background access: the tab is scanned once and only on your click, without content scripts and persistent permissions.
- `play-url` accepts only `http(s)`, rejects userinfo and HLS, and reads no more than 1 MiB; local network addresses are allowed because the import is started by an explicit user action, not by a page. At the same time, a public page cannot lead the import into the internal network through a redirect or a playlist — such a transition is rejected.
- The extension is distributed unpacked and is not signed by a store: enable developer mode and load files only from the directory where the OrbitFM installer put them.
- A local process that can write to your command socket already controls the player; the panel does not extend those rights.

## If the panel does not work

| Symptom | What to check |
|---|---|
| "OrbitFM is not running" | Start OrbitFM as usual and click "Retry" |
| "Specified native messaging host not found" | The manifest is not installed or the browser was not detected; run `orbitfm integration install chrome` (or `firefox`) and restart the browser |
| The panel is there but the buttons do not work | The app is closed or the command is forbidden; open OrbitFM and check `orbitfm status` |
| Search finds nothing | Check the query and the network; the indicator and "Retry" show a timeout or a catalog error |
| "Play this page" finds no audio | The page has no `og:audio`/`<audio>`; browser service pages are not scanned — only the tab URL remains |
| URL import rejected | HLS (`.m3u8`), `file://`, userinfo and bodies over 1 MiB are not supported; use a page with `og:audio`, `.m3u`/`.pls` or a direct audio file |
| Chrome does not see the extension | Is "Developer mode" enabled, and is the `extension` directory selected rather than its parent? |
| Firefox lost the add-on after a restart | A temporary add-on does not survive a restart; load `manifest.json` again or wait for a signed XPI |
| Chrome with `--user-data-dir` does not find the host | Such a profile looks for manifests in `<user-data-dir>/NativeMessagingHosts`; copy `org.modernbyte.orbitfm.json` there from the standard directory |
| Need Brave/Edge/Vivaldi | `install chrome` writes the manifest into every Chromium directory it detects; the extension is loaded the same way as in Chrome |

## Updating and removing

When OrbitFM updates, reinstall the extension: the binary path and the version may change.

```bash
orbitfm integration status chrome      # installed, modified or missing
orbitfm integration install chrome     # update files and manifest
orbitfm integration uninstall chrome   # remove only OrbitFM files
orbitfm integration uninstall firefox
```

`uninstall` removes the files listed in its receipt and the native manifests. The extension itself disappears from the browser after the directory is deleted or removed manually on the extensions page; the installer does not touch browser settings.

## What has been tested

The panel was tested live on Linux/niri, OrbitFM 0.16.0-rc.2: Chrome for Testing 153 with the unpacked extension — search (20 results), playing a result, favorites, history and playing from it, volume with buttons and keyboard, theme switching, offline and "Retry", plus a real click on the extension button with a tab scan and `og:audio` import. Chrome 156 with a user profile showed the same panel. Firefox 159 was tested on 0.13.0-rc.2; it has not been re-run on 0.16. Brave, Edge, Vivaldi and a permanently signed Firefox have not been run; their shared code, manifests and installer have been checked.

---

Next: [Window rules](window-rules.md) ·
[Controls and hotkeys](controls.md) ·
[Troubleshooting](troubleshooting.md)
