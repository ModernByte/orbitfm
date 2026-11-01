# Модуль: Noctalia Plugin и Window Manager Rules

**Путь в коде:**
- Плагин: `orbitfm/` и каталог `catalog.toml`
- Правила WM: `wm-configs/` и `src/wm/` (опциональная генерация)

**Ответственность:** Тонкая интеграция с Noctalia v5 и оконными менеджерами.

---

## 1. Noctalia Plugin

### Структура

```
orbitfm/
├── plugin.toml
├── plugin.luau
├── README.md
├── thumbnail.webp
└── translations/
    ├── en.json
    └── ru.json
```

Имя каталога совпадает с частью id после `/`: этого требует каталог Noctalia.
`catalog.toml` в корне репозитория индексирует плагин для git-источника.

### plugin.toml

```toml
id           = "modernbyte/orbitfm"
name         = "OrbitFM"
version      = "1.1.0"
plugin_api   = 3
author       = "ModernByte"
license      = "MIT"
icon         = "radio"
description  = "3D Interactive Global Radio Player"
tags         = ["bar", "audio", "music"]
dependencies = ["orbitfm"]

[[widget]]
id    = "launcher"
entry = "plugin.luau"

  [[widget.setting]]
  key             = "display"
  type            = "select"
  label_key       = "settings.display.label"
  description_key = "settings.display.description"
  default         = "icon_text"
  options = [
    { value = "icon", label_key = "settings.display.option.icon" },
    { value = "icon_text", label_key = "settings.display.option.icon_text" },
  ]
```

Noctalia требует `id` в формате `<author>/<plugin>`; `org.modernbyte.orbitfm`
использоваться не может. app-id окна остаётся `org.modernbyte.OrbitFM`.

### Поведение

Настройка `display` управляет содержимым: `icon_text` (по умолчанию) показывает
глиф и подпись, `icon` — только глиф. Подпись берётся из
`widget.label`, подсказка — из `widget.tooltip` плюс текущее состояние.

Виджет раз в 2 секунды вызывает `orbitfm status` с колбэком и разбирает первую
строку `status <state>`; exit code ≠ 0 означает, что приложение не запущено.
Состояние меняет только оформление:

| `status` | Глиф | Цвет |
|---|---|---|
| `stopped` / приложение не запущено | `radio` | цвет темы (`on_surface`) |
| `playing`, `buffering` | `radio` | зелёный `#22C55E` |
| `paused` | `radio` | цвет темы (`on_surface`) |
| `reconnecting` | `radio-off` | тёмно-серый `#6B7280` |
| `error` | `radio` | красный `#EF4444` |

Это презентационный опрос: плагин не владеет воспроизведением и не хранит
метаданные, источник истины остаётся в процессе OrbitFM. `runAsync` с колбэком
применяется только к короткому `status`; запуск GUI по-прежнему идёт без
колбэка, потому что таймаут 5 с убивает группу процессов.

При клике выполняется:

```luau
local started = noctalia.runAsync("orbitfm panel")

if not started then
    noctalia.notify(noctalia.tr("widget.error_title"), noctalia.tr("widget.error_body"))
end
```

`runAsync` без колбэка запускает `orbitfm` через `/bin/sh -c` полностью
отсоединённым (double-fork + setsid); бинарник должен быть в `PATH`. Форма с
колбэком здесь неприменима: Noctalia ограничивает такую команду таймаутом
`kDefaultCommandTimeout = 5000 мс` и по истечении убивает всю группу процессов
вместе с только что поднятым GUI. Поэтому для `panel` виджет проверяет лишь факт
запуска команды, а ошибки работы плеера остаются в логах OrbitFM. Плагин **не**
владеет состоянием плеера и не содержит бизнес-логики; опрос `status` нужен
только для цвета и глифа. Команда `panel` — переключатель выпадающей
панели: открытая панель своего режима скрывается, другой режим переключается на
`Panel` и показывается, а при отсутствии экземпляра команда поднимает OrbitFM
сразу с видимой панелью (семантика CLI). Капсулу Overlay той же механикой зовёт
команда `overlay` — из CLI и переключателя «Компоновка»; полный стол Focus —
ярлык приложения с `Exec=orbitfm mode focus` (ADR-0033, ред. 2026-10-28).

`tags` выбираются только из официального списка (здесь `bar`, `audio`, `music`),
а `dependencies = ["orbitfm"]` объявляет внешнюю команду, которую запускает
виджет. Переводы используют вложенные ключи (`widget.label`), а не плоские
ключи с точками.

Проверка манифеста:

```bash
noctalia plugins lint orbitfm
```

### Каталог и публикация

- `catalog.toml` в корне содержит строку `[[plugin]]` с `id`, `name`, `version`,
  `author`, `license`, `icon`, `description`, `deprecated`, `plugin_api`,
  `tags` и `dependencies`; хост читает её для витрины и проверки совместимости.
- Обязательные файлы публикации: `plugin.toml`, entry-скрипт, `README.md`,
  `thumbnail.webp` (960×540 WebP, до 512 КиБ) и `translations/en.json`.
  README обязан содержать разделы `## Plugin`, `## Requirements` (упоминание
  `orbitfm`) и `## Usage`.
- Подключение как источника:
  `noctalia msg plugins source add orbitfm git https://github.com/ModernByte/orbitfm`,
  затем `noctalia msg plugins enable modernbyte/orbitfm`.
- Внешняя публикация в `noctalia-dev/community-plugins` не выполняется: плагин
  распространяется как git-источник `ModernByte/orbitfm`. Если решение изменится,
  понадобится отдельный PR; в общем магазине `catalog.toml` генерируется CI.
- Локальная проверка: community-валидатор из
  `.github/workflows/scripts/validate-plugins.py` на копии каталога плагина —
  `python3 validate-plugins.py --root <staging>` (0 ошибок).

## 2. Window Rules

### Niri

`wm-configs/orbitfm-niri.kdl` содержит по правилу на каждый режим; правило
Focus — то, что открывает ярлык приложения (`orbitfm mode focus`), панель
виджета плагин зовёт командой `orbitfm panel`, капсулу Overlay и остальные
режимы — `orbitfm overlay` и `orbitfm mode overlay|listening`:

```kdl
window-rule {
    match app-id="org.modernbyte.OrbitFM" title=r"^OrbitFM$"
    open-floating true
    default-column-width { fixed 1100; }
    default-window-height { fixed 800; }
    default-floating-position x=60 y=48 relative-to="top-right"
    draw-border-with-background false
}

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

window-rule {
    match app-id="org.modernbyte.OrbitFM" title=r"^OrbitFM · Listening$"
    open-floating true
    default-column-width { fixed 420; }
    default-window-height { fixed 152; }
    default-floating-position x=310 y=48 relative-to="top-right"
    draw-border-with-background false
    border { off; }
    shadow { off; }
}

window-rule {
    match app-id="org.modernbyte.OrbitFM" title=r"^OrbitFM · Panel$"
    open-floating true
    default-column-width { fixed 400; }
    default-window-height { fixed 640; }
    default-floating-position x=310 y=48 relative-to="top-right"
    draw-border-with-background false
    focus-ring { off; }
    border { off; }
    shadow { off; }
}
```

Плавающие окна Niri центрирует по умолчанию, поэтому без
`default-floating-position` окно появляется в центре экрана. Якорь
`top-right` ставит окно под панелью Noctalia (её высота 36 px, отсюда `y=48`)
и по центру виджета OrbitFM. Отступы рассчитаны для панели 1920 px, где центр
виджета примерно в 520 px от правого края: центр окна — это
`output_right − x − ширина/2`, поэтому капсулы центрируются под виджетом при
`x=270` для Overlay шириной 500 px (правка 2026-10-28) и при `x=310` для
420-px Listening; `270 + 250 = 310 + 210 = 520` — оба центра в одной точке.
Panel (400 px) держится `x=310`. Focus при `x=60` — стол шириной 1100 px
центрироваться
под виджетом не может и просто держится у правого края. При другой ширине экрана
или расположении виджета отступы подбираются так же. Не-Focus правила дополнительно
выключают `border` и `shadow`: окно режима Focus — единственное с рамкой.

Wayland не позволяет обычному top-level окну позиционировать себя самому, а
Plugin API Noctalia не отдаёт прямоугольник виджета, поэтому точная привязка
«под виджетом» недостижима: композитор ставит окно по статическому правилу.
`relative-to="center"` в Niri не поддерживается, допустимые значения — края и
углы экрана/выхода.

Файл проверен командой `niri validate -c wm-configs/orbitfm-niri.kdl`.
Локально его удобно положить отдельным include, не меняя основной конфиг:

```kdl
// ~/.config/niri/config.kdl
include "./cfg/orbitfm.kdl"
```

Без этих правил niri тайлит капсулы на всю высоту, и содержимое Overlay/
Listening растягивается. Приёмочная проверка 2026-10-24: `orbitfm toggle`
через изолированный runtime даёт Focus 920×660 floating, `mode overlay` —
420×620 floating с позицией `[1188, 84]` (x = 1920 − 310 − ширина окна,
y = 36 + 48), `mode listening` — 420×152 floating. Замеры сделаны до правки
ширины Overlay 2026-10-28: с `fixed 500` и `x=270` позиция Overlay —
`[1150, 84]` (1920 − 270 − 500). Приёмка 2026-10-27: пресет
Focus поднят до 1100×800 (`WINDOW_SIZE` в `src/session/window.rs`, `fixed 1100` /
`fixed 800` в niri и `size 1100 800` в Hyprland), и живая проверка этой геометрии
по правилу WM ещё не выполнена — пункт остаётся открытым в `docs/ROADMAP.md`
(«Пакет окон, колеса и переработки настроек»).

### Hyprland

`wm-configs/orbitfm-hyprland.conf`:

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
```

Синтаксис `windowrulev2` работает в Hyprland 0.52 и совместимом режиме 0.53+.
Начиная с 0.53 он помечен deprecated (новый формат — `windowrule { ... }`), поэтому
при обновлении файла нужно проверить актуальную документацию Hyprland.

Файлы правил поставляются в `wm-configs/` и копируются пользователем. Модуль
`src/wm/` для автоматической генерации пока не реализован.

## 3. Принципы

- Плагин Noctalia — максимально тонкий: `panel` через IPC, `status` только для оформления
- Все команды идут через существующий IPC (`orbitfm panel`, `orbitfm toggle`, `orbitfm status` и т.д.)
- app-id / class должны быть стабильными и соответствовать Desktop Entry

## 4. Связанные ADR

- ADR-0007: Интеграция с Noctalia v5
- ADR-0005: Single-instance и IPC
