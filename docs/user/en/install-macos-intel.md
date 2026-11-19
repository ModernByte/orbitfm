# Installing on macOS: Intel

**English** · [Русский](../ru/install-macos-intel.md)

← [Installation](install.md) · [Home](index.md)

This page is for Macs with an Intel processor. You need the file
`OrbitFM-0.23.0-rc.3-macos-x86_64.dmg`.

Apple Silicon will also run this build through Rosetta, but the native arm64
is faster and doesn't require Rosetta — for such Macs there is
[a separate guide](install-macos-apple-silicon.md). The reverse doesn't work:
the arm64 app doesn't start on an Intel Mac at all, and Rosetta doesn't help there.

This page covers installation; which of the macOS promises have already been confirmed and which
haven't is on the [Troubleshooting](troubleshooting.md) page. The hardware floor we start
from is `MacBookAir8,2` (MacBook Air Retina 13″ 2019, Intel Core
i5-8210Y, Intel UHD Graphics 617, 8 GB); anything more powerful also works.

## 1. Check your Mac

```bash
uname -m
```

`x86_64` is expected. If the command prints `arm64`, this is Apple Silicon — open
[the Apple Silicon guide](install-macos-apple-silicon.md).

In "About This Mac" the architecture corresponds to the **Processor** line (Intel Core
i5/i7/i9, Xeon and so on). The system version must be macOS 13.0 or newer; on anything older
the build won't start, and this isn't "unofficial", it is checked at build time:
`LSMinimumSystemVersion` and `check-min-os.py` won't let a bundle through if even one
Mach-O requires a different macOS.

One trap is worth knowing about the architecture: `uname -m` reports the architecture of the current
process, not of the machine — a terminal running under Rosetta on Apple Silicon
will print `x86_64`. If the result is in doubt, check the model in
"About This Mac": an identifier like `MacBookAir8,2` means Intel.

## 2. Download and verify the file

From the releases page <https://github.com/ModernByte/orbitfm/releases> download:

- `OrbitFM-0.23.0-rc.3-macos-x86_64.dmg`;
- `SHA256SUMS` — if you want to verify the checksum.

macOS doesn't ship `sha256sum` from GNU coreutils, but it has `shasum`. This verifies only your
file, without the other release assets:

```bash
cd ~/Downloads
grep 'macos-x86_64\.dmg$' SHA256SUMS > orbitfm-x86_64.sha256
shasum -a 256 -c orbitfm-x86_64.sha256
rm orbitfm-x86_64.sha256
```

The expected output is `OrbitFM-0.23.0-rc.3-macos-x86_64.dmg: OK`. The checksum confirms
that the file downloaded in full and matches the published one; it is not a signature.

## 3. Install the app

1. Open the `.dmg` with a double click.
2. Drag **OrbitFM** into the **Applications** folder.
3. Eject the image: right-click it on the desktop → **Eject**.

The app is installed to `/Applications/OrbitFM.app`; no administrator rights are needed.

## 4. First launch and Gatekeeper

The build is signed ad-hoc only (`codesign --sign -`), and there is no Apple notarization.
That's why Gatekeeper warns on first launch — this is expected, not a sign of a
virus.

1. Open **Applications**, right-click **OrbitFM** → **Open**.
2. In the dialog, click **Open** once more.

If there is no **Open** button (this happens on macOS 15 and newer):

1. Try launching the app with a normal double click.
2. Open **System Settings** → **Privacy & Security**.
3. Scroll down to the message about OrbitFM and click **Open Anyway**,
   confirm with your password or Touch ID.

A fallback from the terminal — remove the quarantine flag from the app:

```bash
xattr -d com.apple.quarantine /Applications/OrbitFM.app
open /Applications/OrbitFM.app
```

If macOS says the app is **"damaged"**, it's usually the same quarantine flag on an
ad-hoc signature. Repeat `xattr -dr com.apple.quarantine /Applications/OrbitFM.app`,
then launch it again. If the message persists, download the DMG again and verify the checksum.

## 5. `orbitfm` commands in the terminal

The app in `/Applications` corresponds to the binary
`/Applications/OrbitFM.app/Contents/MacOS/orbitfm`. For `orbitfm status`,
`orbitfm panel`, the Noctalia widget and the other commands to work, put it in `PATH`:

```bash
mkdir -p ~/.local/bin
ln -sf /Applications/OrbitFM.app/Contents/MacOS/orbitfm ~/.local/bin/orbitfm
```

Check that `~/.local/bin` is in `PATH`:

```bash
echo $PATH | tr ':' '\n' | grep "$HOME/.local/bin"
```

If there is no line, add the directory to `~/.zshrc` and restart the terminal:

```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

Verify the installation:

```bash
orbitfm --version
file /Applications/OrbitFM.app/Contents/MacOS/orbitfm
```

The expected result is version `0.23.0-rc.3` and `Mach-O 64-bit executable x86_64`.

### If the picture doesn't appear — capturing the render log

The first frame writes a single line to stderr, `orbitfm render: …`, with the GPU that
was selected for rendering, its driver and its limits. The line is a diagnostic, not the interface: its format
may change between releases. To gather evidence for acceptance testing or for an
[issue](https://github.com/ModernByte/orbitfm/issues):

```bash
orbitfm quit
orbitfm 2>&1 | tee orbitfm-render.log
```

Close the window (or run `orbitfm quit` in another terminal) — and send the whole
`orbitfm-render.log`. If there is no `orbitfm render:` line at all, the app never
got to render initialization, and that's a different defect: in that case the output of
`RUST_LOG=debug orbitfm 2>&1 | head -50` is useful.

## 6. Updating and uninstalling

**Updating.** Download the new DMG, stop the current process with `orbitfm quit`,
drag the new app into **Applications**, replacing the old one. Settings, favorites and
history live separately and are preserved.

**Uninstalling.**

```bash
orbitfm quit
rm -f ~/.local/bin/orbitfm
```

Then drag `OrbitFM.app` from **Applications** to the **Trash**. If you also want to remove
user data:

```bash
rm -rf ~/.config/orbitfm ~/.local/share/orbitfm
```

## 7. Building from source

You need Xcode Command Line Tools and Rust 1.100+. Homebrew and `brew install mpv`
are not needed: the script downloads pinned `libmpv`/`dylibbundler` bottles from
ghcr.io, unpacks them into a prefix and writes `LIBRARY_PATH`.

```bash
xcode-select --install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

```bash
git clone https://github.com/ModernByte/orbitfm
cd orbitfm
python3 packaging/macos/fetch-libmpv.py
export LIBRARY_PATH="/tmp/orbitfm/opt/mpv/lib"
cargo build --release --locked
./target/release/orbitfm
```

A DMG from the same tree — the architecture is declared by a flag, not derived from the build
machine:

```bash
bash packaging/macos/build-dmg.sh --arch x86_64
```

Without `--arch` the script stops with a usage message (as it also does with an
unknown value) — this replaces the previous behaviour, when the architecture was silently taken from
`uname -m` and a build on a different architecture left a `.app` of the wrong slice.

The finished file appears in `dist/OrbitFM-0.23.0-rc.3-macos-x86_64.dmg`. Inside, `libmpv`
from Homebrew bottles for macOS 13 is bundled into the `.app` (the path to `dylibbundler` is set
through `PATH`, the script creates it in the prefix). Before the file is placed in `dist/`,
the bundle passes the gates built into the script: `check-app-arch.sh` (every Mach-O must
have exactly the declared architecture — it catches both an accidental substitution and a fat binary),
`audit-app-dylibs.sh` (dependency paths) and `smoke-launch.sh` (a real
`orbitfm --version` run). One more, `check-min-os.py` — no Mach-O may require a newer macOS
than promised, and the limit is taken from `Info.plist` rather than from the command line — covers
the whole bundle during the release build.

## If something doesn't work

| Symptom | Cause | What to do |
|---|---|---|
| "The program can't be opened… it is not supported on this type of Mac" | A DMG of the wrong architecture was downloaded (arm64 on Intel), or macOS is older than 13.0 | `uname -m` → `x86_64`; download `macos-x86_64.dmg`; update macOS to 13+ |
| "The application requires macOS 13.0 or later" | Old macOS | Update the system; no build for an older macOS is published |
| "The developer cannot be verified" | No Apple notarization | Right-click → "Open", or "Open Anyway" in Settings |
| "The application is damaged" | Quarantine on an ad-hoc signature | `xattr -dr com.apple.quarantine /Applications/OrbitFM.app` |
| `orbitfm: command not found` | The binary is not in `PATH` | Symlink into `~/.local/bin/orbitfm`, check `echo $PATH` |
| No icon in the menu bar | The macOS tray wasn't created | `RUST_LOG=debug` and [Tray and MPRIS](tray-and-mpris.md); the app works without the icon too |
| The window opened but there's no globe / it's black | Rendering didn't initialize, or the GPU couldn't meet the limits | capture `orbitfm-render.log` (section 5) and send it; rendering on Intel UHD 617 has not yet been confirmed by acceptance testing |
| The app crashes right after opening, with no window | dyld couldn't build the dependency graph | `orbitfm --version` from the terminal; if it aborts or says `Symbol not found`, attach the full output, see [Troubleshooting](troubleshooting.md) |
| No sound | Network, codec or the station address | [Troubleshooting](troubleshooting.md) |

---

Next: [First launch](first-launch.md) · [Installing on Apple Silicon](install-macos-apple-silicon.md) ·
[Tray and MPRIS](tray-and-mpris.md)
