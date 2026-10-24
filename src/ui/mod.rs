//! UI-слой Focus-раскладки «Orbital Desk».
//!
//! Модули получают только нужные срезы состояния и возвращают [`UiAction`];
//! побочные эффекты выполняет `OrbitApp`.

pub(crate) mod capsule;
pub(crate) mod globe_view;
pub(crate) mod hud;
pub(crate) mod icons;
pub(crate) mod images;
pub(crate) mod layout;
pub(crate) mod mode_switch;
pub(crate) mod orbit_ring;
pub(crate) mod overlays;
pub(crate) mod palette;
pub(crate) mod station_filter;
pub(crate) mod station_panel;
pub(crate) mod station_row;
pub(crate) mod toast;
pub(crate) mod transport;
pub(crate) mod widgets;

use crate::audio::PlaybackState;
use crate::geo::Country;
use crate::services::Station;
use crate::session::WindowMode;
use crate::settings::{AccentColor, MotionReactivity, MotionStyle, ThemeMode};

/// Режим поиска выбирает ядро, UI только показывает переключатель.
pub(crate) use crate::session::search::SearchMode;

/// Действие, которое UI просит выполнить у Application Core.
#[derive(Debug, Clone)]
pub(crate) enum UiAction {
    Play(Station),
    ToggleFavorite(Station),
    SelectCountry(Country),
    SwitchMode(SearchMode),
    SelectGenre(String),
    RunSearch,
    Roulette,
    ClearHistory,
    SetTheme(ThemeMode),
    SetAccent(AccentColor),
    ToggleSpectrum(bool),
    /// Число столбцов визуализатора: 16, 24 или 32 (ADR-0019).
    SetVizBars(u8),
    /// Реактивность аудио-движения глобуса (ADR-0028).
    SetMotion(MotionReactivity),
    /// Характер движения: Auto или один из шести стилей.
    SetMotionStyle(MotionStyle),
    /// Перевороты камеры на сильном onset (ADR-0028).
    SetAllowFlips(bool),
    /// Reduce motion: выключить геометрическое движение, оставив эфир.
    SetReduceMotion(bool),
    SetVolume(f64),
    TogglePlayback,
    Stop,
    /// Соседняя станция видимого списка: -1 — назад, +1 — вперёд, с wrap.
    PlayRelative(i8),
    Hide,
    Quit,
    TogglePanel,
    ToggleHelp,
    /// Смена режима окна Focus/Overlay/Listening (ADR-0021).
    SetWindowMode(WindowMode),
}

pub(crate) fn playback_label(state: &PlaybackState) -> String {
    match state {
        PlaybackState::Stopped => "stopped".to_owned(),
        PlaybackState::Playing => "on air".to_owned(),
        PlaybackState::Paused => "paused".to_owned(),
        PlaybackState::Buffering => "buffering".to_owned(),
        PlaybackState::Reconnecting => "reconnecting".to_owned(),
        PlaybackState::Error(message) => format!("error: {message}"),
    }
}

pub(crate) fn single_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn country_label(country: &Country) -> String {
    let name = single_line(&country.name);
    let name = if name.is_empty() {
        country.iso_a2.clone()
    } else {
        name
    };

    match country.iso_a3.as_deref() {
        Some(iso_a3) => format!("{name} ({}, {iso_a3})", country.iso_a2),
        None => format!("{name} ({})", country.iso_a2),
    }
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
        assert_eq!(playback_label(&PlaybackState::Stopped), "stopped");
        assert_eq!(playback_label(&PlaybackState::Playing), "on air");
        assert_eq!(
            playback_label(&PlaybackState::Error("timeout".to_owned())),
            "error: timeout"
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

    #[test]
    fn country_label_includes_iso_codes() {
        let country = Country {
            name: "Germany".to_owned(),
            iso_a2: "DE".to_owned(),
            iso_a3: Some("DEU".to_owned()),
        };
        assert_eq!(country_label(&country), "Germany (DE, DEU)");
    }

    #[test]
    fn country_label_falls_back_to_iso_codes() {
        let country = Country {
            name: "  ".to_owned(),
            iso_a2: "XK".to_owned(),
            iso_a3: None,
        };
        assert_eq!(country_label(&country), "XK (XK)");
    }
}
