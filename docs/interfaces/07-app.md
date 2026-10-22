# Интерфейс: Application Core

Application Core не экспортирует публичный трейт для других crate’ов — он является точкой сборки.

Тем не менее, фиксируем контракт поведения:

## Обязанности

1. Инициализировать все подсистемы в корректном порядке.
2. Реализовывать `eframe::App`.
3. Маршрутизировать `IpcCommand` и `MediaCommand` в соответствующие модули.
4. Управлять видимостью окна (show/hide, а не destroy).
5. При клике по глобусу:
   - получить точку на сфере от `GlobeRenderer`
   - спросить страну у `GeoService`
   - при необходимости запросить станции через `RadioClient`
   - обновить UI

## Порядок инициализации

Текущий (Этап 2):

```
1. IpcServer::try_bind()          — ошибка AlreadyRunning → повторная отправка команды
2. AudioEngine::new()             — ошибка фатальна для запуска
3. MediaControls::new(wake)       — ошибка логируется, приложение работает без MPRIS
4. фоновая задача: audio events → wake()
5. IpcServer::run(wake, shutdown) — accept-loop
6. eframe::run_native + OrbitApp  — GUI, после закрытия которого сервер
                                    останавливается и сокет удаляется
```

Целевой порядок после Этапов 3–4: `GeoService` и `RadioClient` добавляются до
`MediaControls`, а `GlobeRenderer` — перед созданием `OrbitApp`.

## Реакция на команды

| Команда       | Действие |
|---------------|----------|
| Toggle/Show/Hide | Изменение `window_visible` + ViewportCommand |
| Play          | На Этапе 2 — `error каталог станций ещё не подключён (Этап 3)`; после Этапа 3 — `radio.send_click` + `audio.play(url_resolved)` + media |
| Stop          | `audio.stop()`, сброс `current_url`, обновление media |
| Volume        | `audio.set_volume(level)`, обновление media |
| Status        | `status <state>`, `volume <0.00>`, `window visible\|hidden` |
| Quit          | Корректное завершение всех подсистем и выход |

`MediaCommand` обрабатывается в том же цикле:

| Команда | Действие |
|---------|----------|
| Play | снять паузу; если поток остановлен — повторно запустить последний URL |
| Pause | поставить на паузу |
| PlayPause | переключить в зависимости от `PlaybackState` |
| Stop | `audio.stop()` |
| SetVolume | `audio.set_volume()` |
| Next/Previous | пока логируются, переключение станций — Этап 3 |

Аудио-вызовы из синхронного `logic()` выполняются через
`tokio::runtime::Handle::block_on`; сами операции короткие (ответ аудио-потока с
таймаутом 3 с).

Ответ клиенту формируется после обновления состояния App и отправки `ViewportCommand`;
применение команды оконным менеджером асинхронно и в ответе не подтверждается.
`window` в `status` отражает состояние, управляемое OrbitFM, а не минимизацию WM.

## Зависимости

`OrbitApp` использует конкретные реализации (`AudioEngine`, `MediaControls`) и каналы
`tokio::sync` как границы. Подмена реализаций в тестах не требуется; модульные тесты
проверяют чистые функции (`format_status`, `playback_label`) и логику внутри модулей.
