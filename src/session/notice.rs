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
    SnapshotCleared,
    SnapshotClearFailed,
    PlaybackFailed,
    StationWithoutUrl,
    SearchTaskEnded,
    LoadMoreFailed,
    RouletteStarted {
        station: Option<String>,
    },
    /// Станция заменена более лёгким потоком (ADR-0043 §4).
    EconomyStream {
        bitrate: u32,
    },
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
    /// Таймер сна остановил эфир (ADR-0046).
    SleepExpired,
    /// Ингибитор сна снят извне: система может уснуть раньше таймера (ADR-0054).
    InhibitionLost,
    /// Действие таймера сна (сон или выключение) не выполнено (ADR-0055).
    SleepActionFailed,
    /// Файл статистики при старте не разобрался: счётчики с нуля (ADR-0049 § 2).
    StatsDamaged,
    /// Файл настроек при старте не разобрался и отодвинут (ADR-0062).
    SettingsDamaged,
    /// Библиотека станций при старте не разобралась и отодвинута (ADR-0062).
    LibraryDamaged,
    /// Аудио-поток завершился паникой: воспроизведение недоступно (ADR-0061).
    AudioWorkerFailed,
}
