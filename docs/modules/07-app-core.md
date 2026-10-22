# Модуль: Application Core

**Путь в коде:** `src/app.rs`, `src/main.rs`  
**Ответственность:** Связывание всех подсистем, состояние приложения, главный цикл egui, маршрутизация команд.

---

## 1. Назначение

Центральный оркестратор.  
Не содержит бизнес-логики доменов, только координацию.

## 2. Основные обязанности

- Инициализация всех модулей в правильном порядке
- Хранение shared state (текущая станция, видимость окна, громкость и т.д.)
- Маршрутизация `IpcCommand` и `MediaCommand` в соответствующие модули
- Реализация `eframe::App` (`logic` + `ui` в eframe 0.37)
- Управление видимостью окна (show/hide вместо destroy)

## 3. Состояние приложения

Целевое состояние (после Этапов 2–4):

```rust
pub struct AppState {
    pub audio: AudioEngine,
    pub radio: RadioBrowserClient,
    pub geo: GeoService,
    pub globe: GlobeRenderer,
    pub media: MediaControls,
    
    pub current_station: Option<Station>,
    pub window_visible: bool,
    pub volume: f64,
    // ...
}
```

Текущее состояние (Этап 1):

```rust
pub struct AppState {
    pub window_visible: bool,
}

pub struct OrbitApp {
    state: AppState,
    commands: mpsc::Receiver<IpcRequest>,
    quitting: bool,
}
```

## 4. Главный цикл (логика)

Каждый кадр egui в целевой архитектуре:

1. Обработать накопившиеся IPC- и Media-команды
2. Обновить `GlobeRenderer` (input + dt)
3. Отрисовать UI (панель управления + область глобуса с PaintCallback)
4. При клике по глобусу → geo → список станций страны → UI

На Этапе 1 цикл ограничен:

1. `logic()` забирает запросы из IPC-очереди, выполняет команду и отвечает через
   `IpcRequest::try_respond`
2. Перехватывает закрытие окна (`close_requested`) и превращает его в Hide, если это не
   `quit`
3. `ui()` рисует карточку-заглушку только при видимом окне

В eframe 0.37 скрытое окно не вызывает `ui()`, но `logic()` продолжает выполняться при
запрошенном repaint. Поэтому IPC-команды обрабатываются и в скрытом состоянии.

## 5. Жизненный цикл окна

- Закрытие крестиком → `ViewportCommand::CancelClose` + `ViewportCommand::Visible(false)`,
  `window_visible = false` (клик мимо — поведение overlay-режима, позже)
- Команда `toggle` / `show` → `Visible(true)` + `Minimized(false)` + `Focus`; если окно
  свёрнуто оконным менеджером, `toggle` восстанавливает его, а не скрывает
- Команда `quit` → `quitting = true` + `ViewportCommand::Close`

## 6. Текущие ограничения

- Аудио-команды (`play`, `stop`, `volume`) возвращают
  `error аудио-ядро ещё не подключено (Этап 2)`
- `MediaCommand` пока не обрабатывается
- 3D-глобус заменён заглушкой до Этапа 4
- Клик мимо окна (overlay) ещё не реализован

## 7. Связанные документы

- ADR-0002, ADR-0005
- `docs/architecture/00-overview.md`
