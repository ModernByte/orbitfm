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
- [x] ADR-0014 (Темы и кастомизация)
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

- [x] Windows: named pipe IPC (`src/ipc/pipe.rs`) и CI-проверка на `windows-latest`
- [ ] Windows (SMTC) — осталось: привязка playwire к HWND
- [x] macOS: CI-сборка, clippy и тесты на `macos-latest` с libmpv из Homebrew
- [ ] macOS (Now Playing) — осталось: рантайм-проверка Now Playing
- [x] FFT-визуализатор (экспериментальный)
- [x] Упаковка Linux: AppImage (`packaging/linux/build-appimage.sh`), desktop entry,
  AppStream-метаданные и иконка
- [x] Flatpak: `packaging/linux/org.modernbyte.OrbitFM.yml`, runtime 25.08,
  libmpv/libass/libplacebo из исходников, vendored cargo (`build-flatpak.sh`)
- [x] MSI: джоба `msi` — libmpv из MSYS2 UCRT64, замыкание DLL, WiX 7,
  per-user установка и smoke-тест `msiexec`; артефакт без подписи
- [x] DMG: джоба `dmg` — `.app` с `dylibbundler`, ad-hoc подпись и UDZO-образ;
  артефакт без нотаризации

## Этап 7: Маркеры и персонализация — завершён

- [x] Маркеры станций на глобусе
- [x] Избранное и история
- [x] Темы и кастомизация
- [x] Плагин-каталог Noctalia: `catalog.toml`, README и карточка для git-источника

Публикация в общий магазин `noctalia-dev/community-plugins` не выполняется:
плагин распространяется как git-источник `ModernByte/orbitfm`.

---

Приоритеты могут корректироваться по результатам прототипирования.
