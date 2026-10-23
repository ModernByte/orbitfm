//! Палитра и стили Orbital Desk.

use eframe::egui;

use crate::settings::{AccentColor, Settings, ThemeMode};

const CARD_ALPHA: u8 = 236;

pub(crate) fn configure_style(ctx: &egui::Context, settings: Settings) {
    ctx.set_theme(theme_preference(settings.theme));

    let accent = accent_color(settings.accent);
    let selection_text = contrast_text(accent);
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        let mut visuals = match theme {
            egui::Theme::Dark => egui::Visuals::dark(),
            egui::Theme::Light => egui::Visuals::light(),
        };
        visuals.panel_fill = egui::Color32::TRANSPARENT;
        visuals.window_fill = egui::Color32::TRANSPARENT;
        visuals.selection.bg_fill = accent;
        visuals.selection.stroke = egui::Stroke::new(1.0, selection_text);
        visuals.hyperlink_color = match theme {
            egui::Theme::Dark => accent,
            egui::Theme::Light => shade(accent, 0.6),
        };
        ctx.set_visuals_of(theme, visuals);
    }
}

pub(crate) fn theme_preference(mode: ThemeMode) -> egui::ThemePreference {
    match mode {
        ThemeMode::System => egui::ThemePreference::System,
        ThemeMode::Dark => egui::ThemePreference::Dark,
        ThemeMode::Light => egui::ThemePreference::Light,
    }
}

pub(crate) fn theme_label(mode: ThemeMode) -> &'static str {
    match mode {
        ThemeMode::System => "Системная",
        ThemeMode::Dark => "Тёмная",
        ThemeMode::Light => "Светлая",
    }
}

pub(crate) fn accent_label(accent: AccentColor) -> &'static str {
    match accent {
        AccentColor::Blue => "Орбита",
        AccentColor::Green => "Изумруд",
        AccentColor::Amber => "Янтарь",
        AccentColor::Violet => "Фиалка",
        AccentColor::Rose => "Роза",
        AccentColor::Cyan => "Океан",
    }
}

pub(crate) fn accent_color(accent: AccentColor) -> egui::Color32 {
    let [red, green, blue] = accent.rgb();
    egui::Color32::from_rgb(red, green, blue)
}

fn contrast_text(color: egui::Color32) -> egui::Color32 {
    let luma =
        0.299 * f32::from(color.r()) + 0.587 * f32::from(color.g()) + 0.114 * f32::from(color.b());
    if luma > 150.0 {
        egui::Color32::BLACK
    } else {
        egui::Color32::WHITE
    }
}

fn shade(color: egui::Color32, factor: f32) -> egui::Color32 {
    let scale = |channel: u8| (f32::from(channel) * factor).round() as u8;
    egui::Color32::from_rgb(scale(color.r()), scale(color.g()), scale(color.b()))
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Palette {
    pub card_fill: egui::Color32,
    pub card_stroke: egui::Color32,
    pub muted: egui::Color32,
    pub error: egui::Color32,
    pub subtle_fill: egui::Color32,
    pub globe_fill: egui::Color32,
    pub accent: egui::Color32,
    pub marker: egui::Color32,
}

impl Palette {
    pub(crate) fn for_ui(theme: egui::Theme, accent: AccentColor) -> Self {
        let marker = accent_color(accent);
        match theme {
            egui::Theme::Dark => Self {
                card_fill: egui::Color32::from_rgba_unmultiplied(12, 15, 23, CARD_ALPHA),
                card_stroke: egui::Color32::from_rgba_unmultiplied(255, 255, 255, 20),
                muted: egui::Color32::from_rgb(150, 158, 176),
                error: egui::Color32::from_rgb(255, 128, 128),
                subtle_fill: egui::Color32::from_rgba_unmultiplied(255, 255, 255, 8),
                globe_fill: egui::Color32::from_rgba_unmultiplied(255, 255, 255, 7),
                accent: marker,
                marker,
            },
            egui::Theme::Light => Self {
                card_fill: egui::Color32::from_rgba_unmultiplied(246, 248, 252, CARD_ALPHA),
                card_stroke: egui::Color32::from_rgba_unmultiplied(15, 20, 30, 40),
                muted: egui::Color32::from_rgb(88, 97, 116),
                error: egui::Color32::from_rgb(176, 40, 40),
                subtle_fill: egui::Color32::from_rgba_unmultiplied(15, 20, 30, 10),
                globe_fill: egui::Color32::from_rgba_unmultiplied(15, 20, 30, 8),
                accent: shade(marker, 0.6),
                marker,
            },
        }
    }

    pub(crate) fn glass(self) -> egui::Frame {
        egui::Frame::default()
            .fill(self.card_fill)
            .stroke(egui::Stroke::new(1.0, self.card_stroke))
            .corner_radius(egui::CornerRadius::same(16))
            .inner_margin(egui::Margin::symmetric(16, 10))
    }

    pub(crate) fn globe(self) -> egui::Frame {
        egui::Frame::default()
            .fill(self.card_fill)
            .stroke(egui::Stroke::new(1.0, self.card_stroke))
            .corner_radius(egui::CornerRadius::same(16))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_preference_maps_all_modes() {
        assert_eq!(
            theme_preference(ThemeMode::System),
            egui::ThemePreference::System
        );
        assert_eq!(
            theme_preference(ThemeMode::Dark),
            egui::ThemePreference::Dark
        );
        assert_eq!(
            theme_preference(ThemeMode::Light),
            egui::ThemePreference::Light
        );
    }

    #[test]
    fn contrast_text_picks_readable_foreground() {
        assert_eq!(
            contrast_text(accent_color(AccentColor::Amber)),
            egui::Color32::BLACK
        );
        assert_eq!(
            contrast_text(egui::Color32::from_rgb(20, 30, 60)),
            egui::Color32::WHITE
        );
    }

    #[test]
    fn light_palette_darkens_accent_but_keeps_marker_bright() {
        let dark = Palette::for_ui(egui::Theme::Dark, AccentColor::Blue);
        let light = Palette::for_ui(egui::Theme::Light, AccentColor::Blue);
        assert_eq!(dark.accent, dark.marker);
        assert_ne!(light.accent, light.marker);
        assert!(light.accent.r() < light.marker.r());
    }
}
