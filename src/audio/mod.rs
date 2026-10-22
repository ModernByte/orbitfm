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

/// Кадр экспериментального FFT-визуализатора.
///
/// Значения приходят из `aspectralstats`/`astats` и описывают кадр целиком,
/// а не отдельные полосы спектра.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpectrumFrame {
    /// Спектральный центроид, Гц.
    pub centroid_hz: f32,
    /// Спектральный разброс, Гц.
    pub spread_hz: f32,
    /// Верхняя граница 85% энергии, Гц.
    pub rolloff_hz: f32,
    /// Спектральная плоскостность, `0.0..=1.0`.
    pub flatness: f32,
    /// Спектральный поток (сырое значение FFmpeg).
    pub flux: f32,
    /// Уровень RMS, дБ (обычно `-80.0..=0.0`).
    pub rms_db: f32,
}

/// Ошибки аудио-подсистемы.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("не удалось инициализировать libmpv: {0}")]
    Init(String),
    #[error("ошибка воспроизведения: {0}")]
    Playback(String),
    #[error("визуализатор недоступен: {0}")]
    Spectrum(String),
    #[error("некорректный URL")]
    InvalidUrl,
    #[error("внутренняя ошибка: {0}")]
    Internal(String),
}
