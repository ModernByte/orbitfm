//! Палитра и стили Orbital Desk.

use eframe::egui;

use crate::settings::{AccentColor, Settings, ThemeMode};

/// `Style::animation_time` по умолчанию в egui 0.37.
const EGUI_ANIMATION_TIME: f32 = 0.2;

pub(crate) fn configure_style(ctx: &egui::Context, settings: Settings) {
    ctx.set_theme(theme_preference(settings.theme));
    // Reduce motion — мастер-выключатель не только нашей геометрии, но и
    // собственных переходов egui (ADR-0032).
    ctx.all_styles_mut(|style| {
        style.animation_time = if settings.reduce_motion {
            0.0
        } else {
            EGUI_ANIMATION_TIME
        };
    });

    let accent = accent_color(settings.accent);
    let selection_text = contrast_text(accent);
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        let mut visuals = match theme {
            egui::Theme::Dark => egui::Visuals::dark(),
            egui::Theme::Light => egui::Visuals::light(),
        };
        visuals.panel_fill = egui::Color32::TRANSPARENT;
        // Попапы и подсказки живут в `window_fill`: прозрачность центральной
        // панели не должна оставлять меню сортировки без фона над глобусом.
        visuals.window_fill = match theme {
            egui::Theme::Dark => egui::Color32::from_rgba_unmultiplied(18, 23, 38, 246),
            egui::Theme::Light => egui::Color32::from_rgba_unmultiplied(248, 250, 253, 246),
        };
        visuals.menu_corner_radius = egui::CornerRadius::same(12);
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
    pub accent: egui::Color32,
    pub marker: egui::Color32,
    /// Космос под full-bleed глобусом: верх и низ вертикальной заливки (ТЗ §3.1).
    pub space_top: egui::Color32,
    pub space_bottom: egui::Color32,
    /// Верх и низ стеклянной панели: `linear-gradient(180deg, …)` из ТЗ §3.1.
    pub glass_top: egui::Color32,
    pub glass_bottom: egui::Color32,
    /// Верхний блик стекла 1–2 px на верхней кромке панели.
    pub rim_light: egui::Color32,
    /// Индикатор «в эфире» (LIVE, эквалайзер).
    pub live: egui::Color32,
    /// Фон pill-чипов и второстепенных кнопок.
    pub chip_fill: egui::Color32,
    pub chip_stroke: egui::Color32,
    /// Основной читаемый текст на стекле.
    pub text: egui::Color32,
}

impl Palette {
    pub(crate) fn for_ui(theme: egui::Theme, accent: AccentColor) -> Self {
        let marker = accent_color(accent);
        match theme {
            egui::Theme::Dark => Self {
                card_fill: egui::Color32::from_rgba_unmultiplied(13, 18, 32, 235),
                card_stroke: egui::Color32::from_rgba_unmultiplied(120, 160, 255, 36),
                muted: egui::Color32::from_rgb(150, 158, 176),
                error: egui::Color32::from_rgb(255, 128, 128),
                accent: marker,
                marker,
                space_top: egui::Color32::from_rgb(7, 10, 16),
                space_bottom: egui::Color32::from_rgb(11, 18, 32),
                glass_top: egui::Color32::from_rgba_unmultiplied(30, 40, 60, 224),
                glass_bottom: egui::Color32::from_rgba_unmultiplied(14, 18, 28, 235),
                rim_light: egui::Color32::from_rgba_unmultiplied(255, 255, 255, 15),
                live: egui::Color32::from_rgb(52, 211, 153),
                chip_fill: egui::Color32::from_rgba_unmultiplied(255, 255, 255, 12),
                chip_stroke: egui::Color32::from_rgba_unmultiplied(120, 160, 255, 36),
                text: egui::Color32::from_rgb(230, 234, 245),
            },
            egui::Theme::Light => Self {
                card_fill: egui::Color32::from_rgba_unmultiplied(246, 248, 252, 235),
                card_stroke: egui::Color32::from_rgba_unmultiplied(15, 20, 30, 40),
                muted: egui::Color32::from_rgb(88, 97, 116),
                error: egui::Color32::from_rgb(176, 40, 40),
                accent: shade(marker, 0.6),
                marker,
                space_top: egui::Color32::from_rgb(233, 238, 248),
                space_bottom: egui::Color32::from_rgb(214, 222, 238),
                glass_top: egui::Color32::from_rgba_unmultiplied(250, 251, 253, 226),
                glass_bottom: egui::Color32::from_rgba_unmultiplied(236, 240, 248, 235),
                rim_light: egui::Color32::from_rgba_unmultiplied(255, 255, 255, 160),
                live: egui::Color32::from_rgb(16, 150, 100),
                chip_fill: egui::Color32::from_rgba_unmultiplied(15, 20, 30, 14),
                chip_stroke: egui::Color32::from_rgba_unmultiplied(15, 20, 30, 30),
                text: egui::Color32::from_rgb(20, 26, 40),
            },
        }
    }

    /// Каркас glass-панели: поля и отступы. Саму стеклянную заливку рисует
    /// [`super::widgets::glass_panel`] по прямоугольнику зоны из `DeskLayout`,
    /// потому что `egui::Frame` умеет только сплошную заливку.
    fn shell(self, radius: u8, margin: egui::Margin) -> egui::Frame {
        egui::Frame::default()
            .fill(egui::Color32::TRANSPARENT)
            .corner_radius(egui::CornerRadius::same(radius))
            .inner_margin(margin)
    }

    pub(crate) fn glass(self) -> egui::Frame {
        self.shell(16, egui::Margin::symmetric(16, 10))
    }

    /// Плавающая стеклянная панель с заданным скруглением.
    pub(crate) fn panel(self, radius: u8) -> egui::Frame {
        self.shell(radius, egui::Margin::same(12))
    }
}

/// Рампа плотности станций (ТЗ §3.2): blue → cyan → green → yellow → orange → red.
const HEATMAP_STOPS: [egui::Color32; 6] = [
    egui::Color32::from_rgb(0x3B, 0x82, 0xF6),
    egui::Color32::from_rgb(0x22, 0xD3, 0xEE),
    egui::Color32::from_rgb(0x34, 0xD3, 0x99),
    egui::Color32::from_rgb(0xFB, 0xBF, 0x24),
    egui::Color32::from_rgb(0xF9, 0x73, 0x16),
    egui::Color32::from_rgb(0xEF, 0x44, 0x44),
];

/// Линейная интерполяция двух цветов в unmultiplied-представлении.
pub(crate) fn mix_colors(from: egui::Color32, to: egui::Color32, t: f32) -> egui::Color32 {
    let t = if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let [r0, g0, b0, a0] = from.to_srgba_unmultiplied();
    let [r1, g1, b1, a1] = to.to_srgba_unmultiplied();
    let channel = |a: u8, b: u8| {
        (f32::from(a) + (f32::from(b) - f32::from(a)) * t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    egui::Color32::from_rgba_unmultiplied(
        channel(r0, r1),
        channel(g0, g1),
        channel(b0, b1),
        channel(a0, a1),
    )
}

/// Цвет плотности по нормализованной интенсивности `0..1`.
pub(crate) fn heat_color(intensity: f32) -> egui::Color32 {
    let segments = (HEATMAP_STOPS.len() - 1) as f32;
    let scaled = if intensity.is_finite() {
        intensity.clamp(0.0, 1.0)
    } else {
        0.0
    } * segments;
    let index = (scaled.floor() as usize).min(HEATMAP_STOPS.len() - 2);
    mix_colors(
        HEATMAP_STOPS[index],
        HEATMAP_STOPS[index + 1],
        scaled - index as f32,
    )
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

    #[test]
    fn glass_is_darker_at_the_bottom_and_has_a_top_rim() {
        let palette = Palette::for_ui(egui::Theme::Dark, AccentColor::Blue);
        assert!(f32::from(palette.glass_top.r()) > f32::from(palette.glass_bottom.r()));
        assert!(f32::from(palette.glass_top.g()) > f32::from(palette.glass_bottom.g()));
        assert!(palette.space_top != palette.space_bottom);
        assert!(f32::from(palette.rim_light.a()) > 0.0);
    }

    #[test]
    fn heat_ramp_runs_blue_to_red_and_clamps() {
        assert_eq!(heat_color(0.0), HEATMAP_STOPS[0]);
        assert_eq!(heat_color(1.0), HEATMAP_STOPS[HEATMAP_STOPS.len() - 1]);
        assert_eq!(heat_color(-3.0), HEATMAP_STOPS[0]);
        assert_eq!(heat_color(f32::NAN), HEATMAP_STOPS[0]);
        assert_eq!(heat_color(0.4), HEATMAP_STOPS[2]);
        // Средина каждой секции — смесь соседних остановок, а не одна из них.
        let mid = heat_color(0.1);
        assert!(mid != HEATMAP_STOPS[0] && mid != HEATMAP_STOPS[1]);
        assert!(f32::from(mid.b()) < f32::from(HEATMAP_STOPS[0].b()));
    }

    #[test]
    fn mix_colors_interpolates_every_channel() {
        let a = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 100);
        let b = egui::Color32::from_rgba_unmultiplied(200, 100, 50, 200);
        assert_eq!(mix_colors(a, b, 0.0), a);
        assert_eq!(mix_colors(a, b, 1.0), b);
        assert_eq!(
            mix_colors(a, b, 0.5),
            egui::Color32::from_rgba_unmultiplied(100, 50, 25, 150)
        );
    }
}
