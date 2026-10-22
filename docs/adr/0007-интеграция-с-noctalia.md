# ADR-0007: Интеграция с Noctalia v5

**Статус:** Принято  
**Дата:** 2026-10-21  
**Участники решения:** ModernByte Architecture, Юрий Анатольевич  

## Контекст

Один из ключевых сценариев OrbitFM — управление из панели Noctalia v5 (клик по виджету → показать/скрыть окно плеера).

Нужно сделать тонкую, каноническую интеграцию без лишней связности.

## Решение

### 1. Плагин Noctalia

Отдельный каталог `orbitfm/` со следующей структурой (по официальной документации docs.noctalia.dev):

```
orbitfm/
├── plugin.toml
├── plugin.luau          # entry script
├── README.md            # страница плагина в каталоге
├── thumbnail.webp       # карточка 960×540
└── translations/
    └── en.json          # (и ru.json при необходимости)
```

**plugin.toml** (минимально необходимый набор):

```toml
id         = "modernbyte/orbitfm"
name       = "OrbitFM"
version    = "1.0.0"
plugin_api = 3
author     = "ModernByte"
icon       = "radio"
description = "3D Interactive Global Radio Player"
tags       = ["bar", "audio", "music"]
dependencies = ["orbitfm"]

[[widget]]
id    = "launcher"
entry = "plugin.luau"
```

### 2. Поведение виджета

В `plugin.luau` виджет при клике выполняет:

```luau
noctalia.runAsync("orbitfm toggle")
```

То есть плагин **не** содержит логики плеера — только отправляет IPC-команду уже запущенному (или запускаемому) процессу OrbitFM.

### 3. Принцип разделения

- Noctalia-плагин = тонкий адаптер
- Вся бизнес-логика и аудио живут в основном процессе `orbitfm`
- Плагин не держит состояние воспроизведения

### Уточнение (2026-10-22)

- Noctalia требует `id` в формате `<author>/<plugin>`; в манифесте используется
  `modernbyte/orbitfm`. Это не меняет app-id `org.modernbyte.OrbitFM` и имя проекта.
- Отправка команды выполняется через `noctalia.runAsync(...)` — актуальный Runtime API
  Noctalia. Функции `exec_async` в Plugin API нет.
- Манифест проверен `noctalia plugins lint`: 0 ошибок, 0 предупреждений.

Решение о тонком адаптере и IPC остаётся прежним.

### Уточнение (2026-10-22): каталог плагинов

- Каталог плагина переименован из `noctalia-plugin/` в `orbitfm/`: каталог
  Noctalia требует, чтобы имя подкаталога совпадало с частью id после `/`.
- В корне добавлен `catalog.toml`, индексирующий плагин для git-источника
  `ModernByte/orbitfm`; публикация в общий магазин выполняется отдельным PR в
  `noctalia-dev/community-plugins`.
- Теги приведены к официальному списку (`bar`, `audio`, `music`), внешняя
  команда `orbitfm` объявлена в `dependencies`.
- Добавлены `README.md` и `thumbnail.webp` 960×540, переводы переведены на
  вложенные ключи. Манифест проверен `noctalia plugins lint` и официальным
  community-валидатором.

### Альтернативы

| Вариант                          | Причина отклонения |
|----------------------------------|--------------------|
| Встраивание всего плеера в Luau  | Невозможно и не нужно (3D + libmpv) |
| D-Bus вместо CLI/IPC             | Усложняет, когда уже есть свой IPC |
| Глубокая интеграция через panel/layer | Избыточно для первой версии |

## Последствия

**Положительные:**
- Полное соответствие официальному Plugin API Noctalia
- Минимальная поверхность поддержки
- Плагин можно обновлять независимо от основного приложения

**Отрицательные / Риски:**
- Нужно следить за уровнем `plugin_api`
- При изменении IPC-команд придётся синхронизировать плагин

## Ссылки (официальные)

- https://docs.noctalia.dev/noctalia/plugins/development/
- https://docs.noctalia.dev/noctalia/plugins/development/manifest/
- https://docs.noctalia.dev/noctalia/plugins/development/plugin-api/
