# OrbitFM documentation

**English** · [Русский](README.ru.md)

This page is the entry point for all OrbitFM documentation. Pick the section
that matches what you need.

## For users

Step-by-step guides for installing and listening. No developer knowledge is
required beyond opening a terminal for the CLI examples.

| Language | Start here |
|----------|------------|
| **English** (default) | [OrbitFM user guide](user/en/index.md) |
| **Русский** | [Руководство пользователя](user/ru/index.md) |

The same guide covers installation on Linux, Windows and macOS, the first-run
wizard, window modes, Settings, keyboard and media keys, integrations
(Noctalia, Plasma 6, GNOME, Waybar and others), the browser panel, window rules,
the tray and MPRIS, ten real use cases, troubleshooting and a glossary.

Every page has a language switch at the top: English pages link to their Russian
counterpart and vice versa. The user guide deliberately contains no architecture
decisions or internal design notes — if you are looking for those, use the
developer section below.

Most requested pages:

| Topic | Page |
|-------|------|
| Install, update and remove | [Installation](user/en/install.md) |
| Something does not work | [Troubleshooting](user/en/troubleshooting.md) |
| Settings, theme and language | [Settings and language](user/en/settings.md) |
| What goes online and what stays on disk | [Security and privacy](user/en/privacy.md) |
| An unfamiliar term | [Glossary](user/en/glossary.md) |

## For developers

| Document | What is inside |
|----------|----------------|
| [Developer documentation](dev/README.md) | Entry point: architecture, decisions, module and interface contracts, engineering rules |
| [Architecture overview](architecture/00-overview.md) | Layers, module boundaries, data flow |
| [Architecture Decision Records](adr/) | Every accepted architectural decision, with context, alternatives and consequences |
| [Module specifications](modules/) | Per-module behavior and responsibilities |
| [Interface contracts](interfaces/) | Public contracts between modules, IPC, file schemas |
| [Patterns catalog](patterns/00-overview.md) | Reusable structural patterns used in the codebase |
| [Roadmap](ROADMAP.md) · [Version map](VERSIONMAP.md) · [Changelog](CHANGELOG.md) | Plans, release contents and history |
| [Audits and research](audit/) | Security, performance and product research |
| [Acknowledgements](dev/acknowledgements.md) | Dependencies, licenses and asset credits |
| [Agent rules](AGENTS.md) · [Contributing](CONTRIBUTING.md) | Workflow, code style and review rules |

Architecture Decision Records are the only place where architectural decisions
are recorded. They are written for maintainers and are not part of the user
guide; the user-facing pages describe behavior, not the reasoning behind it.

## Documentation languages

- User documentation is bilingual: English is the default, Russian is a
  first-class translation. Both trees use the same file names, so switching
  languages keeps you on the same page.
- Developer documentation (architecture, ADRs, modules, interfaces, patterns)
  is written in Russian, the working language of the project.
- UI strings are localized separately in the application itself (seventeen
  languages, including English and Russian); see
  [Settings and language](user/en/settings.md).

## Legal

OrbitFM is distributed under the MIT License, “as is”, without warranties. The
full disclaimer in English and Russian is in [LEGAL.md](../LEGAL.md); the license
text is in [LICENSE](../LICENSE).
