# Roadmap OrbitFM

**Организация:** ModernByte  
**Последнее обновление:** 2026-10-22  

## Завершено: Проектирование и документация

- [x] SRS v3.0
- [x] Обзор архитектуры
- [x] ADR-0001 (фиксация решений)
- [x] ADR-0002 (GUI-стек)
- [x] ADR-0003 (аудио-движок)
- [x] ADR-0004 (Radio Browser Client)
- [x] ADR-0005 (Single-instance + IPC)
- [x] ADR-0006 (Медиа-контроли)
- [x] ADR-0007 (Интеграция с Noctalia)
- [x] ADR-0008 (Геопространственный поиск)
- [x] ADR-0009 (Каталог паттернов)
- [x] VERSIONMAP
- [x] CHANGELOG (инициализация)
- [x] README
- [x] CONTRIBUTING
- [x] AGENTS.md
- [x] Модульные спецификации (`docs/modules/`)
- [x] Интерфейсы (`docs/interfaces/`)
- [x] Паттерны (`docs/patterns/`) + скилл orbitfm-patterns
- [x] Шаблоны GitHub (PR, issue, commit) и merge/status

## Этап 1: Каркас приложения (MVP-0) — завершён

- [x] Инициализация crate `orbitfm`
- [x] Single-instance + Unix-socket IPC
- [x] CLI (`orbitfm toggle`, `orbitfm status` и т.д.)
- [x] Базовый eframe/egui skeleton
- [x] Прозрачное окно (overlay-режим)

## Текущий этап: Этап 2 — Аудио-ядро

- [ ] Обёртка над libmpv
- [ ] Play / Stop / Volume / Reconnect
- [ ] Базовый MPRIS (Linux)

## Этап 3: Radio Browser

- [ ] DNS discovery
- [ ] Поиск станций
- [ ] Кэш и fallback зеркал
- [ ] User-Agent и click-tracking

## Этап 4: 3D-глобус (минимальный)

- [ ] Сфера + текстура
- [ ] Камера (вращение + zoom)
- [ ] Raycasting + Point-in-Polygon
- [ ] Выбор страны

## Этап 5: Интеграции

- [ ] Noctalia plugin (widget + IPC)
- [ ] Window rules для Niri / Hyprland
- [ ] Системный трей (опционально)

## Этап 6: Полировка и кроссплатформа

- [ ] Windows (SMTC)
- [ ] macOS (Now Playing)
- [ ] FFT-визуализатор (экспериментальный)
- [ ] Упаковка (AppImage / Flatpak / MSI / DMG)

## Дальше

- Маркеры станций на глобусе
- Избранное и история
- Темы и кастомизация
- Плагин-каталог Noctalia

---

Приоритеты могут корректироваться по результатам прототипирования.
