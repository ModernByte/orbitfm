//! UI-слой Focus-раскладки «Orbital Desk».
//!
//! Модули получают только нужные срезы состояния и возвращают [`UiAction`];
//! побочные эффекты выполняет `OrbitApp`.

pub(crate) mod globe_view;
pub(crate) mod hud;
pub(crate) mod icons;
pub(crate) mod palette;
pub(crate) mod station_panel;
pub(crate) mod toast;
pub(crate) mod transport;

use crate::audio::PlaybackState;
use crate::geo::Country;
use crate::services::Station;
use crate::settings::{AccentColor, ThemeMode};

/// Действие, которое UI просит выполнить у Application Core.
#[derive(Debug, Clone)]
pub(crate) enum UiAction {
    Play(Station),
    ToggleFavorite(Station),
    SelectCountry(Country),
    RunSearch,
    ClearHistory,
    SetTheme(ThemeMode),
    SetAccent(AccentColor),
    ToggleSpectrum(bool),
    SetVolume(f64),
    TogglePlayback,
    Stop,
    Hide,
    Quit,
    TogglePanel,
}

pub(crate) fn playback_label(state: &PlaybackState) -> String {
    match state {
        PlaybackState::Stopped => "остановлено".to_owned(),
        PlaybackState::Playing => "играет".to_owned(),
        PlaybackState::Paused => "пауза".to_owned(),
        PlaybackState::Buffering => "буферизация".to_owned(),
        PlaybackState::Reconnecting => "переподключение".to_owned(),
        PlaybackState::Error(message) => format!("ошибка: {message}"),
    }
}

pub(crate) fn single_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }

    let mut truncated: String = value.chars().take(max_chars.saturating_sub(1)).collect();
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_labels_are_human_readable() {
        assert_eq!(playback_label(&PlaybackState::Stopped), "остановлено");
        assert_eq!(playback_label(&PlaybackState::Playing), "играет");
        assert_eq!(
            playback_label(&PlaybackState::Error("таймаут".to_owned())),
            "ошибка: таймаут"
        );
    }

    #[test]
    fn truncates_by_characters() {
        assert_eq!(truncate("коротко", 10), "коротко");
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("абвгде", 4), "абв…");
    }

    #[test]
    fn single_line_collapses_whitespace() {
        assert_eq!(single_line("Radio\n  Nova\tLive"), "Radio Nova Live");
    }
}
