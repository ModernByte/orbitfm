# Паттерны проектирования в OrbitFM

**Организация:** ModernByte  
**Дата:** 2026-10-22  
**Статус:** Обязательный справочник для агентов  

Этот документ фиксирует, **какие** паттерны применяются в OrbitFM, **где** и **зачем**.  
Цель — единообразие, предсказуемость и отсутствие «изобретения велосипедов» в ущерб ясности.

Связанные документы:

- [Обзор архитектуры](../architecture/00-overview.md)
- [ADR](../adr/)
- [Модули](../modules/)
- [Интерфейсы](../interfaces/)
- [AGENTS.md](../AGENTS.md) — раздел делегирования и модульности
- Скилл: `.agents/skills/orbitfm-patterns/SKILL.md`

---

## 1. Принципы выбора паттерна

1. Паттерн должен **упрощать** границы модулей или тестируемость — иначе не нужен.
2. Не вводить паттерн «для резюме». Если достаточно функции и трейта — достаточно.
3. Имена в коде отражают роль (например, `RadioClient`, а не `RadioRepositoryImplV2`).
4. Паттерн не освобождает от правил модульности и производительности из [AGENTS.md](../AGENTS.md).

---

## 2. Архитектурные паттерны (уровень системы)

| Паттерн | Где | Зачем | Ссылки |
|---------|-----|-------|--------|
| **Layered Architecture** | Весь проект | Однонаправленные зависимости Presentation → App → Domain → Infra | [architecture/00-overview.md](../architecture/00-overview.md), ADR-0001 |
| **Single Instance + Command** | `ipc`, `app` | Один процесс; CLI/Noctalia шлют команды | ADR-0005, [modules/05-ipc.md](../modules/05-ipc.md) |
| **Ports & Adapters (гексагональный акцент)** | Domain vs Infra | Domain зависит от трейтов; реализации (libmpv, reqwest) — снаружи | [interfaces/](../interfaces/) |

---

## 3. Паттерны по модулям

### 3.1. Application Core

| Паттерн | Применение |
|---------|------------|
| **Facade** | `App` / `AppState` — единая точка оркестрации audio, radio, geo, media, ipc |
| **Mediator (лёгкий)** | Маршрутизация `IpcCommand` / `MediaCommand` в нужные сервисы без прямых связей UI↔domain |
| **State** | `window_visible`, `PlaybackState`, текущая станция — явное состояние, а не размазанные флаги |

См. [modules/07-app-core.md](../modules/07-app-core.md), [interfaces/07-app.md](../interfaces/07-app.md).

### 3.2. Audio Engine

| Паттерн | Применение |
|---------|------------|
| **RAII** | Обёртка над `mpv_handle` с `Drop` |
| **Command queue** | Поток/очередь команд к libmpv (thread-safety) |
| **Observer** | `broadcast::Receiver<AudioEvent>` для подписчиков |
| **State** | `PlaybackState` |

См. ADR-0003, [modules/01-audio-engine.md](../modules/01-audio-engine.md), [interfaces/01-audio.md](../interfaces/01-audio.md).

### 3.3. Radio Browser Client

| Паттерн | Применение |
|---------|------------|
| **Client / Gateway** | Единая точка к внешнему API |
| **Strategy (failover)** | Выбор следующего зеркала при ошибке |
| **Retry with backoff** | Политика повторов (явно, не «магический» цикл) |

См. ADR-0004, [modules/02-radio-browser.md](../modules/02-radio-browser.md).

### 3.4. Geo

| Паттерн | Применение |
|---------|------------|
| **Repository (read-only)** | Загрузка и запросы стран offline |
| **Index + refine** | R-Tree (грубый отбор) → Point-in-Polygon (точная проверка) |

См. ADR-0008, [modules/03-geo.md](../modules/03-geo.md).

### 3.5. Render / Globe

| Паттерн | Применение |
|---------|------------|
| **Callback / Strategy** | `PaintCallback` — внедрение кастомного wgpu-пасса в egui |
| **Separation of update/paint** | `update` (логика камеры) vs `prepare`/`paint` (GPU) |

См. ADR-0002, [modules/04-render-globe.md](../modules/04-render-globe.md).

### 3.6. IPC

| Паттерн | Применение |
|---------|------------|
| **Command** | Текстовые/enum-команды (`toggle`, `play`, …) |
| **Singleton process** | `try_bind` / `AlreadyRunning` |

См. ADR-0005, [interfaces/05-ipc.md](../interfaces/05-ipc.md).

### 3.7. Media Controls

| Паттерн | Применение |
|---------|------------|
| **Adapter** | playwire/souvlaki адаптируют ОС-API к единому `MediaControls` |
| **Observer** | События от системы → `MediaCommand` |

См. ADR-0006, [modules/06-media-controls.md](../modules/06-media-controls.md).

### 3.8. Noctalia / WM

| Паттерн | Применение |
|---------|------------|
| **Thin Adapter** | Плагин только шлёт IPC, без бизнес-логики |
| **Configuration as data** | window rules в `wm-configs/` |

См. ADR-0007, [modules/08-noctalia-wm.md](../modules/08-noctalia-wm.md).

---

## 4. Идиомы Rust (обязательные «мини-паттерны»)

| Идиома | Зачем |
|--------|-------|
| **Newtype** | Типобезопасные ID (`StationUuid(String)`), не смешивать сырые строки |
| **Builder** | Сложная инициализация (NativeOptions, SearchParams) — по необходимости |
| **Type state** | Запрет вызовов в неверном состоянии на этапе компиляции (где окупается) |
| **Drop / RAII** | Все FFI-ресурсы |
| **Error enum + thiserror** | Доменные ошибки по модулям |
| **Trait as port** | `AudioEngine`, `RadioClient`, `GeoService` — тестируемые границы |

Детали стиля: скилл `orbitfm-rust`, [CONTRIBUTING.md](../CONTRIBUTING.md).

---

## 5. Паттерны, которые **не** используем без ADR

- Enterprise Repository + UnitOfWork на каждый чих
- Полноценный Event Sourcing
- DI-контейнеры (достаточно явной сборки в `main` / App)
- Observer на всё подряд (только там, где есть реальные подписчики)
- Singleton как глобальная мутабельная куча

Если кажется, что нужен тяжёлый паттерн — сначала ADR (`orbitfm-architect`).

---

## 6. Как агенту применять

1. Перед написанием модуля — открыть этот файл + module spec + interface.
2. Выбрать **минимальный** набор паттернов из таблиц выше.
3. Не смешивать паттерны разных слоёв в одном файле (нарушение модульности).
4. При новом повторяющемся решении — предложить ADR и дополнить этот документ (`orbitfm-docs`).

Скилл: **orbitfm-patterns**.
