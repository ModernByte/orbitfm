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

## Порядок инициализации (рекомендуемый)

```
1. GeoService::from_embedded()
2. AudioEngine::new()
3. RadioClient::new().await
4. MediaControls::new(...)
5. IpcServer::try_bind().await
6. GlobeRenderer (при наличии wgpu device)
7. AppState + eframe::run_native
```

На Этапе 1 реализованы только шаги 5 и 7; остальные подсистемы подключаются на
Этапах 2–4.

## Реакция на команды

| Команда       | Действие |
|---------------|----------|
| Toggle/Show/Hide | Изменение `window_visible` + ViewportCommand |
| Play          | `radio.send_click` + `audio.play(url_resolved)` + `media.set_playback` |
| Stop          | `audio.stop` + обновление media |
| Volume        | `audio.set_volume` + `media.set_playback` |
| Quit          | Корректное завершение всех подсистем и выход |

На Этапе 1 `Play`, `Stop` и `Volume` возвращают
`error аудио-ядро ещё не подключено (Этап 2)`; `Status` возвращает `status stopped` и
`window visible|hidden`.

Ответ клиенту формируется после обновления состояния App и отправки `ViewportCommand`;
применение команды оконным менеджером асинхронно и в ответе не подтверждается.
`window` в `status` отражает состояние, управляемое OrbitFM, а не минимизацию WM.

## Зависимости только через интерфейсы

`App` зависит от трейтов (`dyn AudioEngine`, `dyn RadioClient` и т.д.) в тестах,  
в production-сборке использует конкретные реализации.

На Этапе 1 App зависит только от `tokio::sync::mpsc::Receiver<IpcRequest>` и egui; это
соответствует принципу, но подсистемы ещё не подключены.
