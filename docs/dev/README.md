# OrbitFM — developer documentation

**Developer documentation** · [Documentation hub](../README.md) · [User guide (English)](../user/en/index.md)

This is the entry point for maintainers and contributors. Everything here
describes how OrbitFM is built and why; user-facing behavior lives in the
[user guide](../user/en/index.md).

Architecture decisions are recorded **only** in the ADR series. If a change
affects a public contract, a module boundary or a platform policy, write or
update an ADR before the code.

## Start here

| Document | What it answers |
|----------|-----------------|
| [Architecture overview](../architecture/00-overview.md) | What the layers are, which module owns what, how data flows |
| [ADR index](adr-index.md) | English topic list for all 66 decision records |
| [ADR directory](../adr/) | Full decision records (Russian) |
| [Module specifications](../modules/) | Behavior, responsibilities and invariants of each module |
| [Interface contracts](../interfaces/) | Public contracts: IPC, schemas, module APIs |
| [Patterns catalog](../patterns/00-overview.md) | Reusable structural patterns used in the code |
| [Roadmap](../ROADMAP.md) | What is done, what is next, what is blocked |
| [Version map](../VERSIONMAP.md) | Which feature landed in which release |
| [Changelog](../CHANGELOG.md) | Release-by-release history |
| [Release notes template](release-notes.md) | How a GitHub Release body is written: Russian + English, preview paragraph, checklist |
| [Audits and research](../audit/) | Independent audits, security and performance research |
| [Plans](../plans/) | Active feature plans and measurement journals |
| [Agent rules](../AGENTS.md) | Mandatory workflow for AI agents and reviewers |
| [Contributing](../CONTRIBUTING.md) | Branches, commits, style, review |
| [Security policy](../../SECURITY.md) | Threat model and vulnerability reporting |

## Documentation layout

```text
docs/
├── README.md              # documentation hub (English default)
├── README.ru.md           # documentation hub (Russian)
├── user/
│   ├── en/                # user guide, English (default)
│   ├── ru/                # user guide, Russian
│   └── images/            # screenshots shared by both languages
├── dev/                   # this section: developer entry point and ADR index
├── releases/en/           # English release notes, one file per version
├── architecture/          # architecture overview
├── adr/                   # Architecture Decision Records (Russian)
├── modules/               # module specifications (Russian)
├── interfaces/            # interface contracts (Russian)
├── patterns/              # pattern catalog (Russian)
├── audit/                 # audits and research (Russian)
└── plans/                 # feature plans (Russian)
```

Rules that keep the tree honest:

1. **User pages never mention ADRs** and never link into `adr/`, `modules/` or
   `interfaces/`. If a user-visible behavior needs explanation, explain the
   behavior, not the decision behind it.
2. **ADR references belong to developer documents**, code comments and release
   notes. A single “For developers” link in the README is the only bridge from
   the user surface into this section.
3. **English is the default for user documentation.** Every user page carries a
   language switch and both trees use identical file names.
4. **Developer documents are written in Russian**, the project’s working
   language. The English ADR index exists so non-Russian readers can find the
   right record; the record itself stays authoritative.
5. **The GitHub wiki is generated, not hand-written.** `packaging/wiki-sync.sh`
   copies `docs/user/{en,ru}` into wiki pages (`Home`, `Install`, … and the same
   names with a `-ru` suffix), rewrites relative links, builds the bilingual
   `_Sidebar`/`_Footer` and refuses to finish if a page loses its language
   switch, links to a missing page or mentions an ADR. `--push` updates the wiki
   repository; edit the guide, never the wiki.

## Engineering rules in one screen

- **Linux first.** Design and verify on Linux (Wayland: niri, Hyprland) before
  other platforms.
- **Single instance.** One process. Closing the window hides it; `orbitfm quit`
  is the only real exit.
- **Layered dependencies.** Presentation → application core → domain services →
  infrastructure. No cycles, no UI types in domain modules.
- **File size.** Soft limit 300–400 lines of implementation; anything above
  800–1000 lines needs an explicit ADR and a split plan.
- **Public behavior changes** go through ADR + interface + module + CHANGELOG.
- **Error handling.** `thiserror` inside modules, `anyhow` at boundaries;
  `unwrap`/`expect` only with a proven invariant.
- **Measured performance.** Changes to the render or audio hot path need a
  measurement (frame stats, p50/p95), not an opinion.
- **Supply chain.** `cargo deny`, SHA-pinned GitHub Actions, vendored Flatpak
  crates.

## Checks before a pull request

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
packaging/cargo-deny-check.sh
packaging/check-docs.sh
packaging/wiki-sync.sh --check
```

Platform packaging lives in `packaging/` (AppImage, Flatpak, MSI, DMG). CI runs
Linux, Windows and macOS jobs, including an Intel macOS runner and a real
launch smoke test for the DMG.
