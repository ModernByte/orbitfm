---
name: orbitfm-core
description: Базовые правила проекта OrbitFM (ModernByte). Использовать при любой задаче по OrbitFM — кодирование, документация, архитектура, рефакторинг. Задаёт именование, структуру репозитория, обязательные источники истины и запреты.
---

# OrbitFM Core

Ты работаешь над проектом **OrbitFM** под эгидой организации **ModernByte**.

## Обязательные источники истины (читать перед изменениями)

1. `orbitfm_srs.md` — актуальная инженерная спецификация
2. `docs/architecture/00-overview.md` — обзор архитектуры
3. `docs/adr/` — все Architecture Decision Records
4. `docs/modules/` — модульные спецификации
5. `docs/interfaces/` — формальные контракты
6. `docs/AGENTS.md` — правила для AI-агентов
7. `docs/CONTRIBUTING.md` — стиль кода и процесс

Если задача затрагивает публичный контракт — сначала проверь соответствующий ADR и interface.

## Именование (ModernByte)

| Сущность | Значение |
|----------|----------|
| Организация | ModernByte |
| Репозиторий | ModernByte/orbitfm |
| Crate / binary | orbitfm |
| Plugin ID (Noctalia) | modernbyte/orbitfm |
| app-id / Desktop Entry | org.modernbyte.OrbitFM |
| User-Agent | OrbitFM/1.0 (ModernByte; Linux; Rust) |
| Author в plugin.toml | ModernByte |

## Структура репозитория

```
orbitfm/
├── assets/
├── orbitfm/                 # плагин Noctalia (имя = суффикс id)
├── catalog.toml             # каталог плагинов для git-источника
├── packaging/
│   └── linux/
├── wm-configs/
├── src/
│   ├── main.rs
│   ├── app.rs
│   ├── audio/
│   ├── geo/
│   ├── render/
│   ├── services/
│   ├── media/
│   ├── library/
│   ├── settings/
│   └── ipc/
├── docs/
│   ├── architecture/
│   ├── adr/
│   ├── modules/
│   └── interfaces/
├── .agents/skills/
├── Cargo.toml
└── README.md
```

Не создавай файлы и каталоги вне этой структуры без явной необходимости.

## Жёсткие правила

1. **Ничего не выдумывать.** Любое техническое утверждение должно опираться на официальную документацию или уже принятый ADR.
2. **Linux First.** Все решения сначала проектируются и проверяются под Linux (Wayland).
3. **Single-instance.** Приложение всегда один процесс. Закрытие окна = Hide, а не завершение процесса.
4. **Документировать решения.** Значимые архитектурные изменения — только через новый или обновлённый ADR.
5. **Язык документации** — русский. Комментарии в коде — русский или английский (доменная логика предпочтительно на русском).
6. **Не трогать лишнее.** Не добавлять зависимости, файлы, картинки и «заготовки на будущее» без запроса.

## Порядок работы над задачей

1. Определи, какие модули/ADR затрагиваются.
2. Прочитай соответствующие документы.
3. Если меняется публичный контракт — обнови interface + ADR (если нужно) + CHANGELOG.
4. Пиши код строго в рамках существующих модульных границ.
5. После изменений обнови ROADMAP/CHANGELOG при необходимости.

## Запрещено

- Хардкодить один сервер Radio Browser
- Игнорировать User-Agent
- Создавать второй процесс для GUI
- Менять принятые ADR без явного указания
- Добавлять `searched_images` или любые артефакты поиска
- Использовать устаревшие crate’ы без обоснования в ADR

## Связанные скиллы

При узких задачах дополнительно активируй профильный скилл:
- `orbitfm-architect` — архитектура и ADR
- `orbitfm-rust` — стиль и практики Rust
- `orbitfm-gui` — egui / wgpu / глобус
- `orbitfm-audio` — libmpv
- `orbitfm-radio` — Radio Browser
- `orbitfm-geo` — geo / rstar
- `orbitfm-ipc` — single-instance и IPC
- `orbitfm-integration` — Noctalia, MPRIS, WM
- `orbitfm-docs` — синхронизация документации
- `orbitfm-patterns` — паттерны (`docs/patterns/00-overview.md`)
