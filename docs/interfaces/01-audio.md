# Интерфейс: Audio Engine

```rust
use tokio::sync::broadcast;

/// Состояние воспроизведения
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Buffering,
    Reconnecting,
    Error,
}

/// События аудио-движка
#[derive(Debug, Clone)]
pub enum AudioEvent {
    Started { url: String },
    Stopped,
    Buffering,
    Reconnecting { attempt: u32 },
    Error { message: String },
}

/// Ошибки аудио-подсистемы
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

/// Контракт аудио-движка
#[async_trait::async_trait]
pub trait AudioEngine: Send + Sync {
    /// Начать воспроизведение (url_resolved предпочтителен)
    async fn play(&mut self, url: &str) -> Result<(), AudioError>;

    /// Остановить
    async fn stop(&mut self) -> Result<(), AudioError>;

    /// Громкость 0.0 ..= 1.0
    async fn set_volume(&mut self, volume: f64) -> Result<(), AudioError>;

    /// Текущее состояние
    fn state(&self) -> PlaybackState;

    /// Текущая громкость
    fn volume(&self) -> f64;

    /// Подписка на события
    fn subscribe(&self) -> broadcast::Receiver<AudioEvent>;
}
```
