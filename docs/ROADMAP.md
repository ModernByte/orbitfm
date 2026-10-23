# Roadmap OrbitFM

**Организация:** ModernByte  
**Последнее обновление:** 2026-10-23  

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
- [x] ADR-0015 (UX-редизайн «Orbital Desk»)
- [x] UX-спецификация `docs/ui/design-vision.md` и план реализации
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
- [x] MSI: джоба `msi` — libmpv из MSYS2 UCRT64, замыкание DLL, WiX 5,
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

## Этап 8: UX «Orbital Desk» — в работе

Целевой UX и макет — [`docs/ui/design-vision.md`](ui/design-vision.md); фазы,
контракты и тесты — [`docs/ui/implementation-plan.md`](ui/implementation-plan.md);
решение — [ADR-0015](adr/0015-ux-редизайн-orbital-desk.md). MVP — фазы 1–4,
фазы 5–6 могут выйти отдельным MINOR-релизом.

- [x] Фаза 1: UI-каркас — HUD, правый drawer, transport, `src/ui/`, глобус доминирует
  (Roulette перенесён в Фазу 4)
- [x] Фаза 2: режим «Страна» — hover/контур выбранной страны, поиск страны в HUD,
  локальные фильтры и сортировка, карточки станций, лимит 100
- [x] Фаза 3: режим «Жанр» — `SearchMode`, tag-поиск 200/200, маркеры с весом,
  поколение поиска и локальный фильтр в контексте
- [x] Фаза 4: Roulette и MPRIS — `order=random`, `fly_to` 900 мс, toast с именем
  и страной, artist из страны/штата
- [ ] Фаза 5: живой глобус — Fresnel pulse от FFT, heatmap выбранной страны
- [ ] Фаза 6: WindowMode Overlay/Listening, favicon-карточки, доступность
- [ ] Синхронизировать `docs/interfaces/` и модули в фазах изменения контрактов

---

Приоритеты могут корректироваться по результатам прототипирования.
