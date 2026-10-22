---
name: orbitfm-patterns
description: Паттерны проектирования OrbitFM. Использовать при проектировании модулей, выборе структуры кода, рефакторинге и когда нужно решить как связать подсистемы без нарушения границ.
---

# OrbitFM Patterns

Ты применяешь только согласованные паттерны проекта.

## Обязательный справочник

Перед выбором структуры кода прочитай:

- `docs/patterns/00-overview.md` — полный каталог паттернов и привязка к модулям
- `docs/architecture/00-overview.md`
- Релевантные ADR и `docs/modules/`, `docs/interfaces/`

## Краткая карта

| Область | Паттерны |
|---------|----------|
| Система | Layered Architecture, Single Instance + Command, Ports & Adapters |
| App | Facade, лёгкий Mediator, State |
| Audio | RAII, command queue, Observer (events), State |
| Radio | Client/Gateway, Strategy (failover), Retry |
| Geo | Read-only repository, Index + refine (R-Tree → PIP) |
| Render | Callback/Strategy (PaintCallback), update/prepare/paint |
| IPC | Command, singleton process |
| Media | Adapter (playwire), Observer |
| Noctalia | Thin Adapter |
| Rust | Newtype, Builder (по нужде), Drop/RAII, thiserror, trait-as-port |

## Правила применения

1. **Минимум достаточного.** Не вводи паттерн, если хватает функции и трейта.
2. **Границы модулей важнее паттерна.** Паттерн не оправдывает god-file или циклические зависимости.
3. **Имена по роли.** `RadioClient`, `AudioEngine`, `IpcCommand` — не `Manager`/`Helper`/`Util` без смысла.
4. **Тяжёлые паттерны (Event Sourcing, DI-контейнер, UoW)** — только через ADR.
5. **Производительность.** Паттерн не отменяет правила `Box<[T]>`, компактных enum и запрета аллокаций в горячем пути (см. AGENTS.md §5).

## Алгоритм

1. Определи слой (Presentation / App / Domain / Infra).
2. Открой таблицу паттернов для модуля в `docs/patterns/00-overview.md`.
3. Реализуй через трейт из `docs/interfaces/`, если контракт уже есть.
4. Если паттерна нет в справочнике, а задача повторяется — предложи ADR + обновление `docs/patterns/`.

## Связанные скиллы

- `orbitfm-architect` — границы и ADR
- `orbitfm-rust` — идиомы языка
- `orbitfm-docs` — синхронизация `docs/patterns/`
- профильные (`orbitfm-audio`, `orbitfm-gui`, …) — детали подсистемы
