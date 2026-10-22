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
    └── en.json
```

### plugin.toml (канон)

```toml
id         = "org.modernbyte.orbitfm"
name       = "OrbitFM"
version    = "1.0.0"
plugin_api = 3
author     = "ModernByte"
icon       = "radio-symbolic"
description = "3D Interactive Global Radio Player"

[[widget]]
id    = "launcher"
entry = "plugin.luau"
```

### Поведение

Виджет при клике выполняет:

```luau
noctalia.exec_async("orbitfm toggle")
```

Плагин **не** хранит состояние плеера и не содержит бизнес-логики.

## 2. Window Rules

### Niri (пример)

```kdl
window-rule {
    match app-id="org.modernbyte.OrbitFM"
    open-floating true
    default-column-width { fixed 920; }
    default-window-height { fixed 660; }
    draw-border-with-background false
}
```

### Hyprland (пример)

```ini
windowrulev2 = float, class:^(org\.modernbyte\.OrbitFM)$
windowrulev2 = size 920 660, class:^(org\.modernbyte\.OrbitFM)$
windowrulev2 = center, class:^(org\.modernbyte\.OrbitFM)$
```

Файлы правил поставляются в `wm-configs/` и могут копироваться пользователем или генерироваться модулем `src/wm/`.

## 3. Принципы

- Плагин Noctalia — максимально тонкий
- Все команды идут через существующий IPC (`orbitfm toggle` и т.д.)
- app-id / class должны быть стабильными и соответствовать Desktop Entry

## 4. Связанные ADR

- ADR-0007: Интеграция с Noctalia v5
- ADR-0005: Single-instance и IPC
