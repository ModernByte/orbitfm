# Changelog

Все значимые изменения проекта OrbitFM документируются в этом файле.

Формат основан на [Keep a Changelog](https://keepachangelog.com/ru/1.1.0/),  
проект следует [Semantic Versioning](https://semver.org/lang/ru/).

## [Unreleased]

### Добавлено
- Этап 3 — Radio Browser и запуск станций:
  - `src/services/`: `RadioBrowserClient` с ленивым DNS-discovery
    `all.api.radio-browser.info`, дедупликацией IP и случайной ротацией пула
  - отдельный `reqwest::Client` на каждый IP с `.resolve` (TLS/SNI остаются на имени
    хоста), User-Agent `OrbitFM/1.0 (ModernByte; Linux; Rust)`, таймаут 5 с
  - failover по пулу для сетевых ошибок/таймаутов/`5xx`/`429`/`408`, повторный
    discovery и один повтор; остальные `4xx` возвращаются сразу
  - поиск `/json/stations/search` (`hidebroken`, сортировка по голосам), станция по
    `/json/stations/byuuid`, click `/json/url`; in-memory кэш до 512 станций
  - `play <uuid>`: станция из результатов поиска или каталога (таймаут 8 с),
    воспроизведение `url_resolved`, асинхронный click
  - поиск станций в окне (поле + Enter/«Найти», до 30 результатов) и запуск по клику
  - MPRIS-метаданные из станции: название, теги, favicon, `url_resolved`
  - `status` дополнен строками `stationuuid` и `name` при активной станции
  - фоновый прогрев пула серверов при старте (ошибка не фатальна)
  - unit-тесты клиента и живой тест `searches_and_fetches_live_station` (вне CI)
- Этап 2 — аудио-ядро и базовый MPRIS:
  - `src/audio/`: безопасная обёртка `libmpv2` в выделенном потоке `orbitfm-audio`,
    команды через каналы tokio с таймаутом 3 с
  - состояния `Stopped`, `Playing`, `Paused`, `Buffering`, `Reconnecting`, `Error`;
    события `Started`, `Stopped`, `Buffering`, `Reconnecting`, `Error`, `EndOfFile`
  - play / pause / resume / stop / volume; громкость по умолчанию 0.7
  - реконнект: до 5 попыток с задержками 1, 2, 4, 8, 16 с
  - `LC_NUMERIC=C` и опции mpv `config=no`, `terminal=no`, `idle=yes`, `vid=no`,
    `network-timeout=10`; отслеживание `paused-for-cache`
  - `src/media/`: MPRIS v2 через playwire (`org.mpris.MediaPlayer2.OrbitFM`),
    метаданные, громкость, Play/Pause/PlayPause/Stop; пробуждение GUI через `WakeFn`
  - IPC `stop` и `volume` управляют аудио-ядром; `status` возвращает
    `status <state>`, `volume <0.00>`, `window visible|hidden`
  - живой тест `plays_and_stops_live_stream` (по запросу, вне CI)
- Этап 1 (MVP-0) — каркас приложения:
  - crate `orbitfm` (Rust 2024, MSRV 1.95), зависимости eframe/egui 0.37, wgpu 30, tokio 1.54
  - single-instance IPC через Unix Domain Socket (`$XDG_RUNTIME_DIR/orbitfm.sock`)
  - CLI: `toggle`, `show`, `hide`, `status`, `play`, `stop`, `volume`, `quit`, `-h`, `-V`
  - eframe/egui-скелет с прозрачным окном overlay и app-id `org.modernbyte.OrbitFM`
  - закрытие окна = Hide (`ViewportCommand::CancelClose` + `Visible(false)`), процесс продолжает работу
  - `status` на Этапе 1: `status stopped` + `window visible|hidden`
  - unit- и интеграционные тесты IPC (13 тестов)
- SRS v3.0
- Обзор архитектуры (`docs/architecture/00-overview.md`)
- ADR-0001: Фиксировать архитектурные решения
- ADR-0002: Выбор GUI-стека (eframe + egui + egui-wgpu)
- ADR-0003: Выбор аудио-движка (libmpv)
- ADR-0004: Выбор клиента Radio Browser
- ADR-0005: Single-instance и IPC
- ADR-0006: Медиа-контроли (MPRIS / системные кнопки)
- ADR-0007: Интеграция с Noctalia v5
- ADR-0008: Геопространственный поиск (Raycasting + Point-in-Polygon)
- Модульные спецификации (`docs/modules/`):
  - 01-audio-engine
  - 02-radio-browser
  - 03-geo
  - 04-render-globe
  - 05-ipc
  - 06-media-controls
  - 07-app-core
  - 08-noctalia-wm
- Интерфейсы (`docs/interfaces/`):
  - 00-overview
  - 01-audio
  - 02-radio
  - 03-geo
  - 04-render
  - 05-ipc
  - 06-media
  - 07-app
- ROADMAP
- VERSIONMAP
- CONTRIBUTING
- AGENTS
- README
- AI-скиллы (`.agents/skills/`):
  - orbitfm-core, orbitfm-architect, orbitfm-rust, orbitfm-patterns
  - orbitfm-gui, orbitfm-audio, orbitfm-radio
  - orbitfm-geo, orbitfm-ipc, orbitfm-integration, orbitfm-docs
- `docs/patterns/00-overview.md` — каталог паттернов
- ADR-0009: каталог паттернов
- Шаблоны GitHub: PR, issue (bug/feature/adr), COMMIT_CONVENTION
- `docs/templates/MERGE_CHECKLIST.md`, `PROJECT_STATUS.md`

### Изменено
- `stop`/`error` больше не сбрасывают последний URL: MPRIS `Play` перезапускает поток
- зависимости: добавлены `reqwest` 0.13 (rustls, json, query), `serde`, `thiserror`
- аудио-стек уточнён под ADR-0003: безопасная обёртка `libmpv2` вместо прямого
  `libmpv-sys`; MPRIS реализован через `playwire` (ADR-0006)
- CI устанавливает `libmpv-dev` для сборки и тестов
- IPC-контракт уточнён под реализацию: `IpcRequest` с oneshot-ответом, `commands(&mut self)`,
  `run(WakeFn, shutdown)`, завершение ответа по EOF, лимит команды 4096 байт, таймауты,
  права сокета `0600`
- `IpcServer` описан как конкретная структура (трейт не используется)
- MSRV в SRS: Rust 1.85+ → 1.95+ (требование eframe 0.37)
- app-id в примерах приведён к `org.modernbyte.OrbitFM`, plugin ID — к `org.modernbyte.orbitfm`
- Именование приведено к стандартам ModernByte
- AGENTS.md — команда субагентов, делегирование, git/docs, модульность, perf, patterns
- CONTRIBUTING — ссылки на шаблоны и patterns

### Исправлено
- IPC: чтение команды ограничено 4096 байтами без неограниченной аллокации; невалидный
  UTF-8 отклоняется; ответ клиента ограничен 64 КиБ; не более 64 соединений
- Single-instance: lock-файл `orbitfm.lock` устраняет гонку удаления живого сокета;
  проверяются абсолютность и права `0700` у `XDG_RUNTIME_DIR`; чужой не-сокет не удаляется
- `quit`: graceful shutdown с дренажом активных соединений гарантирует доставку `ok`
- CLI: не-UTF-8 аргументы дают ошибку вместо паники
- `toggle` восстанавливает свёрнутое окно, а не скрывает его

## [0.1.0] - TBD

Первый публичный прототип (планируется).
