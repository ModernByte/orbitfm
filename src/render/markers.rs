//! Экранные маркеры станций поверх глобуса.

use eframe::egui::{self, Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, Vec2};
use glam::Vec3;

const MARKER_COLOR: Color32 = Color32::from_rgb(108, 176, 255);
const ACTIVE_COLOR: Color32 = Color32::from_rgb(255, 208, 112);
const MARKER_MIN_RADIUS: f32 = 3.5;
const MARKER_WEIGHT_RADIUS: f32 = 3.5;
const MARKER_HIGHLIGHT_RADIUS: f32 = 2.0;
const MARKER_GLOW_RADIUS: f32 = 3.0;
const MARKER_GLOW_WEIGHT_RADIUS: f32 = 2.0;

/// Цвета маркеров, задаваемые темой интерфейса.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkerColors {
    pub marker: Color32,
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
    pub highlighted: bool,
    /// Число станций, совпавших по координатам.
    pub count: usize,
    /// Визуальный вес 0..1 (популярность и битрейт станции).
    pub weight: f32,
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

/// Нормализует визуальный вес маркера в `0..1`.
fn marker_weight(weight: f32) -> f32 {
    if weight.is_finite() {
        weight.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Радиус маркера: растёт с весом, активный маркер крупнее.
fn marker_radius(weight: f32, highlighted: bool) -> f32 {
    let highlight = if highlighted {
        MARKER_HIGHLIGHT_RADIUS
    } else {
        0.0
    };
    MARKER_MIN_RADIUS + MARKER_WEIGHT_RADIUS * marker_weight(weight) + highlight
}

/// Рисует маркеры и подпись наведённого маркера поверх области глобуса.
pub fn paint_markers(
    painter: &Painter,
    bounds: Rect,
    markers: &[MarkerVisual],
    hovered: Option<usize>,
    hovered_label: Option<&str>,
    colors: MarkerColors,
) {
    let glow_color = Color32::from_rgba_unmultiplied(
        colors.marker.r(),
        colors.marker.g(),
        colors.marker.b(),
        40,
    );
    let label_bg = Color32::from_rgba_unmultiplied(8, 11, 18, 235);

    for (index, marker) in markers.iter().enumerate() {
        let radius = marker_radius(marker.weight, marker.highlighted);
        let glow =
            radius + MARKER_GLOW_RADIUS + marker_weight(marker.weight) * MARKER_GLOW_WEIGHT_RADIUS;
        let color = if marker.highlighted {
            colors.active
        } else {
            colors.marker
        };

        painter.circle_filled(marker.pos, glow, glow_color);
        painter.circle_filled(marker.pos, radius, color);
        painter.circle_stroke(
            marker.pos,
            radius,
            Stroke::new(1.0, Color32::from_white_alpha(200)),
        );

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
    fn marker_radius_grows_with_weight() {
        let light = marker_radius(0.0, false);
        let heavy = marker_radius(1.0, false);
        let active = marker_radius(1.0, true);

        assert!((light - MARKER_MIN_RADIUS).abs() < 1e-5);
        assert!(heavy > light);
        assert!(active > heavy);
        assert!((marker_radius(0.5, false) - (light + heavy) / 2.0).abs() < 1e-5);
    }

    #[test]
    fn marker_radius_clamps_invalid_weight() {
        assert!((marker_radius(-3.0, false) - MARKER_MIN_RADIUS).abs() < 1e-5);
        assert!(
            (marker_radius(f32::NAN, false) - MARKER_MIN_RADIUS).abs() < 1e-5,
            "NaN не должен раздувать маркер"
        );
        assert_eq!(
            marker_radius(9.0, false),
            marker_radius(1.0, false),
            "вес ограничен сверху"
        );
    }
}
