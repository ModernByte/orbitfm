# Roadmap OrbitFM

**Организация:** ModernByte  
**Последнее обновление:** 2026-10-24  

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
- [x] ADR-0016 (асинхронный favicon-загрузчик)
- [x] ADR-0017 (цветной emoji-шрифт для флагов)
- [x] ADR-0018 (скрытие окна закрытием вьюпорта, редакция 2)
- [x] ADR-0019 (синтетический аудиовизуализатор)
- [x] ADR-0020 (иконка в системном трее, SNI)
- [x] ADR-0021 (режимы окна `WindowMode` и IPC-глагол `mode`)
- [x] ADR-0022 (визуализатор в стиле cava)
- [x] ADR-0023 (пульс глобуса и авто-вращение после простоя)
- [x] ADR-0024 (выделенное окно Windows для SMTC)
- [x] ADR-0025 (безопасная загрузка и декодирование favicon)
- [x] ADR-0029 (орбитальное аудиокольцо за глобусом)
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
- [x] Системный трей: SNI-иконка (`src/tray/`, `ksni`), меню, колесо громкости и
  команды при закрытом окне (ADR-0020); проверка на живом Noctalia

## Текущий этап: Этап 6 — Полировка и кроссплатформа

- [x] Windows: named pipe IPC (`src/ipc/pipe.rs`) и CI-проверка на `windows-latest`
- [x] Windows (SMTC) — ADR-0024 принят: скрытое top-level окно на выделенном потоке
  отдаёт HWND для playwire, контролы переживают скрытие окна; осталось
  рантайм-проверить play/pause/stop, громкость, hide/show и отсутствие окна в Alt-Tab
- [x] macOS: CI-сборка, clippy и тесты на `macos-latest` с libmpv из Homebrew
- [ ] macOS (Now Playing) — метаданные playwire публикуются; осталось
  рантайм-проверить, а доставку remote-команд при скрытом окне закрывает
  ADR-0026 (предложено)
- [ ] macOS (трей) — ADR-0026 (предложено): статус-элемент в строке меню
  (`tray-icon`) и скрытое окно eframe вместо `park`; нужна live-проверка на Mac
- [x] Безопасная обработка favicon (ADR-0025): allowlist PNG/JPEG/WebP, лимиты
  размера и разрешения, MIME/`http(s)`-проверки, перехват паники декодера
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

## Этап 8: UX «Orbital Desk» — завершён (0.8.0-rc.1 выкатан)

Целевой UX и макет — [`docs/ui/design-vision.md`](ui/design-vision.md); фазы,
контракты и тесты — [`docs/ui/implementation-plan.md`](ui/implementation-plan.md);
решение — [ADR-0015](adr/0015-ux-редизайн-orbital-desk.md). Фазы 1–7
реализованы; фаза 6 вошла в 0.7.0-rc.1, фаза 7 вышла пре-релизом 0.8.0-rc.1.

- [x] Фаза 1: UI-каркас — HUD, правый drawer, transport, `src/ui/`, глобус доминирует
  (Roulette перенесён в Фазу 4)
- [x] Фаза 2: режим «Страна» — hover/контур выбранной страны, поиск страны в HUD,
  локальные фильтры и сортировка, карточки станций, лимит 100
- [x] Фаза 3: режим «Жанр» — `SearchMode`, tag-поиск 200/200, маркеры,
  поколение поиска и локальный фильтр в контексте
- [x] Фаза 4: Roulette и MPRIS — `order=random`, `fly_to` 900 мс, toast с именем
  и страной, artist из страны/штата
- [x] Фаза 5: живой глобус — Fresnel pulse от FFT (сглаженные level/flux),
  heatmap выбранной и наведённой страны по числу станций
- [x] Фаза 6: полировка UI по эскизу — плавающая раскладка (full-bleed глобус,
  floating-зоны `DeskLayout`), кастомные контролы (`widgets.rs`), favicon-кэш
  (ADR-0016), цветной emoji-шрифт для флагов (ADR-0017), тултип страны,
  prev/next по видимому списку, английские строки
- [x] Фаза 7: WindowMode Overlay/Listening и доступность — `LayoutDensity` и
  капсулы `src/ui/capsule.rs` (Overlay: HUD-строка + мини-глобус + избранное;
  Listening: now-playing с пульсом), `mode` в IPC, focus-ring и клавиатурная
  навигация по списку; проверено на Wayland/niri (ADR-0021)
- [x] Синхронизировать `docs/interfaces/` и модули в фазах изменения контрактов

## Этап 9: Аудио-реактивная планета — в работе

Концепция, контракты и фазы — [`docs/ui/audio-reactive-globe.md`](ui/audio-reactive-globe.md);
решения — [ADR-0027](adr/0027-трёхполосный-анализ-эфира.md) (полосы эфира)
и [ADR-0028](adr/0028-аудио-реактивная-деформация-глобуса.md) (деформация).
Оба ADR приняты; фазы 0–2 реализованы, дальше — камера и приёмка. Старый
Fresnel-пульс снят с глобуса до начала работ, чтобы не мешал пересмотру визуала.

- [x] Вне фаз: орбитальное аудиокольцо за глобусом (ADR-0029) — haze, корона
  из 120 столбцов, аврора, комета, искры на onset; Focus-only, без новых
  зависимостей и без влияния на hit-testing
- [x] Фаза 0: полосы эфира — `@level` + `@spectrum` (`acrossover`/`amerge`/`pan`),
  Bass/Mid/High без PCM-tap, mono/stereo и fallback (ADR-0027)
- [x] Фаза 1: ядро движения — `MotionMixer`, spring-squash с сохранением объёма,
  idle-дыхание, `GlobeDeform` для шейдера/маркеров/стран/hit-test, настройки
  Motion и Reduce motion, анализ по требованию, кадры 30/15 fps
- [x] Фаза 2: onset, бит и шесть стилей — детектор удара (20 Гц, медианная
  базовая линия, рефрактерность, период 60–180 BPM), Auto с удержанием 3 с и
  кроссфейдом, uniform `motion2`, Fresnel-вспышка, выбор стиля в HUD
- [ ] Фаза 3: камера и режимы — музыкальный sway/roll, перевороты с кулдауном,
  Overlay ≤ Low, Listening Off
- [ ] Фаза 4: полировка и приёмка — CPU/GPU, Wayland/niri, Windows/macOS,
  документация; сверка ADR-0027/0028 с реализацией

---

Приоритеты могут корректироваться по результатам прототипирования.
