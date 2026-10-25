//! Типизированные сообщения Application Core (ADR-0030).
//!
//! Ядро не знает про язык интерфейса: оно называет причину, а presentation
//! превращает её в строку каталога. Технический `detail` не переводится.

/// Сообщение для пользователя. Как его показать, решает UI (обычно тостом).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Notice {
    Info(NoticeKey),
    Error {
        key: NoticeKey,
        detail: Option<String>,
    },
}

/// Причина сообщения; тексты живут в `i18n::Catalog::notices`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NoticeKey {
    HistoryNotSaved,
    FavoritesNotSaved,
    HistoryCleared,
    ClearHistoryFailed,
    PlaybackFailed,
    StationWithoutUrl,
    SearchTaskEnded,
    RouletteStarted { station: Option<String> },
    RouletteFailed,
    RouletteTaskEnded,
    ThemeNotSaved,
    AccentNotSaved,
    MotionNotSaved,
    SettingNotSaved,
    VolumeNotSaved,
    LanguageNotSaved,
    VisualizerNotSaved,
    FftUnavailable,
}
