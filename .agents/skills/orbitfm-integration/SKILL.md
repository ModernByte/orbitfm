---
name: orbitfm-integration
description: Интеграции OrbitFM с окружением — Noctalia v5, MPRIS/playwire, window rules Niri и Hyprland. Использовать при работе с плагином, медиа-клавишами и правилами окон.
---

# OrbitFM Integration

Ты работаешь с внешними интеграциями OrbitFM.

Опирайся на ADR-0006, ADR-0007, ADR-0051, `docs/modules/06-media-controls.md`,
`docs/modules/08-noctalia-wm.md`, `docs/modules/16-integrations.md`.

## Noctalia v5

- Плагин — тонкий адаптер, без бизнес-логики.
- `plugin.toml`: `id = "modernbyte/orbitfm"` (формат `<author>/<plugin>` обязателен), `plugin_api = 3`, author = ModernByte.
- Виджет при клике: `noctalia.runAsync("orbitfm panel")` — без колбэка (вариант с callback
  имеет таймаут 5 с и убивает группу процессов, поэтому GUI запускается отсоединённо);
  `exec_async` не существует.
- Точки входа разведены командами, а не Luau: бар-виджет зовёт `panel`, ярлык
  приложения (`org.modernbyte.OrbitFM.desktop`, `Exec=orbitfm mode focus`) всегда
  даёт `Focus`, капсулу `Overlay` открывает `overlay` из CLI (ADR-0033, ред. 2026-10-28).
- Структура: `orbitfm/plugin.toml`, `plugin.luau`, `README.md`, `thumbnail.webp`, `translations/en.json`, `translations/ru.json`.
- Имя каталога совпадает с частью id после `/` — требование каталога Noctalia.
- Теги только из официального списка (`bar`, `audio`, `music`); `orbitfm` объявлен в `dependencies`.
- Переводы — вложенные объекты (`widget.label`), плоские ключи с точками не принимаются.
- Проверка: `noctalia plugins lint orbitfm`.
- Каталог: `catalog.toml` в корне репозитория; подключение
  `noctalia msg plugins source add orbitfm git https://github.com/ModernByte/orbitfm`
  и `noctalia msg plugins enable modernbyte/orbitfm`. Те же два шага делает
  цель `noctalia` в установщике и кнопка «Установить» на странице
  «Интеграции» (ADR-0051 §2): файлы не копируются, скачивание — работа оболочки.
- `noctalia msg plugins install` не существует; `source add` + `enable` —
  единственный путь. `source remove` оставляет виджет выключенным даже при
  живом git-источнике, поэтому снятие цели возвращает прежнюю включённость
  (`was_enabled` в квитанции), а не просто выключает.
- `path`-источник с тем же id перекрывает git-источник — проверено живым
  прогоном: канал обновлений пропадает. Поэтому цель работает только с
  официальным git-источником.
- Внешняя публикация в `noctalia-dev/community-plugins` не выполняется: плагин
  распространяется как git-источник. PR туда — только по новому решению владельца.

Следуй официальной документации: https://docs.noctalia.dev/noctalia/plugins/development/

## Media Controls

- Предпочтительно `playwire` (Linux MPRIS + Windows SMTC + macOS Now Playing).
- Fallback: `souvlaki`.
- Реализуй контракт `MediaControls` из `docs/interfaces/06-media.md`.
- Минимальный MPRIS: Identity, PlaybackStatus, Metadata, Volume, CanPlay/CanPause.

## Window Rules

- Niri / Hyprland правила поставляются в `wm-configs/` и ставятся целями
  `niri` и `hyprland` (ADR-0051 §2) — та же кнопка, что и у остальных целей.
- app-id / class должны быть стабильными: `org.modernbyte.OrbitFM`.
- Не генерировать правила без необходимости; набор фиксирован кодом.
- `niri` — единственный чужой конфиг, который правит установщик: ровно одна
  строка `include "./cfg/orbitfm.kdl"` в `config.kdl`, копия
  `config.kdl.orbitfm.bak` перед первой правкой, идемпотентность и снятие
  ровно своей целой строки.
- Hyprland `hyprland.conf` не правит (синтаксис include не верифицирован,
  новая версия переезжает на Lua): правила ставятся на сеанс через
  `hyprctl keyword windowrulev2`, а строку `source = …` показывают пользователю.
- argv внешних команд строит только код модуля — фиксированным вектором, без
  shell-оболочки и без внешних строк (ADR-0033, ADR-0051 §5).
- Тестовая песочница: `ORBITFM_INTEGRATION_ROOT` переводит цели в
  `<root>/targets/<id>` и не даёт командовать живой оболочке.

## Локальная выкатка

- Интеграции ставятся только по обнаруженным целям: перед `integration install`
  смотреть автообнаружение страницы «Интеграции», а не раскладывать `--force`
  все 17 целей. На машине приёмки это `chrome`, `firefox`, `noctalia` и `niri`.
- Снятая цель чистится `integration uninstall <цель>`; квитанция позволяет
  удалить только то, что поставила: файлы, добавленные строки `lines[]` и
  опустевшие каталоги `dirs[]` (ADR-0051 §4).
- Обнаружение не имеет права кормиться следом установщика: файлы правил —
  только в каталогах оболочки, а пустой каталог, созданный нами, снимается.

## Чего нельзя

- Делать Noctalia-плагин «умным» (логика плеера только в основном процессе)
- Игнорировать DesktopEntry / app-id
- Реализовывать MPRIS вручную через zbus без необходимости, если playwire покрывает задачу
- Ставить `--force` цели, которых на машине нет: квитанция есть — бейдж
  «установлено» тоже, а реальной оболочки нет (ADR-0042 §1, ред. 2)
- Править чужие конфиги сверх одной строки `include` у niri: Hyprland и прочие
  получают готовый сниппет или runtime-команду, но не редакцию файла
  (ADR-0037, ADR-0051 §5)
- Собирать argv внешних команд из внешних строк — только фиксированный вектор
  из кода модуля (ADR-0033)
