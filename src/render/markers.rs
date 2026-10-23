//! Экранные маркеры станций поверх глобуса.

use eframe::egui::{self, Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, Vec2};
use glam::Vec3;

const MARKER_COLOR: Color32 = Color32::from_rgb(108, 176, 255);
const ACTIVE_COLOR: Color32 = Color32::from_rgb(255, 208, 112);
/// Радиус самой тихой станции (ТЗ §2.2, уровень XS).
const MARKER_MIN_RADIUS: f32 = 3.5;
/// Радиус станции на насыщении популярности (уровень L/XL).
const MARKER_MAX_RADIUS: f32 = 13.0;
/// Играющая станция не мельче XL, независимо от голосов.
const PLAYING_MIN_RADIUS: f32 = 11.0;
const MARKER_GLOW_RADIUS: f32 = 3.0;
const MARKER_GLOW_WEIGHT_RADIUS: f32 = 2.0;
/// Голоса, после которых маркер перестаёт расти.
const POPULARITY_SATURATION: f32 = 5_000.0;
const PULSE_PERIOD_SECONDS: f32 = 1.4;
const PULSE_AMPLITUDE: f32 = 0.15;
const PULSE_RING_SPAN: f32 = 3.0;
const HOVER_BRIGHTNESS: f32 = 1.3;
const MARKER_RING_WIDTH: f32 = 2.0;

/// Цвета маркеров, задаваемые темой интерфейса.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkerColors {
    /// Обычный маркер — он же accent, когда битрейт станции неизвестен.
    pub marker: Color32,
    /// Маркер станции в эфире.
    pub active: Color32,
}

impl Default for MarkerColors {
    fn default() -> Self {
        Self {
            marker: MARKER_COLOR,
            active: ACTIVE_COLOR,
        }
    }
}

/// Готовый к отрисовке маркер в экранных координатах.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkerVisual {
    pub pos: Pos2,
    /// Станция сейчас в эфире: отдельный цвет, кольцо и пульс.
    pub playing: bool,
    /// Число станций, совпавших по координатам.
    pub count: usize,
    /// Сигнал популярности (голоса): только размер, на цвет не влияет.
    pub popularity: f32,
    /// Битрейт в kbps; 0 означает «неизвестен».
    pub bitrate: u32,
}

/// Точка единичной сферы по географическим координатам.
///
/// Соглашение: `x = cos φ · sin λ`, `y = sin φ`, `z = cos φ · cos λ`;
/// `+Z` — Гринвич, `+Y` — север.
pub fn sphere_point(lat_deg: f32, lon_deg: f32) -> Vec3 {
    let lat = lat_deg.to_radians();
    let lon = lon_deg.to_radians();
    Vec3::new(lat.cos() * lon.sin(), lat.sin(), lat.cos() * lon.cos())
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Популярность в `0..1`: log-насыщение голосов и плавная рампа.
fn popularity_score(popularity: f32) -> f32 {
    if popularity.is_nan() || popularity <= 0.0 {
        return 0.0;
    }
    let t = (1.0 + popularity).ln() / (1.0 + POPULARITY_SATURATION).ln();
    smoothstep(t.clamp(0.0, 1.0))
}

/// Радиус маркера: растёт с популярностью, играющая станция крупнее.
fn marker_radius(popularity: f32, playing: bool) -> f32 {
    let radius =
        MARKER_MIN_RADIUS + (MARKER_MAX_RADIUS - MARKER_MIN_RADIUS) * popularity_score(popularity);
    if playing {
        radius.max(PLAYING_MIN_RADIUS)
    } else {
        radius
    }
}

fn mix(from: Color32, to: Color32, t: f32) -> Color32 {
    let channel = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t) as u8;
    Color32::from_rgb(
        channel(from.r(), to.r()),
        channel(from.g(), to.g()),
        channel(from.b(), to.b()),
    )
}

/// Цвет по качеству потока (ТЗ §2.3); неизвестный битрейт — accent.
fn bitrate_color(bitrate: u32, accent: Color32) -> Color32 {
    let (from, to, low, span) = match bitrate {
        0 => return accent,
        1..=63 => return Color32::from_rgb(0x6B, 0x72, 0x80),
        64..=127 => (
            Color32::from_rgb(0x3B, 0x82, 0xF6),
            Color32::from_rgb(0x22, 0xD3, 0xEE),
            64,
            63,
        ),
        128..=255 => (
            Color32::from_rgb(0x34, 0xD3, 0x99),
            Color32::from_rgb(0xA3, 0xE6, 0x35),
            128,
            127,
        ),
        _ => (
            Color32::from_rgb(0xFB, 0xBF, 0x24),
            Color32::from_rgb(0xF9, 0x73, 0x16),
            256,
            64,
        ),
    };
    let t = ((bitrate - low) as f32 / span as f32).clamp(0.0, 1.0);
    mix(from, to, t)
}

fn brighten(color: Color32, factor: f32) -> Color32 {
    let channel = |c: u8| (f32::from(c) * factor).round().min(255.0) as u8;
    Color32::from_rgb(channel(color.r()), channel(color.g()), channel(color.b()))
}

/// Масштаб пульса играющего маркера: `1.0 ↔ 1.0 + PULSE_AMPLITUDE`.
fn pulse_scale(time: f64) -> f32 {
    if !time.is_finite() {
        return 1.0;
    }
    let phase = time as f32 * (std::f32::consts::TAU / PULSE_PERIOD_SECONDS);
    1.0 + PULSE_AMPLITUDE * (1.0 - phase.cos()) * 0.5
}

/// Рисует маркеры и подпись наведённого маркера поверх области глобуса.
pub fn paint_markers(
    painter: &Painter,
    bounds: Rect,
    markers: &[MarkerVisual],
    hovered: Option<usize>,
    hovered_label: Option<&str>,
    colors: MarkerColors,
    time: f64,
) {
    let label_bg = Color32::from_rgba_unmultiplied(8, 11, 18, 235);

    for (index, marker) in markers.iter().enumerate() {
        let score = popularity_score(marker.popularity);
        let pulse = if marker.playing {
            pulse_scale(time)
        } else {
            1.0
        };
        let radius = marker_radius(marker.popularity, marker.playing) * pulse;
        let glow = radius + MARKER_GLOW_RADIUS + score * MARKER_GLOW_WEIGHT_RADIUS;
        let base = if marker.playing {
            colors.active
        } else {
            bitrate_color(marker.bitrate, colors.marker)
        };
        let color = if hovered == Some(index) {
            brighten(base, HOVER_BRIGHTNESS)
        } else {
            base
        };
        let glow_color = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 40);

        painter.circle_filled(marker.pos, glow, glow_color);
        painter.circle_filled(marker.pos, radius, color);
        painter.circle_stroke(
            marker.pos,
            radius,
            Stroke::new(1.0, Color32::from_white_alpha(200)),
        );

        if marker.playing {
            let ring =
                radius + MARKER_RING_WIDTH + PULSE_RING_SPAN * (pulse - 1.0) / PULSE_AMPLITUDE;
            painter.circle_stroke(
                marker.pos,
                ring,
                Stroke::new(MARKER_RING_WIDTH, colors.active),
            );
        }

        if marker.count > 1 {
            let badge = marker.pos + Vec2::new(radius + 2.0, -(radius + 2.0));
            painter.circle_filled(badge, 6.5, Color32::from_rgb(18, 23, 33));
            painter.circle_stroke(badge, 6.5, Stroke::new(1.0, glow_color));
            painter.text(
                badge,
                Align2::CENTER_CENTER,
                marker.count.to_string(),
                FontId::proportional(9.0),
                Color32::WHITE,
            );
        }

        if hovered == Some(index) {
            painter.circle_stroke(marker.pos, radius + 4.0, Stroke::new(1.5, Color32::WHITE));
        }
    }

    if let Some(label) = hovered_label
        && let Some(marker) = hovered.and_then(|index| markers.get(index))
    {
        paint_label(painter, bounds, marker.pos, label, label_bg);
    }
}

fn paint_label(painter: &Painter, bounds: Rect, anchor: Pos2, label: &str, label_bg: Color32) {
    let galley =
        painter.layout_no_wrap(label.to_owned(), FontId::proportional(12.0), Color32::WHITE);
    let padding = Vec2::new(8.0, 5.0);
    let size = galley.size() + padding * 2.0;

    let mut min = Pos2::new(anchor.x + 12.0, anchor.y - size.y - 12.0);
    if min.x + size.x > bounds.right() - 6.0 {
        min.x = anchor.x - size.x - 12.0;
    }
    let max_x = (bounds.right() - size.x - 6.0).max(bounds.left() + 6.0);
    let max_y = (bounds.bottom() - size.y - 6.0).max(bounds.top() + 6.0);
    min.x = min.x.clamp(bounds.left() + 6.0, max_x);
    min.y = min.y.clamp(bounds.top() + 6.0, max_y);

    let rect = Rect::from_min_size(min, size);
    painter.rect_filled(rect, egui::CornerRadius::same(6), label_bg);
    painter.galley(min + padding, galley, Color32::WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sphere_point_uses_documented_convention() {
        let greenwich = sphere_point(0.0, 0.0);
        assert!((greenwich - Vec3::Z).length() < 1e-5);

        let north_pole = sphere_point(90.0, 0.0);
        assert!((north_pole - Vec3::Y).length() < 1e-5);

        let east = sphere_point(0.0, 90.0);
        assert!((east - Vec3::X).length() < 1e-5);
    }

    #[test]
    fn sphere_points_are_normalized() {
        for (lat, lon) in [
            (-89.9, -179.9),
            (-33.87, 151.21),
            (0.0, 0.0),
            (52.52, 13.405),
        ] {
            let point = sphere_point(lat, lon);
            assert!((point.length() - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn radius_grows_with_popularity_and_stays_in_tiers() {
        let xs = marker_radius(0.0, false);
        let s = marker_radius(10.0, false);
        let m = marker_radius(100.0, false);
        let l = marker_radius(1_000.0, false);
        let xl = marker_radius(POPULARITY_SATURATION, false);

        assert!((xs - MARKER_MIN_RADIUS).abs() < 1e-5, "пустой маркер = XS");
        assert!(
            s > xs && m > s && l > m && xl > l,
            "монотонность по голосам"
        );
        assert!(xl <= MARKER_MAX_RADIUS + 1e-5);
        assert_eq!(
            marker_radius(POPULARITY_SATURATION * 10.0, false),
            xl,
            "насыщение останавливает рост"
        );
    }

    #[test]
    fn playing_marker_is_at_least_xl() {
        let quiet_playing = marker_radius(0.0, true);
        assert!(
            (quiet_playing - PLAYING_MIN_RADIUS).abs() < 1e-5,
            "играна не мельче XL"
        );
        assert!(marker_radius(POPULARITY_SATURATION, true) > quiet_playing);
    }

    #[test]
    fn radius_survives_invalid_popularity() {
        assert!((marker_radius(-3.0, false) - MARKER_MIN_RADIUS).abs() < 1e-5);
        assert!(
            (marker_radius(f32::NAN, false) - MARKER_MIN_RADIUS).abs() < 1e-5,
            "NaN не должен раздувать маркер"
        );
        assert!(
            (marker_radius(f32::INFINITY, false) - MARKER_MAX_RADIUS).abs() < 1e-4,
            "бесконечность не раздувает маркер"
        );
    }

    #[test]
    fn bitrate_tiers_match_spec() {
        let accent = Color32::from_rgb(1, 2, 3);
        assert_eq!(bitrate_color(0, accent), accent, "неизвестный битрейт");
        assert_eq!(
            bitrate_color(32, accent),
            Color32::from_rgb(0x6B, 0x72, 0x80)
        );

        let low = bitrate_color(64, accent);
        let mid = bitrate_color(127, accent);
        assert_eq!(low, Color32::from_rgb(0x3B, 0x82, 0xF6));
        assert_eq!(mid, Color32::from_rgb(0x22, 0xD3, 0xEE), "конец тира");
        assert_ne!(low, mid, "внутри тира цвет меняется");

        assert_eq!(
            bitrate_color(128, accent),
            Color32::from_rgb(0x34, 0xD3, 0x99)
        );
        assert_eq!(
            bitrate_color(320, accent),
            Color32::from_rgb(0xF9, 0x73, 0x16)
        );

        let golden = bitrate_color(256, accent);
        assert_eq!(golden, Color32::from_rgb(0xFB, 0xBF, 0x24));
        assert_eq!(
            bitrate_color(10_000, accent),
            bitrate_color(320, accent),
            "за 320 кбит/с цвет не уезжает"
        );
    }

    #[test]
    fn size_follows_popularity_and_color_follows_bitrate() {
        let quiet_hi_bitrate = MarkerVisual {
            pos: Pos2::ZERO,
            playing: false,
            count: 1,
            popularity: 10.0,
            bitrate: 320,
        };
        let loud_low_bitrate = MarkerVisual {
            pos: Pos2::ZERO,
            playing: false,
            count: 1,
            popularity: 4_000.0,
            bitrate: 32,
        };

        assert!(
            marker_radius(loud_low_bitrate.popularity, false)
                > marker_radius(quiet_hi_bitrate.popularity, false),
            "размер задаёт популярность, а не качество потока"
        );
        assert_eq!(
            bitrate_color(quiet_hi_bitrate.bitrate, Color32::WHITE),
            Color32::from_rgb(0xF9, 0x73, 0x16)
        );
        assert_eq!(
            bitrate_color(loud_low_bitrate.bitrate, Color32::WHITE),
            Color32::from_rgb(0x6B, 0x72, 0x80)
        );
    }

    #[test]
    fn hover_brightens_without_clipping() {
        let base = Color32::from_rgb(100, 120, 140);
        let lifted = brighten(base, HOVER_BRIGHTNESS);
        assert!(lifted.r() > base.r() && lifted.g() > base.g() && lifted.b() > base.b());
        assert_eq!(
            brighten(Color32::from_rgb(255, 200, 100), HOVER_BRIGHTNESS).r(),
            255
        );
    }

    #[test]
    fn pulse_stays_between_one_and_one_five() {
        assert!(
            (pulse_scale(0.0) - 1.0).abs() < 1e-5,
            "старт без раздувания"
        );
        let half_period = PULSE_PERIOD_SECONDS as f64 / 2.0;
        assert!(
            (pulse_scale(half_period) - (1.0 + PULSE_AMPLITUDE)).abs() < 1e-4,
            "пик пульса"
        );
        for step in 0..64 {
            let scale = pulse_scale(step as f64 * 0.11);
            assert!(
                (1.0..=1.0 + PULSE_AMPLITUDE + 1e-5).contains(&scale),
                "пульс ограничен диапазоном 1.0↔1.15"
            );
        }
        assert_eq!(pulse_scale(f64::NAN), 1.0);
    }
}
