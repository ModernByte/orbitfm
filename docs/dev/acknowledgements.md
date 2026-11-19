# Acknowledgements and dependencies

**Developer documentation** · [Documentation hub](../README.md) · [Developer hub](README.md)

ModernByte built OrbitFM on top of other people's good work and sees no reason
to hide it: almost every pleasant detail of the interface is someone's
long-standing maintenance. Thanks to the egui and `eframe` communities for
fitting a UI and a 3D globe into one window, and to `wgpu` for making it work on
ordinary graphics hardware. Thanks to the mpv project: `libmpv` has carried
playback for years, including the most capricious streams. Thanks to the Radio
Browser community for a free station catalog and mirrors — without it, clicking
a country would mean nothing. Thanks to Natural Earth for country borders and to
the people behind the Noto fonts — outlines and flags depend on them. And a
special thanks to noctalia.dev, niri and Hyprland: this is the environment
OrbitFM was designed for as a Linux-first application.

## Rust dependencies

Names and versions come from `Cargo.toml` and `Cargo.lock`.

| Crate | Version | Why it is in OrbitFM |
|-------|---------|----------------------|
| [eframe](https://docs.rs/eframe) | 0.37 | Window and application loop: `wayland`, `x11`, `wgpu`, `accesskit`, `system_fonts`, `default_fonts` |
| [egui-wgpu](https://docs.rs/egui-wgpu) | 0.37 | egui canvas over the wgpu globe scene |
| [epaint](https://docs.rs/epaint) | 0.37 | Custom UI painting; the `color_fonts` feature provides color flags |
| [wgpu](https://docs.rs/wgpu) | 30 | Globe sphere, markers, lighting and atmosphere |
| [geo](https://docs.rs/geo) | 0.33 | Point-in-polygon: which country is under a point on the sphere |
| [geojson](https://docs.rs/geojson) | 1.0 | Reading the border dataset `assets/geojson/world-110m.json` |
| [rstar](https://docs.rs/rstar) | 0.13 | R-tree over country bounding boxes for fast candidate selection |
| [glam](https://docs.rs/glam) | 0.34 | Camera matrices and vectors for uniforms |
| [bytemuck](https://docs.rs/bytemuck) | 1.25 | Safe casting of vertices and uniforms to bytes for buffers |
| [libmpv2](https://docs.rs/libmpv2) | 6.0 | Wrapper over the libmpv C API: playback, volume, events, reconnect |
| [playwire](https://crates.io/crates/playwire) | 2.0 | MPRIS on Linux, SMTC on Windows, Now Playing on macOS |
| [reqwest](https://docs.rs/reqwest) | 0.13 | HTTP to Radio Browser servers and favicon downloads (rustls, json) |
| [tokio](https://docs.rs/tokio) | 1.54 | Async runtime, channels, timeouts, DNS discovery of mirrors |
| [serde](https://docs.rs/serde) | 1.0 | Serialization of settings, library and API responses |
| [serde_json](https://docs.rs/serde_json) | 1.0 | `settings.json`, `library.json`, Radio Browser JSON and `status --json` |
| [thiserror](https://docs.rs/thiserror) | 2.0 | Typed errors inside modules |
| [anyhow](https://docs.rs/anyhow) | 1.0 | Errors and context at boundaries and during process startup |
| [tracing](https://docs.rs/tracing) | 0.1 | Subsystem logs |
| [tracing-subscriber](https://docs.rs/tracing-subscriber) | 0.3 | Console log output with `env-filter` |
| [image](https://docs.rs/image) | 0.25 | Decoding the Earth texture and favicons (JPEG/PNG/WebP) |
| [url](https://docs.rs/url) | 2.5 | Parsing and validating stream and favicon addresses |
| [libc](https://docs.rs/libc) | 0.2 | Setting the C locale so libmpv behaves correctly |
| [ksni](https://crates.io/crates/ksni) | 0.3.7 | StatusNotifierItem tray (Linux only, `tray` feature; Unlicense noted in the ADR) |
| [tray-icon](https://crates.io/crates/tray-icon) | 0.26.1 | macOS menu-bar icon (macOS only, `tray` feature) |
| [muda](https://crates.io/crates/muda) | 0.21.2 | macOS menu-bar menu and its updates (macOS only, `tray` feature) |
| [windows](https://docs.rs/windows) | 0.62 | Win32 owner window for SMTC (Windows only) |
| [tempfile](https://docs.rs/tempfile) | 3 | Temporary directories in tests (dev dependency) |

## Projects and data around OrbitFM

- [Radio Browser](https://www.radio-browser.info) and its
  [API documentation](https://docs.radio-browser.info/) — the station catalog,
  mirrors and click tracking.
- [Natural Earth](https://www.naturalearthdata.com) — `ne_110m_admin_0_countries`
  borders in `assets/geojson/world-110m.json`, public domain.
- Earth texture `assets/textures/earth.jpg` (2048×1024) and its detailed tier
  `assets/textures/earth@2x.jpg` (5400×2700) are embedded in the binary — NASA
  Blue Marble Next Generation, August, cloudless, public domain.
- [mpv](https://mpv.io) and its
  [embeddable libmpv](https://mpv.io/manual/master/#embedding-libmpv).
- [Noto Color Emoji](https://fonts.google.com/noto/specimen/Noto+Color+Emoji)
  for flags; CJK fonts come from the system.
- [Noctalia](https://noctalia.dev) and its
  [plugin documentation](https://docs.noctalia.dev/noctalia/plugins/development/)
  — the panel the widget was made for.
- [niri](https://niri-wm.github.io/niri/) and [Hyprland](https://hyprland.org) —
  the environments targeted by the window rules in `wm-configs/`.
- [Rust](https://www.rust-lang.org) — Edition 2024, `rust-version = 1.100`.
- [eframe / egui](https://github.com/emilk/egui).
- [cava](https://github.com/karlstav/cava) — the visual reference for the
  visualizer.
- `linuxdeploy` and `appimagetool` for AppImage, WiX for MSI.

## Assets and licenses

Third-party assets keep their own licenses; the project license (MIT) does not
override them. When adding an asset, record its origin and license in the pull
request and in this file.
