# Security and privacy

**English** · [Русский](../ru/privacy.md)

The full threat model lives in [SECURITY.md](../../../SECURITY.md). This page
covers what is useful to know without opening that document.

- **Any process running as your user can control the player.** Commands travel
  over a local transport: the socket `$XDG_RUNTIME_DIR/orbitfm.sock` with mode
  `0600` on Linux, `$TMPDIR/orbitfm-<uid>/orbitfm.sock` on macOS, and
  `\\.\pipe\orbitfm-<profile hash>` on Windows. The socket directory belongs to
  the user and is closed to everyone else; the Windows pipe name is unique per
  profile. There is no privilege separation between processes of the same user,
  and there cannot be one — this is a user-session application, not a service.
  OrbitFM listens on no network ports at all.
- **Stations are untrusted data.** Names, countries and tags from the shared
  catalog are stripped of control sequences and bidirectional characters, and
  panels and bars receive them without markup rights: a station name cannot
  recolor someone else's bar or rewrite a terminal line.
- **Only what is necessary leaves the machine:** requests to Radio Browser
  mirrors (search, station details), the provider's click counter when a station
  starts, the stream itself and the artwork URL. Resolving a link through an
  explicit `play-url` reads a playlist or a page, but no more than 1 MiB. There
  is no telemetry, analytics, background polling or self-update; the User-Agent
  is `OrbitFM/1.0 (ModernByte; Linux; Rust)`.
- **Artwork is fetched by policy, not blindly.** The `favicon` link comes from a
  third-party catalog, so the address passes two tiers: a syntactic one (private
  and reserved ranges, `localhost`, `*.local`, `*.internal`, single-label hosts
  such as `http://router/`, cloud metadata) and one over DNS addresses — all of
  them must be public, and every redirect is checked again. Only `https` works
  by default: an `http` address is upgraded to `https`, and the explicit “Allow
  HTTP artwork” switch is the owner's deliberate choice. The same DNS verdict
  decides whether an address may appear in MPRIS/SMTC/Now Playing metadata: it
  is cached for 60 seconds, so a domain that resolves into an internal network
  never reaches `artUrl`.
- **A long session does not grow without bound.** The artwork cache holds at most
  256 entries and at most eight in-flight downloads, a response body is capped at
  512 KiB, and an image is capped at 512 px per side.
- **Release artifacts are unsigned.** `SHA256SUMS` contains checksums, not a
  signature: the project has no certificates, so verify what you downloaded
  against the sums and take builds only from the Releases section.

## What is stored on disk

| File | Contents | Sent anywhere |
|------|----------|---------------|
| `~/.config/orbitfm/settings.json` | theme, language, volume, layout, network profile, sleep settings | no |
| `~/.local/share/orbitfm/library.json` | favorites, history, manual station links | no |
| `~/.local/share/orbitfm/stats.json` | listening minutes by station, country and tag | no |
| `~/.local/share/orbitfm/cache/stations-snapshot.json` | offline catalog snapshot | no |

There are no accounts, no cloud sync and no telemetry. Only requests to the
catalog, the selected station's stream and its artwork URL leave the machine —
as with any network request, the broadcaster can see your IP address.

## Reporting a vulnerability

Do not open a public issue: report it privately following
[SECURITY.md](../../../SECURITY.md), section 11.

Next: [Troubleshooting](troubleshooting.md) · [Installation](install.md) ·
[Home](index.md)
