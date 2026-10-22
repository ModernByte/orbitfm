# OrbitFM

**3D Interactive Global Radio Player**  
Под эгидой [ModernByte](https://github.com/ModernByte)

OrbitFM — кроссплатформенный десктоп-плеер интернет-радио с интерактивным 3D-глобусом.  
Приоритетная платформа — Linux (Niri, Hyprland, Noctalia v5). Поддержка Windows и macOS
запланирована.

## Основные возможности

- Интерактивный 3D-глобус с выбором страны (raycasting + Point-in-Polygon)
- Маркеры станций на глобусе с подписями, группировкой и запуском по клику
- Избранное и история станций с локальным хранением и запуском без сети
- Темы интерфейса (системная / тёмная / светлая) и шесть акцентов с сохранением
  настроек
- Воспроизведение станций из Radio Browser
- Режим standalone-окна и overlay (через IPC)
- Интеграция с Noctalia v5 (виджет панели)
- MPRIS / системные медиа-клавиши
- Экспериментальный FFT-визуализатор (агрегированные метрики `aspectralstats`)
- AppImage для Linux x86_64 (libmpv и FFmpeg внутри)
- Flatpak для Linux x86_64 (libmpv/libass/libplacebo из исходников)
- MSI для Windows x64 (libmpv из MSYS2, WiX) и DMG для macOS (libmpv из Homebrew)
- Single-instance архитектура (Unix-сокет / Windows named pipe)

## Статус

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
а также публикует неподписанные MSI и DMG артефактами. Кроссплатформенные
медиа-интеграции — привязка SMTC к HWND и Now Playing на macOS — пока не
реализованы.

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
  правила плавающего окна 920×660.

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

- Windows: `packaging/windows/build-msi.ps1` — MSI через WiX 7, libmpv из MSYS2
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
