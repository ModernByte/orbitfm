# Интерфейс: Media Controls

```rust
/// Информация, отображаемая системой
#[derive(Debug, Clone)]
pub struct PlaybackInfo {
    pub title: String,
    pub artist: Option<String>,
    pub art_url: Option<String>,
    pub playing: bool,
    pub volume: f64,
}

/// Команды, приходящие от системы
#[derive(Debug, Clone)]
pub enum MediaCommand {
    Play,
    Pause,
    PlayPause,
    Stop,
    Next,
    Previous,
    SetVolume(f64),
}

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("не удалось инициализировать медиа-контроли: {0}")]
    Init(String),
    #[error("платформа не поддерживается")]
    UnsupportedPlatform,
}

/// Контракт
pub trait MediaControls: Send {
    fn set_playback(&mut self, info: &PlaybackInfo);

    fn events(&self) -> tokio::sync::mpsc::Receiver<MediaCommand>;
}
```
