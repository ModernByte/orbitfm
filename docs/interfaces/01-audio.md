# Интерфейс: Audio Engine

Реализация — конкретная структура `AudioEngine` (`src/audio/`), публичный трейт не
используется. Методы, изменяющие состояние, асинхронные; команды выполняются в выделенном
аудио-потоке с таймаутом 3 с. Ошибка выполнения команды не меняет состояние молча: движок
публикует соответствующее событие.

```rust
use tokio::sync::broadcast;

/// Состояние воспроизведения.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
    Buffering,
    Reconnecting,
    Error(String),
}

impl PlaybackState {
    /// Короткое имя для IPC-статуса.
    pub fn as_str(&self) -> &'static str;
    // stopped | playing | paused | buffering | reconnecting | error
}

/// События аудио-движка.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioEvent {
    Started { url: String },
    Stopped,
    Buffering,
    Reconnecting { attempt: u32 },
    Error { message: String },
    EndOfFile,
}

/// Ошибки аудио-подсистемы.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("не удалось инициализировать libmpv: {0}")]
    Init(String),
    #[error("ошибка воспроизведения: {0}")]
    Playback(String),
    #[error("некорректный URL")]
    InvalidUrl,
    #[error("внутренняя ошибка: {0}")]
    Internal(String),
}

/// Аудио-движок на базе libmpv.
pub struct AudioEngine { /* ... */ }

impl AudioEngine {
    /// Создаёт libmpv-контекст и запускает выделенный аудио-поток.
    pub fn new() -> Result<Self, AudioError>;

    /// Запускает воспроизведение URL (только http/https с непустым хостом).
    pub async fn play(&mut self, url: &str) -> Result<(), AudioError>;

    /// Останавливает воспроизведение; состояние переходит в `Stopped`.
    pub async fn stop(&mut self) -> Result<(), AudioError>;

    /// Ставит на паузу или возобновляет; требует активного URL.
    pub async fn set_paused(&mut self, paused: bool) -> Result<(), AudioError>;

    /// Громкость `0.0..=1.0`; начальное значение — 0.7.
    pub async fn set_volume(&mut self, volume: f64) -> Result<(), AudioError>;

    /// Текущее состояние.
    pub fn state(&self) -> PlaybackState;

    /// Текущая громкость.
    pub fn volume(&self) -> f64;

    /// Подписка на события.
    pub fn subscribe(&self) -> broadcast::Receiver<AudioEvent>;
}
```

## 1. Семантика команд

| Команда | Успех | Ошибки |
|---------|-------|--------|
| `play(url)` | состояние `Buffering`, событие `Buffering`; после загрузки — `Playing`/`Paused` и `Started` | `InvalidUrl`, `Playback` (mpv не принял `loadfile`), `Internal` (поток остановлен/таймаут) |
| `stop()` | состояние `Stopped`, событие `Stopped` | `Playback`, `Internal` |
| `set_paused(true/false)` | `Paused` / `Playing`; при возобновлении публикуется `Started` | `Playback`, если нет активного URL или состояние `Stopped`/`Reconnecting`/`Error` |
| `set_volume(v)` | значение применяется к mpv и к `volume()` | `Playback`, если `v` не конечное число или вне `0.0..=1.0` |

## 2. Реконнект

- При обрыве активного потока движок переходит в `Reconnecting` и публикует
  `Reconnecting { attempt }`.
- Не более 5 попыток; задержки — 1, 2, 4, 8 и 16 с.
- После исчерпания попыток состояние — `Error(message)`, событие `Error`.
- Успешная загрузка (`FileLoaded`) сбрасывает счётчик попыток.
- `stop()` и новый `play()` также сбрасывают счётчик.

Естественное завершение потока даёт `EndOfFile`, после чего запускается та же политика
реконнекта (для живого радио это эквивалентно обрыву).

## 3. Потоки и жизненный цикл

- Все вызовы libmpv выполняются только в потоке `orbitfm-audio`.
- `AudioEngine::Drop` отправляет команду `Shutdown` и дожидается завершения потока.
- События доставляются через `broadcast`-канал ёмкостью 64; отставший подписчик получает
  `RecvError::Lagged`, а не блокирует движок.
