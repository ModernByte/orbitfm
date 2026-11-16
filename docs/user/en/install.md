# Installing OrbitFM

**English** · [Русский](../ru/install.md)

← [Home](index.md)

The player installs as a single binary: there are no separate "engines" or services to add. Pick one of
three paths — if you just want to listen to radio, take the ready-made package for your system.

| Method | Who it suits | Difficulty |
|---|---|---|
| Ready-made package (AppImage / MSI / DMG) | regular user | low |
| Build from source (`cargo`) | your own build, a patch, a different toolchain | medium |
| Flatpak locally from the repository | sandboxes; **without** the Noctalia widget and the external CLI | medium |

> There are **no** ready-made packages in AUR, Flathub or Homebrew: the project is distributed only through
> GitHub releases and building from source.

## 1. Ready-made packages

The files are on the releases page: <https://github.com/ModernByte/orbitfm/releases>. In 0.23.0-rc.2 there are six
builds and `SHA256SUMS`:

| File | System |
|---|---|
| `OrbitFM-0.23.0-rc.2-x86_64.AppImage` | Linux x86_64 |
| `OrbitFM-0.23.0-rc.2-windows-x64.msi` | Windows x64 |
| `OrbitFM-0.23.0-rc.2-macos-arm64.dmg` | macOS on Apple Silicon |
| `OrbitFM-0.23.0-rc.2-macos-x86_64.dmg` | macOS on Intel |
| `OrbitFM-0.23.0-rc.2-plasma.plasmoid` | Plasma 6 widget (not the app) |
| `install-appimage.sh` | optional AppImage integration into the menu and PATH (see 1.2) |
| `SHA256SUMS` | checksums of all six files above |

`.plasmoid` is a Plasma 6 panel widget, not a build of the player: how to install and connect it is
described on the [Integrations](integrations.md) page.

To verify integrity, run this from the directory you downloaded into (`sha256sum` is available on Linux and macOS):

```bash
sha256sum -c SHA256SUMS     # expected output: one "OK" line per file
```

On Windows, get the hash like this and compare it with the first column of `SHA256SUMS` (case doesn't matter):
`Get-FileHash .\OrbitFM-0.23.0-rc.2-windows-x64.msi -Algorithm SHA256`

It is important to understand what this checksum verifies: `SHA256SUMS` contains **digests, not signatures**. The project
has no Authenticode or Developer ID certificates, so comparing hashes confirms that the file downloaded in full and matches
the one published in Releases, but it does not confirm the publisher. Download only from the releases page of the same
repository.

### 1.1. Linux — one command

If you just want to listen, install the AppImage with one command — no root and no file manager:

```bash
curl -fsSL https://raw.githubusercontent.com/ModernByte/orbitfm/main/install.sh | sh
```

The script picks the latest release, downloads the `x86_64.AppImage`, compares it with `SHA256SUMS`, copies the image to `~/.local/bin/orbitfm`, adds the **OrbitFM** entry with its icon to the app menu and refreshes the caches. Settings, favourites and listening statistics live in separate files, so reinstalling or removing does not touch them.

| Task | Command |
|---|---|
| Install or update to the latest release | `curl -fsSL https://raw.githubusercontent.com/ModernByte/orbitfm/main/install.sh \| sh` |
| Install a specific version | `… \| sh -s -- --release 0.23.0-rc.2` |
| Download and verify only, change nothing | `… \| sh -s -- --dry-run` |
| Remove what the script installed | `… \| sh -s -- --uninstall` |

It needs Linux x86_64, `curl` (or `wget`) and `sha256sum`. On systems without `libfuse2` (Ubuntu 24.04 and newer) the menu entry is created with `APPIMAGE_EXTRACT_AND_RUN=1`, so the player still starts without FUSE, only a little slower on launch; after installing `libfuse2`, run the installer again to get the normal launch. The `orbitfm` command becomes available in a new terminal only if `~/.local/bin` is already in `PATH`, otherwise add it to your shell profile — the installer prints a reminder.

The script is Linux-only: macOS uses the DMG and Windows uses the MSI (sections 1.3 and 1.4).

### 1.2. Linux — AppImage

An AppImage isn't installed, it's run. `libmpv` and its native dependencies are already inside, so you do **not** need
to install `mpv`/`libmpv` from your package manager.

```bash
chmod +x OrbitFM-0.23.0-rc.2-x86_64.AppImage
./OrbitFM-0.23.0-rc.2-x86_64.AppImage
```

No FUSE (containers, systems with `libfuse` blocked)? Unpack and run the file like this:
`APPIMAGE_EXTRACT_AND_RUN=1 ./OrbitFM-0.23.0-rc.2-x86_64.AppImage`

**Menu and PATH without root.** `install-appimage.sh` is published next to the AppImage:

```bash
bash install-appimage.sh OrbitFM-0.23.0-rc.2-x86_64.AppImage
```

The script copies the image to `~/.local/bin/orbitfm`, installs a desktop entry with an absolute `Exec`, an icon and metainfo
into `~/.local/share`, refreshes the caches and warns you if `~/.local/bin` isn't in `PATH`. To remove it, run
`bash install-appimage.sh --uninstall`; the downloaded AppImage and your settings are left untouched.
To update, run the script with the new image.

**Manually.** For running from a terminal and for the Noctalia widget, the `orbitfm` command must be in PATH — both refer
to the app by name:

```bash
mkdir -p ~/.local/bin
ln -sf ~/Downloads/OrbitFM-0.23.0-rc.2-x86_64.AppImage ~/.local/bin/orbitfm
```

Check `echo $PATH`: `~/.local/bin` must be there, otherwise add it to your shell rc.

The app menu and icon — ready-made files from `packaging/linux/` in the repository tree:

```bash
mkdir -p ~/.local/share/applications ~/.local/share/icons/hicolor/scalable/apps
cp packaging/linux/org.modernbyte.OrbitFM.desktop ~/.local/share/applications/
cp packaging/linux/org.modernbyte.OrbitFM.svg ~/.local/share/icons/hicolor/scalable/apps/
```

The `.desktop` file defaults to `Exec=orbitfm mode focus` and `Icon=org.modernbyte.OrbitFM`: the launcher must find
`orbitfm` in PATH (the symlink above), otherwise replace `Exec=` with the absolute path to the AppImage. Because of `mode focus`,
clicking the shortcut always gives you the Focus layout rather than toggling window visibility. The `Icon=`
name matches the `.svg` name, so the icon is taken from the hicolor directory. The metainfo
(`org.modernbyte.OrbitFM.metainfo.xml`) can be placed in `~/.local/share/metainfo/` — app centers read it,
it doesn't affect launching.

### 1.3. Windows — MSI

The MSI installs **for the current user** (`Scope="perUser"`), no administrator rights are needed.

1. Run `OrbitFM-0.23.0-rc.2-windows-x64.msi` and click **Next → Install**.
2. The files go to `%LOCALAPPDATA%\Programs\OrbitFM`, the shortcut into the Start menu (from there: `Win` → `OrbitFM`).
3. On first launch SmartScreen shows "Windows protected your PC", because the installer is **not signed**
   with an Authenticode certificate: click **More info → Run anyway**.

### 1.4. macOS — DMG

Two builds are published for macOS, and they are not interchangeable: the arm64 app won't start on an Intel Mac,
and Rosetta translates only x86_64 → arm64, not the other way round.

| Mac | File | Guide |
|---|---|---|
| Apple Silicon — any, M1 and newer | `OrbitFM-0.23.0-rc.2-macos-arm64.dmg` | [Installing on Apple Silicon](install-macos-apple-silicon.md) |
| Intel | `OrbitFM-0.23.0-rc.2-macos-x86_64.dmg` | [Installing on Intel](install-macos-intel.md) |

Check the architecture: `uname -m` prints `arm64` or `x86_64`. In "About This Mac" that's the **Chip** line
(Apple M…) or the **Processor** line (Intel). The first command has a trap: it reports the architecture of the
process, not of the machine — a terminal running under Rosetta on Apple Silicon will print `x86_64`. When in doubt,
check "About This Mac".

Common to both builds: macOS 13.0 or newer is required — the floor is enforced by the build itself (`LSMinimumSystemVersion`
in `Info.plist`, `MACOSX_DEPLOYMENT_TARGET=13.0`, `libmpv` bottles for macOS 13 and a minimum-version check at build time),
not just by words. The signature is ad-hoc only and there is no notarization, so the first launch is a right click on the app → **Open**
(fallback: `xattr -d com.apple.quarantine`). Step-by-step
instructions, setting up `PATH`, checking the version and troubleshooting errors are on the pages linked above.

## 2. Building from source

You need Rust **1.95+** (Edition 2024) and the system libraries the project links directly.

Debian / Ubuntu — the exact package set the build needs:

```bash
sudo apt install -y libgl1-mesa-dev libmpv-dev libwayland-dev libx11-dev \
  libxcursor-dev libxi-dev libxkbcommon-dev libxrandr-dev
```

Arch (`libmpv` comes in the `mpv` package) and Fedora: `sudo pacman -S mpv`, `sudo dnf install mpv-libs-devel`.

```bash
git clone https://github.com/ModernByte/orbitfm
cd orbitfm
cargo build --release
./target/release/orbitfm
```

A local AppImage from the same tree: `./packaging/linux/build-appimage.sh` — the file appears in
`target/appimage/OrbitFM-<version>-x86_64.AppImage`.

For `orbitfm status`, `orbitfm status --json` and the Noctalia widget to work, the binary you built also needs to be in PATH:

```bash
ln -sf "$(pwd)/target/release/orbitfm" ~/.local/bin/orbitfm
```

## 3. Flatpak (locally from the repository)

Flatpak is **not part of the releases**, build it yourself; you need `flatpak-builder` and the `freedesktop` 25.08 runtime:

```bash
./packaging/linux/build-flatpak.sh
```

Limitation: the IPC socket of the Flatpak build lives inside the sandbox, so **the Noctalia widget and `orbitfm …` commands
from an outside shell don't work with it**. If you need desktop integration, take the AppImage.

## 4. After installation

Launch the app and go through the first two minutes — [First launch](first-launch.md). The network is needed only
for the Radio Browser catalog: the globe is drawn without internet, country borders are baked into the binary.

## Common installation pitfalls

| Symptom | Cause | What to do |
|---|---|---|
| `Permission denied` when starting the AppImage | the file is not executable | `chmod +x OrbitFM-…AppImage` |
| AppImage fails on FUSE | `libfuse` missing / blocked | `APPIMAGE_EXTRACT_AND_RUN=1 ./OrbitFM-….AppImage` |
| `orbitfm: command not found` | the binary is not in PATH | symlink into `~/.local/bin/orbitfm`, check `echo $PATH` |
| The Noctalia widget is empty | no `orbitfm` in PATH, or the Flatpak build | see 1.1; can't be fixed with Flatpak |
| `cargo build`: `libmpv` error | dev packages are not installed | `libmpv-dev` (Debian) / `mpv-libs-devel` (Fedora) |
| The build fails on Edition 2024 | old Rust | `rustup update stable`, 1.95+ is required |
| SmartScreen blocks the MSI | no Authenticode signature | "More info → Run anyway" |
| macOS: "the developer cannot be verified" | ad-hoc signature, no notarization | right-click → "Open", or remove `com.apple.quarantine` |
| No sound / the station won't start | this isn't installation: network, codec, broken URL | [Troubleshooting](troubleshooting.md) |

Next: [First launch](first-launch.md) · [Window modes](window-modes.md) · [Troubleshooting](troubleshooting.md)
