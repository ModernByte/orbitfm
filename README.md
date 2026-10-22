# OrbitFM

**3D Interactive Global Radio Player**  
Под эгидой [ModernByte](https://github.com/ModernByte)

OrbitFM — кроссплатформенный десктоп-плеер интернет-радио с интерактивным 3D-глобусом.  
Приоритетная платформа — Linux (Niri, Hyprland, Noctalia v5). Также поддерживаются Windows и macOS.

## Основные возможности

- Интерактивный 3D-глобус с выбором страны (raycasting + Point-in-Polygon)
- Воспроизведение станций из Radio Browser
- Режим standalone-окна и overlay (через IPC)
- Интеграция с Noctalia v5 (виджет панели)
- MPRIS / системные медиа-клавиши
- Single-instance архитектура

## Статус

**Этап 2 реализован:** аудио-ядро на libmpv (play / pause / stop / volume / реконнект),
MPRIS v2 на Linux и управление через IPC. Каталог станций Radio Browser и команда
`play <uuid>` — следующий этап.

Требования для сборки:

- Rust 1.95+
- Linux с Wayland/X11
- системная библиотека libmpv с заголовками
  (Debian/Ubuntu: `libmpv-dev`, Arch: `mpv`, Fedora: `mpv-libs-devel`)

Подробный план — [ROADMAP](docs/ROADMAP.md), изменения — [CHANGELOG](docs/CHANGELOG.md).

## Документация

| Документ | Описание |
|----------|----------|
| [SRS](orbitfm_srs.md) | Технические требования и инженерная спецификация |
| [Архитектура](docs/architecture/00-overview.md) | Высокоуровневый обзор |
| [ADR](docs/adr/) | Architecture Decision Records |
| [ROADMAP](docs/ROADMAP.md) | План развития |
| [CONTRIBUTING](docs/CONTRIBUTING.md) | Правила разработки и стиль кода |
| [AGENTS](docs/AGENTS.md) | Правила работы AI-агентов |

## Стек (кратко)

- **Язык:** Rust (Edition 2024)
- **GUI:** eframe + egui + egui-wgpu
- **Аудио:** libmpv (безопасная обёртка `libmpv2`)
- **Гео:** geo + geojson + rstar
- **Сеть:** reqwest + Radio Browser API
- **Интеграции:** Noctalia (Luau), MPRIS (playwire)

Подробности — в ADR и SRS.

## Лицензия

MIT

---

© ModernByte
