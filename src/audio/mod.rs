//! Аудио-ядро OrbitFM: единственный владелец сессии libmpv.

mod engine;
mod worker;

pub use engine::AudioEngine;

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
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Playing => "playing",
            Self::Paused => "paused",
            Self::Buffering => "buffering",
            Self::Reconnecting => "reconnecting",
            Self::Error(_) => "error",
        }
    }
}

/// События, публикуемые аудио-ядром.
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
