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
- [x] ADR-0010 (FFT-визуализатор)
- [x] ADR-0011 (Упаковка Linux)
- [x] ADR-0012 (Маркеры станций)
- [x] ADR-0013 (Избранное и история)
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

## Этап 2: Аудио-ядро — завершён

- [x] Обёртка над libmpv (`libmpv2`) в выделенном потоке
- [x] Play / Pause / Stop / Volume / Reconnect
- [x] Базовый MPRIS (Linux) через playwire
- [x] Живой тест воспроизведения (по запросу, вне CI)

## Этап 3: Radio Browser — завершён

- [x] DNS discovery (`all.api.radio-browser.info`, ленивый + фоновый прогрев)
- [x] Поиск станций и поиск в GUI
- [x] Кэш и fallback зеркал
- [x] User-Agent и click-tracking
- [x] Подключение `play <uuid>` к аудио-ядру и метаданные станции в MPRIS

## Этап 4: 3D-глобус (минимальный) — завершён

- [x] Сфера + текстура
- [x] Камера (вращение + zoom)
- [x] Raycasting + Point-in-Polygon
- [x] Выбор страны

## Этап 5: Интеграции — завершён

- [x] Noctalia plugin (widget + IPC)
- [x] Window rules для Niri / Hyprland
- [ ] Системный трей (опционально; не реализован)

## Текущий этап: Этап 6 — Полировка и кроссплатформа

- [ ] Windows (SMTC) — заблокировано: нужны Windows-раннер, сборка libmpv,
  named pipe IPC и привязка SMTC к HWND
- [ ] macOS (Now Playing) — заблокировано: нужны macOS-раннер и сборка libmpv
- [x] FFT-визуализатор (экспериментальный)
- [x] Упаковка Linux: AppImage (`packaging/linux/build-appimage.sh`), desktop entry,
  AppStream-метаданные и иконка
- [ ] Flatpak — заблокировано: нужны vendored cargo-исходники и модуль libmpv/FFmpeg
- [ ] MSI — заблокировано: нужна Windows-сборка (см. пункт Windows)
- [ ] DMG — заблокировано: нужна macOS-сборка (см. пункт macOS)

## Этап 7: Маркеры и персонализация — в работе

- [x] Маркеры станций на глобусе
- [x] Избранное и история
- [ ] Темы и кастомизация
- [ ] Плагин-каталог Noctalia

---

Приоритеты могут корректироваться по результатам прототипирования.
