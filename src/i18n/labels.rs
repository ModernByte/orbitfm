//! Подписи enum'ов настроек: единственное место, где i18n знает о `settings`.

use crate::settings::{AccentColor, Language, MotionReactivity, MotionStyle, ThemeMode};

use super::catalog;

pub(crate) fn theme_label(ui: Language, theme: ThemeMode) -> &'static str {
    let labels = &catalog(ui).labels;
    match theme {
        ThemeMode::System => labels.theme_system,
        ThemeMode::Dark => labels.theme_dark,
        ThemeMode::Light => labels.theme_light,
    }
}

pub(crate) fn accent_label(ui: Language, accent: AccentColor) -> &'static str {
    let labels = &catalog(ui).labels;
    match accent {
        AccentColor::Blue => labels.accent_orbit,
        AccentColor::Green => labels.accent_emerald,
        AccentColor::Amber => labels.accent_amber,
        AccentColor::Violet => labels.accent_violet,
        AccentColor::Rose => labels.accent_rose,
        AccentColor::Cyan => labels.accent_ocean,
    }
}

pub(crate) fn motion_label(ui: Language, motion: MotionReactivity) -> &'static str {
    let labels = &catalog(ui).labels;
    match motion {
        MotionReactivity::Off => labels.motion_off,
        MotionReactivity::Low => labels.motion_low,
        MotionReactivity::Normal => labels.motion_normal,
        MotionReactivity::High => labels.motion_high,
    }
}

pub(crate) fn style_label(ui: Language, style: MotionStyle) -> &'static str {
    let labels = &catalog(ui).labels;
    match style {
        MotionStyle::Auto => labels.style_auto,
        MotionStyle::SoftPulse => labels.style_soft_pulse,
        MotionStyle::GrooveSway => labels.style_groove_sway,
        MotionStyle::BassDrop => labels.style_bass_drop,
        MotionStyle::AmbientDrift => labels.style_ambient_drift,
        MotionStyle::SparkleTwitch => labels.style_sparkle_twitch,
        MotionStyle::BeatMarch => labels.style_beat_march,
    }
}

/// `System` показывается как «Система» на текущем языке; остальные — нативно.
pub(crate) fn language_label(ui: Language, value: Language) -> &'static str {
    match value {
        Language::System => catalog(ui).settings.system_language,
        other => other.native_name(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_labels_match_the_previous_ui_wording() {
        let ui = Language::English;
        assert_eq!(theme_label(ui, ThemeMode::Dark), "Dark");
        assert_eq!(accent_label(ui, AccentColor::Blue), "Orbit");
        assert_eq!(accent_label(ui, AccentColor::Cyan), "Ocean");
        assert_eq!(motion_label(ui, MotionReactivity::Normal), "Normal");
        assert_eq!(style_label(ui, MotionStyle::BeatMarch), "Beat March");
        assert_eq!(language_label(ui, Language::System), "System");
        assert_eq!(language_label(ui, Language::Japanese), "日本語");
    }

    #[test]
    fn every_theme_and_accent_has_a_label() {
        let ui = Language::English;
        for theme in ThemeMode::ALL {
            assert!(!theme_label(ui, theme).is_empty());
        }
        for accent in AccentColor::ALL {
            assert!(!accent_label(ui, accent).is_empty());
        }
        for motion in MotionReactivity::ALL {
            assert!(!motion_label(ui, motion).is_empty());
        }
        for style in MotionStyle::ALL {
            assert!(!style_label(ui, style).is_empty());
        }
    }
}
