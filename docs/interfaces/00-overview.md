# Интерфейсы OrbitFM

**Организация:** ModernByte  
**Дата:** 2026-11-03

Этот каталог содержит формальные контракты между модулями и внешними системами.

## Содержание

| Файл | Описание |
|------|----------|
| `01-audio.md` | Контракт аудио-движка (`AudioEngine`, FFT-метрики) |
| `02-radio.md` | Контракт клиента Radio Browser |
| `03-geo.md` | Контракт геосервиса (`GeoService`, `Country`) |
| `04-render.md` | Контракт рендерера глобуса и спектрального оверлея (`GlobeCamera`, `GlobeRenderer`, `SpectrumOverlay`) |
| `05-ipc.md` | IPC-протокол и команды |
| `06-media.md` | Контракт медиа-контролов (`MediaControls`) |
| `07-app.md` | Контракт presentation-слоя (`OrbitApp`, раскладка «Orbital Desk», цикл запуска) |
| `08-library.md` | Контракт библиотеки станций (`StationLibrary`, избранное и история) |
| `09-settings.md` | Контракт настроек оформления (`SettingsStore`, тема и акцент) |
| `10-session.md` | Контракт Application Core (`Session`, `Notice`, `UiIntent`, `ParkOutcome`, жизненный цикл окна по ADR-0018) |
| `11-tray.md` | Контракт системного трея (`TrayHandle`, `TrayCommand`, `TraySnapshot`, SNI по ADR-0020) |
| `12-i18n.md` | Контракт локализации (`Language`, каталог строк, `TrayText` по ADR-0030) |
| `13-events.md` | Поток событий состояния `orbitfm events`: конверт `ev: 1`, NDJSON (ADR-0039) |
| `14-native-host.md` | Native messaging host для расширений Chromium и Firefox: фрейминг, allowlist, конверт `{"ok":…}` (ADR-0038) |
| `15-library-export.md` | Экспорт и импорт библиотеки: схема 1, слияние по UUID, безопасность файлов (ADR-0046) |
| `16-tui.md` | Контракт `orbitfm-tui` ↔ ядро: CLI-адаптер, `events`, поиск, библиотека, кадр `spectrum`, выход `q`/`Q` (ADR-0047, план) |
| `17-stats.md` | Локальная статистика прослушивания: файл `stats.json` схемы 1, накопление секунд, команды `stats`, `stats --json`, `stats clear` (ADR-0049) |
| `18-aliases.md` | Алиасы URL станций: поле `aliases` в `library.json`, подмена адреса в `start_station`, команды `alias`, пометка «ручная ссылка» (ADR-0050) |

## Принципы

1. Интерфейсы описывают **поведение**, а не реализацию.
2. Все публичные типы ошибок — через `thiserror`.
3. Асинхронные операции возвращают `Result` и не паникуют.
4. Готовые подсистемы могут быть конкретными структурами; их контракт фиксируется сигнатурами
   методов, состояниями и событиями.
