---
name: orbitfm-integration
description: Интеграции OrbitFM с окружением — Noctalia v5, MPRIS/playwire, window rules Niri и Hyprland. Использовать при работе с плагином, медиа-клавишами и правилами окон.
---

# OrbitFM Integration

Ты работаешь с внешними интеграциями OrbitFM.

Опирайся на ADR-0006, ADR-0007, `docs/modules/06-media-controls.md`, `docs/modules/08-noctalia-wm.md`.

## Noctalia v5

- Плагин — тонкий адаптер, без бизнес-логики.
- `plugin.toml`: `id = "modernbyte/orbitfm"` (формат `<author>/<plugin>` обязателен), `plugin_api = 3`, author = ModernByte.
- Виджет при клике: `noctalia.runAsync("orbitfm toggle", callback)`; `exec_async` не существует.
- Структура: `orbitfm/plugin.toml`, `plugin.luau`, `README.md`, `thumbnail.webp`, `translations/en.json`, `translations/ru.json`.
- Имя каталога совпадает с частью id после `/` — требование каталога Noctalia.
- Теги только из официального списка (`bar`, `audio`, `music`); `orbitfm` объявлен в `dependencies`.
- Переводы — вложенные объекты (`widget.label`), плоские ключи с точками не принимаются.
- Проверка: `noctalia plugins lint orbitfm`.
- Каталог: `catalog.toml` в корне репозитория; подключение
  `noctalia msg plugins source add orbitfm git https://github.com/ModernByte/orbitfm`
  и `noctalia msg plugins enable modernbyte/orbitfm`.
- Внешняя публикация в `noctalia-dev/community-plugins` не выполняется: плагин
  распространяется как git-источник. PR туда — только по новому решению владельца.

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
