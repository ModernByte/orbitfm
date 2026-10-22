---
name: orbitfm-integration
description: Интеграции OrbitFM с окружением — Noctalia v5, MPRIS/playwire, window rules Niri и Hyprland. Использовать при работе с плагином, медиа-клавишами и правилами окон.
---

# OrbitFM Integration

Ты работаешь с внешними интеграциями OrbitFM.

Опирайся на ADR-0006, ADR-0007, `docs/modules/06-media-controls.md`, `docs/modules/08-noctalia-wm.md`.

## Noctalia v5

- Плагин — тонкий адаптер, без бизнес-логики.
- `plugin.toml`: `id = "org.modernbyte.orbitfm"`, `plugin_api = 3`, author = ModernByte.
- Виджет при клике: `noctalia.exec_async("orbitfm toggle")`.
- Структура: `noctalia-plugin/plugin.toml`, `plugin.luau`, `translations/`.

Следуй официальной документации: https://docs.noctalia.dev/noctalia/plugins/development/

## Media Controls

- Предпочтительно `playwire` (Linux MPRIS + Windows SMTC + macOS Now Playing).
- Fallback: `souvlaki`.
- Реализуй контракт `MediaControls` из `docs/interfaces/06-media.md`.
- Минимальный MPRIS: Identity, PlaybackStatus, Metadata, Volume, CanPlay/CanPause.

## Window Rules

- Niri / Hyprland правила поставляются в `wm-configs/`.
- app-id / class должны быть стабильными: `org.modernbyte.OrbitFM`.
- Не генерировать правила без необходимости; пользователь может скопировать готовые.

## Чего нельзя

- Делать Noctalia-плагин «умным» (логика плеера только в основном процессе)
- Игнорировать DesktopEntry / app-id
- Реализовывать MPRIS вручную через zbus без необходимости, если playwire покрывает задачу
