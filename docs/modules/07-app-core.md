# Модуль: Application Core

**Путь в коде:** `src/app.rs`, `src/main.rs`  
**Ответственность:** Связывание всех подсистем, состояние приложения, главный цикл egui, маршрутизация команд.

---

## 1. Назначение

Центральный оркестратор.  
Не содержит бизнес-логики доменов, только координацию.

## 2. Основные обязанности

- Инициализация всех модулей в правильном порядке
- Хранение shared state (видимость окна, текущий URL, флаг завершения)
- Маршрутизация `IpcCommand` и `MediaCommand` в соответствующие модули
- Реализация `eframe::App` (`logic` + `ui` в eframe 0.37)
- Управление видимостью окна (show/hide вместо destroy)

## 3. Состояние приложения

Целевое состояние (после Этапов 3–4) добавит `radio`, `geo`, `globe` и `current_station`.

Текущее состояние (Этап 2):

```rust
pub struct AppState {
    pub window_visible: bool,
}

pub struct OrbitApp {
    state: AppState,
    commands: mpsc::Receiver<IpcRequest>,
    audio: AudioEngine,
    audio_events: broadcast::Receiver<AudioEvent>,
    media: Option<MediaControls>,
    current_url: Option<String>,
    runtime: tokio::runtime::Handle,
    quitting: bool,
}
```

`current_url` заполняется из `AudioEvent::Started` и очищается на `Stopped`/`Error`.
Он используется для паузы/возобновления из MPRIS. `media` — `Option`, потому что MPRIS
может быть недоступен (нет D-Bus); приложение в этом случае продолжает работать.

## 4. Порядок инициализации

```
IpcServer::try_bind → AudioEngine::new → MediaControls::new(wake)
→ задача audio events → wake → IpcServer::run → eframe
```

`try_bind` выполняется до создания аудио-ядра, чтобы второй экземпляр завершился, не
занимая аудиоустройство. Ошибка `AudioEngine::new` фатальна; ошибка MPRIS — только
предупреждение.

## 5. Главный цикл (логика)

Каждый `logic()`:

1. Разбирает накопившиеся `AudioEvent`, обновляя `current_url`.
2. Обрабатывает очередь `IpcRequest` и отвечает через `IpcRequest::try_respond`.
3. Разбирает `MediaCommand` от MPRIS.
4. Синхронизирует состояние с MPRIS (`sync_media`).
5. Перехватывает закрытие окна (`close_requested`) и превращает его в Hide, если это не
   `quit`.

`ui()` рисует карточку только при видимом окне. В eframe 0.37 скрытое окно не вызывает
`ui()`, но `logic()` продолжает выполняться при запрошенном repaint. Поэтому IPC- и
MPRIS-команды обрабатываются и в скрытом состоянии; пробуждение выполняют `WakeFn` из
IPC-сервера, MPRIS-колбэка и задача аудио-событий.

## 6. Синхронизация с MPRIS

`sync_media()` вызывается каждый кадр, но `MediaControls::set_playback` дедуплицирует
неизменившееся состояние. Соответствие:

- `Stopped`/`Error` → трек отсутствует, `Stopped`;
- `Paused` → трек есть, `Paused`;
- `Playing`/`Buffering`/`Reconnecting` → трек есть, `Playing`.

Пока Radio Browser не подключён, заголовком служит URL потока.

## 7. Жизненный цикл окна

- Закрытие крестиком → `ViewportCommand::CancelClose` + `ViewportCommand::Visible(false)`,
  `window_visible = false`
- Команда `toggle` / `show` → `Visible(true)` + `Minimized(false)` + `Focus`; если окно
  свёрнуто оконным менеджером, `toggle` восстанавливает его, а не скрывает
- Команда `quit` → `quitting = true` + `ViewportCommand::Close`; после выхода из GUI main
  останавливает IPC-сервер, аудио-поток и удаляет сокет

## 8. Текущие ограничения

- `play <uuid>` недоступен до Этапа 3 (нет каталога станций)
- `Next`/`Previous` из MPRIS не переключают станции
- 3D-глобус заменён заглушкой до Этапа 4
- Клик мимо окна (overlay) ещё не реализован
- Метаданные станции в MPRIS ограничены URL

## 9. Связанные документы

- ADR-0002, ADR-0005, ADR-0006
- `docs/architecture/00-overview.md`
