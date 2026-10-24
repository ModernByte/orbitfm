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
version      = "1.0.0"
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
```

Noctalia требует `id` в формате `<author>/<plugin>`; `org.modernbyte.orbitfm`
использоваться не может. app-id окна остаётся `org.modernbyte.OrbitFM`.

### Поведение

Скрипт задаёт глиф `radio`, текст «OrbitFM» и подсказку из переводов. При клике
выполняется:

```luau
noctalia.runAsync("orbitfm toggle", function(result)
    if result.exitCode ~= 0 then
        noctalia.notify(noctalia.tr("widget.error_title"), result.stderr)
    end
end)
```

`runAsync` запускает `orbitfm` через `/bin/sh -c`; бинарник должен быть в `PATH`.
Если процесс не запустился, виджет показывает уведомление. Плагин **не** хранит
состояние плеера и не содержит бизнес-логики; `toggle` при отсутствии экземпляра
запускает OrbitFM с видимым окном (семантика CLI).

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
Focus — то, что открывает плагин через `orbitfm toggle`, капсулы — через
`orbitfm mode overlay|listening`:

```kdl
window-rule {
    match app-id="org.modernbyte.OrbitFM" title=r"^OrbitFM$"
    open-floating true
    default-column-width { fixed 920; }
    default-window-height { fixed 660; }
    draw-border-with-background false
}

window-rule {
    match app-id="org.modernbyte.OrbitFM" title=r"^OrbitFM · Overlay$"
    open-floating true
    default-column-width { fixed 420; }
    default-window-height { fixed 620; }
    draw-border-with-background false
}

window-rule {
    match app-id="org.modernbyte.OrbitFM" title=r"^OrbitFM · Listening$"
    open-floating true
    default-column-width { fixed 420; }
    default-window-height { fixed 152; }
    draw-border-with-background false
}
```

Плавающие окна Niri центрирует по умолчанию. `default-floating-position`
не используется: допустимые значения `relative-to` не включают `center`.
Файл проверен командой `niri validate -c wm-configs/orbitfm-niri.kdl`.
Локально его удобно положить отдельным include, не меняя основной конфиг:

```kdl
// ~/.config/niri/config.kdl
include "./cfg/orbitfm.kdl"
```

Без этих правил niri тайлит капсулы на всю высоту, и содержимое Overlay/
Listening растягивается. Приёмочная проверка 2026-10-24: `orbitfm toggle`
через изолированный runtime даёт Focus 920×660 floating, `mode overlay` —
420×620 floating, `mode listening` — 420×152 floating.

### Hyprland

`wm-configs/orbitfm-hyprland.conf`:

```ini
windowrulev2 = float, class:^(org\.modernbyte\.OrbitFM)$
windowrulev2 = center, class:^(org\.modernbyte\.OrbitFM)$
windowrulev2 = stayfocused, class:^(org\.modernbyte\.OrbitFM)$
windowrulev2 = size 920 660, title:^OrbitFM$
windowrulev2 = size 420 620, title:^OrbitFM · Overlay$
windowrulev2 = size 420 152, title:^OrbitFM · Listening$
```

Синтаксис `windowrulev2` работает в Hyprland 0.52 и совместимом режиме 0.53+.
Начиная с 0.53 он помечен deprecated (новый формат — `windowrule { ... }`), поэтому
при обновлении файла нужно проверить актуальную документацию Hyprland.

Файлы правил поставляются в `wm-configs/` и копируются пользователем. Модуль
`src/wm/` для автоматической генерации пока не реализован.

## 3. Принципы

- Плагин Noctalia — максимально тонкий
- Все команды идут через существующий IPC (`orbitfm toggle` и т.д.)
- app-id / class должны быть стабильными и соответствовать Desktop Entry

## 4. Связанные ADR

- ADR-0007: Интеграция с Noctalia v5
- ADR-0005: Single-instance и IPC
