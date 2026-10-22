# Модуль: Noctalia Plugin и Window Manager Rules

**Путь в коде:**  
- Плагин: `noctalia-plugin/`  
- Правила WM: `wm-configs/` и `src/wm/` (опциональная генерация)

**Ответственность:** Тонкая интеграция с Noctalia v5 и оконными менеджерами.

---

## 1. Noctalia Plugin

### Структура

```
noctalia-plugin/
├── plugin.toml
├── plugin.luau
└── translations/
    ├── en.json
    └── ru.json
```

### plugin.toml

```toml
id          = "modernbyte/orbitfm"
name        = "OrbitFM"
version     = "1.0.0"
plugin_api  = 3
author      = "ModernByte"
license     = "MIT"
icon        = "radio"
description = "3D Interactive Global Radio Player"
tags        = ["radio", "audio", "orbitfm"]

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

Проверка манифеста:

```bash
noctalia plugins lint noctalia-plugin
```

## 2. Window Rules

### Niri

`wm-configs/orbitfm-niri.kdl`:

```kdl
window-rule {
    match app-id="org.modernbyte.OrbitFM"
    open-floating true
    default-column-width { fixed 920; }
    default-window-height { fixed 660; }
    draw-border-with-background false
}
```

Плавающие окна Niri центрирует по умолчанию. `default-floating-position`
не используется: допустимые значения `relative-to` не включают `center`.
Файл проверен командой `niri validate -c wm-configs/orbitfm-niri.kdl`.

### Hyprland

`wm-configs/orbitfm-hyprland.conf`:

```ini
windowrulev2 = float, class:^(org\.modernbyte\.OrbitFM)$
windowrulev2 = size 920 660, class:^(org\.modernbyte\.OrbitFM)$
windowrulev2 = center, class:^(org\.modernbyte\.OrbitFM)$
windowrulev2 = stayfocused, class:^(org\.modernbyte\.OrbitFM)$
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
