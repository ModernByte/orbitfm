//! Экранные маркеры станций поверх глобуса.
//!
//! Маркеры одного цвета и размера (ADR-0012, обновление Этапа 9): качество
//! потока и рейтинг станции показывает подпись при наведении, а не радиус.

use eframe::egui::{self, Color32, FontId, Painter, Pos2, Rect, Stroke, Vec2};
use glam::Vec3;

/// Радиус маркера одинаков для всех станций, px.
const MARKER_RADIUS: f32 = 3.0;
const MARKER_GLOW_RADIUS: f32 = 2.5;
const MARKER_GLOW_ALPHA: u8 = 34;
/// Кольцо наведения отходит от маркера на столько пикселей.
const HOVER_RING_GAP: f32 = 3.5;
const HOVER_RING_WIDTH: f32 = 1.5;
const PULSE_PERIOD_SECONDS: f32 = 1.4;
const PULSE_RING_SPAN: f32 = 3.5;
const PULSE_RING_WIDTH: f32 = 1.5;
const PULSE_RING_ALPHA: f32 = 150.0;
const LABEL_TITLE_COLOR: Color32 = Color32::WHITE;
const LABEL_DETAIL_COLOR: Color32 = Color32::from_rgb(199, 210, 228);

/// Подпись наведённого маркера: имя и вторая строка с местом, качеством и
/// рейтингом. Строки раскладываются разными шрифтами, поэтому это не одна
/// строка с `\n`.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkerLabel {
    pub title: String,
    pub details: Option<String>,
}

/// Готовый к отрисовке маркер в экранных координатах.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkerVisual {
    pub pos: Pos2,
    /// Станция сейчас в эфире: пульсирующее кольцо того же цвета.
    pub playing: bool,
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

/// Осветление к белому без изменения альфы.
fn lighten(color: Color32, amount: f32) -> Color32 {
    let amount = amount.clamp(0.0, 1.0);
    let channel = |c: u8| {
        (f32::from(c) + (255.0 - f32::from(c)) * amount)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Color32::from_rgb(channel(color.r()), channel(color.g()), channel(color.b()))
}

/// Фаза пульса 0..1: кольцо расходится от маркера и гаснет.
fn pulse_fraction(time: f64) -> f32 {
    if !time.is_finite() {
        return 0.0;
    }
    let phase = time as f32 * (std::f32::consts::TAU / PULSE_PERIOD_SECONDS);
    (1.0 - phase.cos()) * 0.5
}

/// Рисует маркеры и подпись наведённого маркера поверх области глобуса.
pub fn paint_markers(
    painter: &Painter,
    bounds: Rect,
    markers: &[MarkerVisual],
    hovered: Option<usize>,
    hovered_label: Option<&MarkerLabel>,
    color: Color32,
    time: f64,
) {
    for (index, marker) in markers.iter().enumerate() {
        let hovered_here = hovered == Some(index);
        let fill = if hovered_here {
            lighten(color, 0.28)
        } else {
            color
        };
        painter.circle_filled(
            marker.pos,
            MARKER_RADIUS + MARKER_GLOW_RADIUS,
            Color32::from_rgba_unmultiplied(fill.r(), fill.g(), fill.b(), MARKER_GLOW_ALPHA),
        );
        painter.circle_filled(marker.pos, MARKER_RADIUS, fill);
        painter.circle_stroke(
            marker.pos,
            MARKER_RADIUS,
            Stroke::new(1.0, Color32::from_white_alpha(190)),
        );

        if marker.playing {
            let phase = pulse_fraction(time);
            let radius = MARKER_RADIUS + 1.0 + PULSE_RING_SPAN * phase;
            let alpha = (PULSE_RING_ALPHA * (1.0 - phase)).round() as u8;
            painter.circle_stroke(
                marker.pos,
                radius,
                Stroke::new(
                    PULSE_RING_WIDTH,
                    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha),
                ),
            );
        }

        if hovered_here {
            painter.circle_stroke(
                marker.pos,
                MARKER_RADIUS + HOVER_RING_GAP,
                Stroke::new(HOVER_RING_WIDTH, Color32::from_white_alpha(220)),
            );
        }
    }

    if let Some(label) = hovered_label
        && let Some(marker) = hovered.and_then(|index| markers.get(index))
    {
        paint_label(painter, bounds, marker.pos, label, color);
    }
}

fn paint_label(
    painter: &Painter,
    bounds: Rect,
    anchor: Pos2,
    label: &MarkerLabel,
    accent: Color32,
) {
    let title = painter.layout_no_wrap(
        label.title.clone(),
        FontId::proportional(12.5),
        LABEL_TITLE_COLOR,
    );
    let details = label.details.as_ref().map(|details| {
        painter.layout_no_wrap(
            details.clone(),
            FontId::proportional(10.5),
            LABEL_DETAIL_COLOR,
        )
    });

    let padding = Vec2::new(9.0, 6.0);
    let gap = 2.0;
    let text_width = title
        .size()
        .x
        .max(details.as_ref().map_or(0.0, |g| g.size().x));
    let text_height = title.size().y + details.as_ref().map_or(0.0, |g| g.size().y + gap);
    let size = Vec2::new(text_width + padding.x * 2.0, text_height + padding.y * 2.0);

    let mut min = Pos2::new(anchor.x + 12.0, anchor.y - size.y - 12.0);
    if min.x + size.x > bounds.right() - 6.0 {
        min.x = anchor.x - size.x - 12.0;
    }
    let max_x = (bounds.right() - size.x - 6.0).max(bounds.left() + 6.0);
    let max_y = (bounds.bottom() - size.y - 6.0).max(bounds.top() + 6.0);
    min.x = min.x.clamp(bounds.left() + 6.0, max_x);
    min.y = min.y.clamp(bounds.top() + 6.0, max_y);

    let rect = Rect::from_min_size(min, size);
    painter.rect(
        rect,
        egui::CornerRadius::same(7),
        Color32::from_rgba_unmultiplied(8, 11, 18, 235),
        Stroke::new(
            1.0,
            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 70),
        ),
        egui::StrokeKind::Middle,
    );
    let title_height = title.size().y;
    painter.galley(rect.min + padding, title, Color32::WHITE);
    if let Some(details) = details {
        painter.galley(
            Pos2::new(
                rect.left() + padding.x,
                rect.top() + padding.y + title_height + gap,
            ),
            details,
            Color32::WHITE,
        );
    }
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
    fn hover_lightens_without_losing_alpha() {
        let base = Color32::from_rgb(100, 120, 140);
        let lifted = lighten(base, 0.3);
        assert!(lifted.r() > base.r() && lifted.g() > base.g() && lifted.b() > base.b());
        assert_eq!(lifted.a(), base.a());
        assert_eq!(lighten(Color32::WHITE, 0.5), Color32::WHITE);
        assert_eq!(lighten(base, 0.0), base);
    }

    #[test]
    fn pulse_ring_travels_and_fades() {
        assert!((pulse_fraction(0.0)).abs() < 1e-5, "старт кольца");
        let half_period = f64::from(PULSE_PERIOD_SECONDS) / 2.0;
        assert!(
            (pulse_fraction(half_period) - 1.0).abs() < 1e-4,
            "к середине периода кольцо ушло дальше всего"
        );
        for step in 0..64 {
            let phase = pulse_fraction(step as f64 * 0.11);
            assert!((0.0..=1.0).contains(&phase));
        }
        assert_eq!(pulse_fraction(f64::NAN), 0.0);
    }
}
