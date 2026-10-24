# OrbitFM

**3D Interactive Global Radio Player**  
Под эгидой [ModernByte](https://github.com/ModernByte)

OrbitFM — кроссплатформенный десктоп-плеер интернет-радио с интерактивным 3D-глобусом.  
Приоритетная платформа — Linux (Niri, Hyprland, Noctalia v5). Поддержка Windows и macOS
запланирована.

## Основные возможности

- Интерактивный 3D-глобус с выбором страны (raycasting + Point-in-Polygon)
- Режим «Страна»: hover-контур, поиск страны в HUD, локальные фильтры и сортировка
  станций
- Маркеры станций на глобусе с подписями, группировкой и запуском по клику
- Избранное и история станций с локальным хранением и запуском без сети
- Темы интерфейса (системная / тёмная / светлая) и шесть акцентов с сохранением
  настроек
- Воспроизведение станций из Radio Browser
- Скрытие окна без остановки процесса: эфир, IPC, MPRIS и трей продолжают работать
- Режимы окна Focus/Overlay/Listening с капсульными раскладками и IPC `mode …`;
  состояние — в Application Core (ADR-0021)
- Иконка в системном трее (SNI): меню, колесо громкости и команды при скрытом окне
- Интеграция с Noctalia v5 (виджет панели)
- MPRIS / системные медиа-клавиши
- Cava-визуализатор на агрегированных метриках эфира и пульс глобуса в такт
  (ADR-0022, ADR-0023)
- Аудио-реактивная планета: spring-squash, шесть стилей движения, onset/beat,
  перевороты камеры и Reduce motion (ADR-0028)
- Орбитальное аудиокольцо в Focus: корона из 120 столбцов, аврора, комета и
  искры на onset (ADR-0029)
- Трёхполосный анализ эфира Bass/Mid/High без PCM-tap (ADR-0027); при скрытом
  окне анализ снимается
- AppImage для Linux x86_64 (libmpv и FFmpeg внутри)
- Flatpak для Linux x86_64 (libmpv/libass/libplacebo из исходников)
- MSI для Windows x64 (libmpv из MSYS2, WiX) и DMG для macOS (libmpv из Homebrew)
- Single-instance архитектура (Unix-сокет / Windows named pipe)

## Статус

**Пре-релиз `v0.9.0-rc.2`:** Этап 9 завершён — аудио-реактивная планета. Эфир
раскладывается на Bass/Mid/High без PCM-tap
([ADR-0027](docs/adr/0027-трёхполосный-анализ-эфира.md)); глобус дышит и качается
в такт spring-squash'ем, шесть стилей движения и Auto с onset/beat-детектором
([ADR-0028](docs/adr/0028-аудио-реактивная-деформация-глобуса.md)); за планетой в
Focus живёт орбитальное аудиокольцо с короной, авророй, кометой и искрами
([ADR-0029](docs/adr/0029-орбитальное-аудиокольцо.md)). Пока окно закрыто,
аудио-анализ снимается, поэтому скрытый эфир не тратит CPU (19–23 % → ≈2 %).
Приёмка Linux на niri 26.04: live FLAC/MP3, hide/show, все режимы, моно-поток и
реконнект; правила окон Focus/Overlay/Listening проверены для плагина Noctalia.

**Этап 8 (0.8.0-rc.1):** Application Core, режимы окна и системный трей.
Скрытие окна закрывает root-вьюпорт: UI-цикл останавливается, но
процесс, эфир, IPC, MPRIS и трей живут, а состоянием владеет `src/session/`
([ADR-0018](docs/adr/0018-скрытие-окна-закрытием-вьюпорта.md)). Режимы окна
`Focus`/`Overlay`/`Listening`
([ADR-0021](docs/adr/0021-режимы-окна-windowmode.md)): Overlay — компактная
капсула с HUD-строкой, мини-глобусом и избранным, Listening — одна строка
now-playing с пульсом; переключение — IPC `mode …` или меню шестерёнки, возврат в
Focus — кнопка expand. Трей по SNI
([ADR-0020](docs/adr/0020-иконка-в-системном-трее-sni.md)): меню, колесо громкости
и все команды при скрытом окне. Доступность: focus-ring, роли и подписи
контролов, клавиатурный шаг громкости и навигация ↑/↓ по списку станций
(Space/Enter запускают станцию).

Раскладка Focus (фазы 1–6): интерфейс переведён на
«Orbital Desk» (HUD, доминирующий глобус, правый drawer, transport). Режим «Страна»:
наведение и выбор страны подсвечивают контур, HUD ищет страну по названию или
ISO-коду, панель показывает до 100 станций с локальными фильтрами по
имени/жанру/штату и сортировкой; выбор страны мягко доворачивает камеру. Режим
«Жанр» ищет станции по тегу (до 200) с маркерами; все маркеры одного размера и
цвета, а качество (битрейт, кодек) и рейтинг (голоса) видны в подписи при
наведении. «Рулетка» запускает случайную станцию с перелётом камеры и тостом,
MPRIS показывает страну/штат. Живой глобус: Fresnel-обод атмосферы пульсирует от
сглаженного уровня/flux при воспроизведении, выбранная страна получает
heatmap-заливку по числу загруженных станций.

**Фаза 6 и доводка:** плавающая раскладка full-bleed (геометрия зон в
`DeskLayout`), кастомные контролы (pill-переключатель, жанровые чипы, поиск с
подсказками, LIVE-бейдж, круглая play/pause), асинхронный favicon-кэш,
цветной emoji-шрифт для флагов, prev/next по видимому списку, английские строки,
синтетический cava-визуализатор на 16/24/32 столбца вместо FFT-strip, единые
маркеры с деталями при наведении, country callout с leader line, пульс глобуса в
такт эфиру и медленное авто-вращение после простоя, осветлённый глобус с key light
и Fresnel-кромкой.

**Этап 7 завершён:** плагин Noctalia оформлен как источник каталога — добавлены
`catalog.toml`, README с требованиями и карточка 960×540; манифест прошёл
официальный community-валидатор. Также добавлены темы и кастомизация — блок «Оформление» с выбором
системной, тёмной или светлой темы и одного из шести акцентов; настройки
сохраняются в `$XDG_CONFIG_HOME/orbitfm/settings.json`, акцент окрашивает выделение,
гиперссылки и маркеры. Также добавлены избранное и история станций — вкладки
«Результаты / Избранное / История», звезда у каждой строки, сохранение в
`$XDG_DATA_HOME/orbitfm/library.json` и запуск сохранённых станций без сети. На
глобусе есть маркеры станций с координатами: наведение показывает подпись, клик
запускает станцию. Ранее добавлены экспериментальный FFT-визуализатор и упаковка
Linux — AppImage с метаданными рабочего стола и встроенными libmpv/FFmpeg, а также
Flatpak с libmpv/libass/libplacebo из исходников. Также
реализованы интеграция Noctalia v5 и правила окон для Niri и Hyprland, интерактивный
3D-глобус с выбором страны, каталог Radio Browser (DNS-discovery, поиск, failover,
кэш, click-tracking), воспроизведение станции по UUID и MPRIS с метаданными станции.
CI собирает и тестирует проект на Linux и macOS, проверяет компиляцию под Windows,
а также публикует неподписанные MSI и DMG артефактами. Готовые пакеты пре-релиза —
[страница релизов](https://github.com/ModernByte/orbitfm/releases). Кроссплатформенные
медиа-интеграции — привязка SMTC к HWND и Now Playing на macOS — пока не
реализованы.
Целевой UX и план —
[`docs/ui/design-vision.md`](docs/ui/design-vision.md) и
[`docs/ui/implementation-plan.md`](docs/ui/implementation-plan.md).

Границы стран (Natural Earth 110m) и текстура Земли встроены в бинарник, поэтому
глобус работает offline.

Интеграции:

- `orbitfm/` — плагин Noctalia (`modernbyte/orbitfm`) с манифестом, виджетом,
  README и карточкой каталога; требуется `orbitfm` в `PATH`;
- `catalog.toml` — каталог плагинов для git-источника. Подключение:

  ```bash
  noctalia msg plugins source add orbitfm git https://github.com/ModernByte/orbitfm
  noctalia msg plugins enable modernbyte/orbitfm
  ```

- `wm-configs/orbitfm-niri.kdl` и `wm-configs/orbitfm-hyprland.conf` — готовые
  правила плавающих окон по заголовкам режимов: Focus 920×660, Overlay 420×620,
  Listening 420×152. Они нужны в том числе для окон, которые открывает плагин
  Noctalia; без них niri тайлит капсулы. Niri-правила дополнительно якорят окно
  к правому верхнему углу, чтобы оно появлялось под панелью Noctalia у виджета,
  а не в центре экрана. Для niri файл можно подключить отдельным include
  (`include "./cfg/orbitfm.kdl"`), не меняя остальной конфиг;
- системный трей — `src/tray/` (SNI / StatusNotifierItem на `ksni`, фича `tray` включена
  по умолчанию): работает с любым хостом SNI, проверено на Noctalia; на macOS и Windows
  модуль — no-op.

Сборка AppImage (Linux x86_64):

```bash
./packaging/linux/build-appimage.sh
```

Скрипт собирает release-бинарник, бандлит libmpv/FFmpeg через `linuxdeploy` и
складывает `target/appimage/OrbitFM-<version>-x86_64.AppImage`. Инструменты
`linuxdeploy` и `appimagetool` скачиваются при первом запуске, если не заданы через
`LINUXDEPLOY` / `APPIMAGETOOL`.

Нативные установщики собираются в CI (артефакты `OrbitFM-windows-x64-msi` и
`OrbitFM-macos-dmg`):

- Windows: `packaging/windows/build-msi.ps1` — MSI через WiX 5, libmpv из MSYS2
  UCRT64; per-user установка в `%LOCALAPPDATA%\Programs\OrbitFM`;
- macOS: `packaging/macos/build-dmg.sh` — `.app` с зависимостями из Homebrew,
  ad-hoc подпись и UDZO DMG.

Установщики не подписаны сертификатами: MSI без Authenticode, DMG без Developer ID
и нотаризации.

Требования для сборки:

- Rust 1.95+
- Linux с Wayland/X11
- системная библиотека libmpv с заголовками
  (Debian/Ubuntu: `libmpv-dev`, Arch: `mpv`, Fedora: `mpv-libs-devel`)

Подробный план — [ROADMAP](docs/ROADMAP.md), изменения — [CHANGELOG](docs/CHANGELOG.md).

## Документация

| Документ | Описание |
|----------|----------|
| [SRS](orbitfm_srs.md) | Технические требования и инженерная спецификация |
| [Архитектура](docs/architecture/00-overview.md) | Высокоуровневый обзор |
| [UX «Orbital Desk»](docs/ui/design-vision.md) | Целевой интерфейс и референс-макет |
| [План UX-редизайна](docs/ui/implementation-plan.md) | Фазы, контракты и тесты Этапа 8 |
| [ADR](docs/adr/) | Architecture Decision Records |
| [ROADMAP](docs/ROADMAP.md) | План развития |
| [CONTRIBUTING](docs/CONTRIBUTING.md) | Правила разработки и стиль кода |
| [AGENTS](docs/AGENTS.md) | Правила работы AI-агентов |

## Стек (кратко)

- **Язык:** Rust (Edition 2024)
- **GUI:** eframe + egui + egui-wgpu
- **Аудио:** libmpv (безопасная обёртка `libmpv2`)
- **Гео:** geo + geojson + rstar
- **Сеть:** reqwest + Radio Browser API
- **Интеграции:** Noctalia (Luau), MPRIS (playwire)

Подробности — в ADR и SRS.

## Лицензия

MIT

---

© ModernByte
