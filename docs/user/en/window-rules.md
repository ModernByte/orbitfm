# Integrations: window rules

**English** · [Русский](../ru/window-rules.md)

← [Integrations](integrations.md) · [Home](index.md)

In tiling environments a window without rules takes the whole tile. OrbitFM is
not an ordinary layout: it has three layouts and a drop-down panel, and each one
needs its own size. Ready-made rules for **niri** and **Hyprland** live in the
repository and are installed by the same installer as the widgets — with the
button on the **Settings → Integrations** page or with a command.

Rules are matched by a window class + title pair — `app-id`
(`org.modernbyte.OrbitFM`) and `title`. The title distinguishes the surfaces, so
they do not overlap:

| Surface | Window title | Size |
|---|---|---|
| Focus | `OrbitFM` | 1100×800 |
| Overlay | `OrbitFM · Overlay` | 500×620 |
| Listening | `OrbitFM · Listening` | 420×152 |
| Panel | `OrbitFM · Panel` | 400×640 |
| Settings, Compact | `OrbitFM · Settings · Compact` | 420×560 |
| Settings, Medium | `OrbitFM · Settings · Medium` | 760×544 |
| Settings, Wide | `OrbitFM · Settings · Wide` | 1040×720 |
| Sleep timer warning | `OrbitFM · Sleep Warning` | 420×320 |

In niri there are eight `window-rule` blocks, in Hyprland 18 `windowrulev2`
lines: three class-based (`float`, `center`, `stayfocused`) and 15 by title. The
settings and warning windows are centered by the compositor: the size is set by
a rule, and they have no top-right anchor.

You can check that the window matches a rule at all with one command:
`niri msg windows` (section "Check the size") should show
`App ID: "org.modernbyte.OrbitFM"` for every OrbitFM window. An empty app-id
means no rule will match the window — that was the case in 0.21.0-rc.2 and rc.3
for the settings and warning windows; fixed in 0.21.0-rc.4.

The rules do not touch playback and do not interfere with switching modes: only
the way the compositor shows the window changes.

## niri

```bash
orbitfm integration install niri
orbitfm integration uninstall niri
```

Installation puts the rules in `~/.config/niri/cfg/orbitfm.kdl` and adds exactly
one line to `~/.config/niri/config.kdl`:

```kdl
// ~/.config/niri/config.kdl
include "./cfg/orbitfm.kdl"
```

This is the only foreign file the installer edits. Before the first edit a copy
`config.kdl.orbitfm.bak` is placed next to it (it is never overwritten again),
the write is atomic, the line is searched for as a whole — a repeated
installation does not duplicate it. Removal takes away its own line, the rules
file and the empty `cfg/` directory. If you added the line yourself before
installation, removal will leave it in the config and report it: without the
rules file niri cannot parse such an `include`. Installation answers the same
way — if the line was not added by it, the output says so honestly and does not
show a non-existent backup.

niri 26.04+ rereads the config itself, there is no separate reload command.

Inside the file there are eight `window-rule` blocks, each fixes width, height
and floating opening, and non-Focus modes additionally turn off the border and
shadow. The window is placed under the Noctalia panel with an anchor in the
top-right corner (`relative-to="top-right"`). The `x` offset is tuned for the
right section of the bar; with a different layout, change it manually:

```kdl
window-rule {
    match app-id="org.modernbyte.OrbitFM" title=r"^OrbitFM · Overlay$"
    open-floating true
    default-column-width { fixed 500; }
    default-window-height { fixed 620; }
    default-floating-position x=270 y=48 relative-to="top-right"
    draw-border-with-background false
    border { off; }
    shadow { off; }
}
```

In the same file, for Focus — `x=60` and `1100×800`, for Listening — `x=310` and
`420×152`, for Panel — `x=310`, `400×640` and additionally a disabled
focus-ring: the panel should read as a continuation of the bar.

If the capsule went under the bar widget, adjust `x` so that its center
coincides with the icon. The window center is computed as
`screen_width − x − width/2`. For Overlay (500 px, `x=270`) and Listening
(420 px, `x=310`) it lies at the same point — 520 px from the right edge of the
screen.

### Check the size under niri

The socket name depends on the version and backend — find it in
`$XDG_RUNTIME_DIR` (on niri 26.04 this is, for example,
`niri.wayland-1.1108.sock`):

```bash
ls "$XDG_RUNTIME_DIR" | grep niri
NIRI_SOCKET="$XDG_RUNTIME_DIR/niri.wayland-1.1108.sock" niri msg windows
```

A real output fragment for the Overlay capsule:

```text
  Title: "OrbitFM · Overlay"
  App ID: "org.modernbyte.OrbitFM"
  Is floating: yes
  PID: 59220
  Layout:
    Tile size: 500 x 620
    Workspace-view position: 1150, 84
    Window size: 500 x 620
    Window offset in tile: 0 x 0
```

- `Window size` — the actual window size; compare it with the mode's
  expectations (500×620 for Overlay).
- `Tile size` — the tile together with border and shadow, a couple of pixels
  larger, and that is normal; if the rule turns off border and shadow, the tile
  equals the window size.
- `Workspace-view position` — the offset from the screen corner; if the window
  creeps under the Noctalia panel, increase `x` in the rule.

After `orbitfm hide` the window disappears from this list entirely, while the
process stays alive. A capsule stretched over the whole tile is a sign of a rule
that is not connected.

## Hyprland

```bash
orbitfm integration install hyprland
orbitfm integration uninstall hyprland
```

Installation puts `~/.config/hypr/orbitfm.conf` and immediately applies its 18
rules to the current session with `hyprctl keyword windowrulev2 …` commands — no
restart is needed. The installer does **not** edit your `hyprland.conf`: you add
the line `source = ~/.config/hypr/orbitfm.conf` yourself, otherwise the rules
disappear after the session restarts. Installation prints the ready-made line
and shows the target detail on the Integrations page.

```ini
windowrulev2 = float, class:^(org\.modernbyte\.OrbitFM)$
windowrulev2 = center, class:^(org\.modernbyte\.OrbitFM)$
windowrulev2 = stayfocused, class:^(org\.modernbyte\.OrbitFM)$
windowrulev2 = size 1100 800, title:^OrbitFM$
windowrulev2 = size 500 620, title:^OrbitFM · Overlay$
windowrulev2 = noborder, title:^OrbitFM · Overlay$
windowrulev2 = noshadow, title:^OrbitFM · Overlay$
windowrulev2 = size 420 152, title:^OrbitFM · Listening$
windowrulev2 = noborder, title:^OrbitFM · Listening$
windowrulev2 = noshadow, title:^OrbitFM · Listening$
windowrulev2 = size 400 640, title:^OrbitFM · Panel$
windowrulev2 = noborder, title:^OrbitFM · Panel$
windowrulev2 = noshadow, title:^OrbitFM · Panel$
windowrulev2 = noblur, title:^OrbitFM · Panel$
windowrulev2 = size 420 560, title:^OrbitFM · Settings · Compact$
windowrulev2 = size 760 544, title:^OrbitFM · Settings · Medium$
windowrulev2 = size 1040 720, title:^OrbitFM · Settings · Wide$
windowrulev2 = size 420 320, title:^OrbitFM · Sleep Warning$
```

The `windowrulev2` syntax works in Hyprland 0.52 and in the compatibility mode
of 0.53+, where it is marked as deprecated (the new format is
`windowrule { … }`). When updating Hyprland, check its documentation: the
deprecation warning is not an OrbitFM defect.

Removal takes away the rules file and the receipt; `hyprctl` does not cancel
rules already applied to a live session — they expire with the session.

## If the rules did not work

| Symptom | What to check |
|---|---|
| niri did not apply the rules | `config.kdl` has the line `include "./cfg/orbitfm.kdl"` and the file `cfg/orbitfm.kdl` exists; niri 26.04+ rereads the config itself, older versions need a restart |
| Hyprland loses the rules after a restart | the line `source = ~/.config/hypr/orbitfm.conf` is added to `hyprland.conf` — the installer does not write it |
| The capsule is stretched over the whole tile | the rule is not connected or the window title did not match; check `niri msg windows` |
| The settings or warning window is stretched over the whole tile | the window has an empty `App ID` (the rule requires it together with the title) — update the build to 0.21.0-rc.4 or newer |
| The window went under the panel | increase `x` in the niri rule so that the window center coincides with the widget icon |
| The window opens in the center of the screen | niri has no `relative-to="top-right"` anchor; reinstall the `niri` target |
| After removing niri it complains about `include` | the line was added manually before installation; remove it yourself or restore the rules file |

---

Next: [Panels and widgets](panels.md) ·
[Browser](browser.md) ·
[Window modes](window-modes.md) ·
[Troubleshooting](troubleshooting.md)
