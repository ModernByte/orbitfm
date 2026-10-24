//! Векторные иконки, не зависящие от шрифтов egui.

use eframe::egui::{self, Color32, Pos2, Response, Sense, Shape, Stroke, Vec2, pos2};

use super::widgets::focus_ring;

fn icon_button(
    ui: &mut egui::Ui,
    size: Vec2,
    enabled: bool,
    hint: &str,
    paint: impl FnOnce(&egui::Painter, egui::Rect, Color32),
) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    if enabled && response.hovered() {
        let visuals = ui.style().interact(&response);
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(6), visuals.bg_fill);
    }
    let color = if enabled {
        ui.visuals().text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    paint(ui.painter(), rect, color);
    focus_ring(ui, &response, rect, 6.0);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, hint));
    if enabled {
        response.on_hover_text(hint)
    } else {
        response
    }
}

pub(crate) fn star_button(ui: &mut egui::Ui, filled: bool, color: Color32, hint: &str) -> Response {
    icon_button(
        ui,
        Vec2::splat(20.0),
        true,
        hint,
        move |painter, rect, _| {
            let points = star_points(rect.center(), 7.0, 3.0);
            if filled {
                painter.add(filled_polygon(&points, color));
            } else {
                painter.add(Shape::closed_line(points, Stroke::new(1.2, color)));
            }
        },
    )
}

pub(crate) fn stop_button(ui: &mut egui::Ui, enabled: bool) -> Response {
    icon_button(
        ui,
        Vec2::new(28.0, 24.0),
        enabled,
        "Stop",
        |painter, rect, color| {
            painter.rect_filled(
                egui::Rect::from_center_size(rect.center(), Vec2::splat(9.0)),
                egui::CornerRadius::same(1),
                color,
            );
        },
    )
}

fn star_points(center: Pos2, outer: f32, inner: f32) -> Vec<Pos2> {
    (0..10)
        .map(|index| {
            let radius = if index % 2 == 0 { outer } else { inner };
            let angle = -std::f32::consts::FRAC_PI_2 + index as f32 * std::f32::consts::PI / 5.0;
            pos2(
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin(),
            )
        })
        .collect()
}

/// Поза мыши для карточки подсказок (эскиз, левая панель).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MousePose {
    /// Вращение — ПКМ drag.
    Orbit,
    /// Одиночный клик ЛКМ.
    Click,
    /// Колесо.
    Wheel,
}

pub(crate) fn gear_button(ui: &mut egui::Ui, hint: &str) -> Response {
    icon_button(ui, Vec2::splat(24.0), true, hint, |painter, rect, color| {
        draw_gear(painter, rect.center(), 8.0, color);
    })
}

pub(crate) fn moon_button(ui: &mut egui::Ui, hint: &str) -> Response {
    icon_button(ui, Vec2::splat(24.0), true, hint, |painter, rect, color| {
        draw_moon(painter, rect.center(), 8.0, color);
    })
}

pub(crate) fn heart_button(
    ui: &mut egui::Ui,
    filled: bool,
    color: Color32,
    hint: &str,
) -> Response {
    icon_button(
        ui,
        Vec2::splat(22.0),
        true,
        hint,
        move |painter, rect, _| {
            draw_heart(painter, rect.center(), 8.0, color, filled);
        },
    )
}

pub(crate) fn prev_button(ui: &mut egui::Ui, enabled: bool) -> Response {
    icon_button(
        ui,
        Vec2::new(26.0, 24.0),
        enabled,
        "Previous station",
        |painter, rect, color| {
            draw_skip(painter, rect.center(), 7.0, color, true);
        },
    )
}

pub(crate) fn next_button(ui: &mut egui::Ui, enabled: bool) -> Response {
    icon_button(
        ui,
        Vec2::new(26.0, 24.0),
        enabled,
        "Next station",
        |painter, rect, color| {
            draw_skip(painter, rect.center(), 7.0, color, false);
        },
    )
}

/// Кнопка раскрытия/сворачивания панели списка (иконка «список»).
pub(crate) fn panel_button(ui: &mut egui::Ui, hint: &str) -> Response {
    icon_button(ui, Vec2::splat(24.0), true, hint, |painter, rect, color| {
        draw_list_icon(painter, rect, color);
    })
}

/// Кнопка скрытия окна (×). По смыслу не выход: процесс и поток остаются живыми.
pub(crate) fn close_button(ui: &mut egui::Ui, hint: &str) -> Response {
    icon_button(ui, Vec2::splat(24.0), true, hint, |painter, rect, color| {
        let arm = rect.width() * 0.22;
        let center = rect.center();
        let stroke = Stroke::new(1.7, color);
        painter.line_segment(
            [center - Vec2::splat(arm), center + Vec2::splat(arm)],
            stroke,
        );
        painter.line_segment(
            [
                pos2(center.x + arm, center.y - arm),
                pos2(center.x - arm, center.y + arm),
            ],
            stroke,
        );
    })
}

/// Возврат из капсулы в полный Focus: четыре угла, разведённые наружу.
pub(crate) fn expand_button(ui: &mut egui::Ui, hint: &str) -> Response {
    icon_button(ui, Vec2::splat(22.0), true, hint, |painter, rect, color| {
        let center = rect.center();
        let arm = 4.5;
        let stroke = Stroke::new(1.4, color);
        for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            let corner = center + Vec2::new(dx * arm, dy * arm);
            painter.line_segment([corner, corner - Vec2::new(dx * 3.0, 0.0)], stroke);
            painter.line_segment([corner, corner - Vec2::new(0.0, dy * 3.0)], stroke);
        }
    })
}

/// Кнопка карточки управления (знак «?»).
pub(crate) fn help_button(ui: &mut egui::Ui, hint: &str, active: bool) -> Response {
    icon_button(
        ui,
        Vec2::splat(24.0),
        true,
        hint,
        move |painter, rect, color| {
            if active {
                painter.rect_filled(
                    rect,
                    egui::CornerRadius::same(6),
                    color.gamma_multiply(0.18),
                );
            }
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "?",
                egui::FontId::proportional(14.0),
                color,
            );
        },
    )
}

pub(crate) fn draw_search(painter: &egui::Painter, center: Pos2, radius: f32, color: Color32) {
    let circle = center - Vec2::splat(radius * 0.3);
    painter.circle_stroke(circle, radius, Stroke::new(1.6, color));
    let offset = radius * 0.72;
    painter.line_segment(
        [
            circle + Vec2::new(offset, offset),
            center + Vec2::splat(radius * 0.95),
        ],
        Stroke::new(1.6, color),
    );
}

pub(crate) fn draw_speaker(painter: &egui::Painter, center: Pos2, size: f32, color: Color32) {
    let s = size / 8.0;
    painter.add(Shape::convex_polygon(
        vec![
            pos2(center.x - 3.0 * s, center.y - 1.5 * s),
            pos2(center.x - 1.2 * s, center.y - 1.5 * s),
            pos2(center.x + 1.0 * s, center.y - 3.4 * s),
            pos2(center.x + 1.0 * s, center.y + 3.4 * s),
            pos2(center.x - 1.2 * s, center.y + 1.5 * s),
            pos2(center.x - 3.0 * s, center.y + 1.5 * s),
        ],
        color,
        Stroke::NONE,
    ));
    painter.add(Shape::line(
        arc_points(
            Pos2::new(center.x + 1.0 * s, center.y),
            2.6 * s,
            -0.62,
            0.62,
        ),
        Stroke::new(1.4, color),
    ));
}

/// Точки дуги окружности от `from` до `to` (рад), ~2 точки на 10°.
pub(crate) fn arc_points(center: Pos2, radius: f32, from: f32, to: f32) -> Vec<Pos2> {
    let steps = (((to - from).abs() / 0.35).ceil() as usize).max(2);
    (0..=steps)
        .map(|step| {
            let angle = from + (to - from) * step as f32 / steps as f32;
            pos2(
                center.x + angle.cos() * radius,
                center.y + angle.sin() * radius,
            )
        })
        .collect()
}

pub(crate) fn draw_gear(painter: &egui::Painter, center: Pos2, radius: f32, color: Color32) {
    for tooth in 0..8 {
        let angle = tooth as f32 * std::f32::consts::FRAC_PI_4;
        let dir = Vec2::new(angle.cos(), angle.sin());
        let from = center + dir * (radius * 0.62);
        let to = center + dir * radius;
        painter.line_segment([from, to], Stroke::new(2.4, color));
    }
    painter.circle_stroke(center, radius * 0.52, Stroke::new(1.6, color));
}

pub(crate) fn draw_moon(painter: &egui::Painter, center: Pos2, radius: f32, color: Color32) {
    // Полумесяц — лента между внешней дугой и дугой «укуса»: обе дуги
    // непрерывны и имеют общие концы, поэтому strip-триангуляция корректна
    // для невыпуклой формы.
    let bite = center + Vec2::new(radius * 0.5, -radius * 0.3);
    let bite_radius = radius * 0.9;
    const SAMPLES: usize = 96;
    let mut outer: Vec<Pos2> = Vec::with_capacity(SAMPLES);
    let mut inner: Vec<Pos2> = Vec::with_capacity(SAMPLES);
    for sample in 0..SAMPLES {
        let angle = sample as f32 * std::f32::consts::TAU / SAMPLES as f32;
        let dir = Vec2::new(angle.cos(), angle.sin());
        let point = center + dir * radius;
        if (point - bite).length_sq() > bite_radius * bite_radius {
            outer.push(point);
        }
        let point = bite + dir * bite_radius;
        if (point - center).length_sq() < radius * radius {
            inner.push(point);
        }
    }
    if outer.is_empty() || inner.is_empty() {
        painter.circle_stroke(center, radius, Stroke::new(1.5, color));
        return;
    }
    inner.reverse();
    let rows = outer.len().min(inner.len());
    let mut mesh = egui::Mesh::default();
    for row in 0..rows {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: outer[row],
            uv: pos2(0.0, 0.0),
            color,
        });
        mesh.vertices.push(egui::epaint::Vertex {
            pos: inner[row.min(inner.len() - 1)],
            uv: pos2(0.0, 0.0),
            color,
        });
    }
    for row in 0..rows - 1 {
        let base = (row * 2) as u32;
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base + 1, base + 3, base + 2);
    }
    painter.add(Shape::from(mesh));
}

pub(crate) fn draw_dice(painter: &egui::Painter, center: Pos2, size: f32, color: Color32) {
    let rect = egui::Rect::from_center_size(center, Vec2::splat(size * 2.0));
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(3),
        Stroke::new(1.5, color),
        egui::StrokeKind::Middle,
    );
    let dot = size * 0.55;
    for (dx, dy) in [(-1.0, -1.0), (0.0, 0.0), (1.0, 1.0)] {
        painter.circle_filled(center + Vec2::new(dx * dot, dy * dot), size * 0.16, color);
    }
}

pub(crate) fn draw_heart(
    painter: &egui::Painter,
    center: Pos2,
    size: f32,
    color: Color32,
    filled: bool,
) {
    let s = size / 8.0;
    let lobe = 3.8 * s;
    let left = Pos2::new(center.x - 3.8 * s, center.y - 3.4 * s);
    let right = Pos2::new(center.x + 3.8 * s, center.y - 3.4 * s);
    let bottom = pos2(center.x, center.y + 6.2 * s);
    if filled {
        // Сердце невыпукло: две доли + нижний треугольник.
        painter.circle_filled(left, lobe, color);
        painter.circle_filled(right, lobe, color);
        painter.add(Shape::convex_polygon(
            vec![
                pos2(left.x - lobe * 0.72, left.y + lobe * 0.55),
                pos2(right.x + lobe * 0.72, right.y + lobe * 0.55),
                bottom,
            ],
            color,
            Stroke::NONE,
        ));
    } else {
        painter.add(Shape::closed_line(
            vec![
                bottom,
                pos2(left.x - lobe * 0.72, left.y + lobe * 0.55),
                left + Vec2::new(-lobe * 0.28, -lobe * 0.55),
                pos2(center.x, center.y - 2.2 * s),
                right + Vec2::new(lobe * 0.28, -lobe * 0.55),
                pos2(right.x + lobe * 0.72, right.y + lobe * 0.55),
            ],
            Stroke::new(1.5, color),
        ));
    }
}

pub(crate) fn draw_skip(
    painter: &egui::Painter,
    center: Pos2,
    size: f32,
    color: Color32,
    backward: bool,
) {
    let mirror = if backward { -1.0 } else { 1.0 };
    let tip = center + Vec2::new(mirror * size * 0.8, 0.0);
    let base_x = center - Vec2::new(mirror * size * 0.6, 0.0);
    painter.add(Shape::convex_polygon(
        vec![
            pos2(base_x.x, base_x.y - size * 0.7),
            pos2(base_x.x, base_x.y + size * 0.7),
            pos2(tip.x, tip.y),
        ],
        color,
        Stroke::NONE,
    ));
    let bar_x = center.x - mirror * size;
    painter.rect_filled(
        egui::Rect::from_center_size(
            Pos2::new(bar_x, center.y),
            Vec2::new(size * 0.28, size * 1.4),
        ),
        1.0,
        color,
    );
}

pub(crate) fn draw_clock(painter: &egui::Painter, center: Pos2, radius: f32, color: Color32) {
    painter.circle_stroke(center, radius, Stroke::new(1.5, color));
    painter.line_segment(
        [center, center + Vec2::new(0.0, -radius * 0.6)],
        Stroke::new(1.5, color),
    );
    painter.line_segment(
        [center, center + Vec2::new(radius * 0.5, 0.0)],
        Stroke::new(1.5, color),
    );
}

pub(crate) fn draw_list_icon(painter: &egui::Painter, rect: egui::Rect, color: Color32) {
    let rows = 3;
    for row in 0..rows {
        let y = rect.top() + (row as f32 + 0.5) * rect.height() / rows as f32;
        painter.circle_filled(pos2(rect.left() + 2.0, y), 1.4, color);
        painter.line_segment(
            [pos2(rect.left() + 6.0, y), pos2(rect.right(), y)],
            Stroke::new(1.5, color),
        );
    }
}

pub(crate) fn draw_mouse(
    painter: &egui::Painter,
    center: Pos2,
    size: f32,
    pose: MousePose,
    color: Color32,
) {
    let body = egui::Rect::from_center_size(
        center + Vec2::new(0.0, size * 0.1),
        Vec2::new(size * 0.85, size * 1.25),
    );
    painter.rect_stroke(
        body,
        egui::CornerRadius::same(6),
        Stroke::new(1.5, color),
        egui::StrokeKind::Middle,
    );
    match pose {
        MousePose::Orbit => {
            painter.line_segment(
                [
                    pos2(body.center().x, body.top() + size * 0.15),
                    pos2(body.center().x, body.top() + size * 0.5),
                ],
                Stroke::new(1.5, color),
            );
            let arrow_r = size * 0.85;
            painter.add(Shape::line(
                arc_points(
                    center,
                    arrow_r,
                    std::f32::consts::TAU * 1.25,
                    std::f32::consts::TAU * 1.75,
                ),
                Stroke::new(1.3, color),
            ));
        }
        MousePose::Click => {
            let button = egui::Rect::from_min_max(
                pos2(body.left() + 1.5, body.top() + 1.5),
                pos2(body.center().x - 1.0, body.top() + size * 0.55),
            );
            painter.rect_filled(button, egui::CornerRadius::same(3), color);
        }
        MousePose::Wheel => {
            painter.rect_filled(
                egui::Rect::from_center_size(
                    pos2(body.center().x, body.top() + size * 0.32),
                    Vec2::new(size * 0.16, size * 0.34),
                ),
                egui::CornerRadius::same(3),
                color,
            );
        }
    }
}

fn filled_polygon(points: &[Pos2], color: Color32) -> Shape {
    let mut mesh = egui::Mesh::default();
    let center = points
        .iter()
        .fold(Pos2::ZERO, |acc, point| acc + point.to_vec2())
        / points.len() as f32;
    mesh.colored_vertex(center, color);
    for point in points {
        mesh.colored_vertex(*point, color);
    }
    for index in 0..points.len() {
        let next = (index + 1) % points.len();
        mesh.add_triangle(0, index as u32 + 1, next as u32 + 1);
    }
    Shape::from(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_has_ten_alternating_points() {
        let points = star_points(pos2(10.0, 10.0), 8.0, 3.0);
        assert_eq!(points.len(), 10);
        let outer = (points[0].to_vec2() - Vec2::new(10.0, 10.0)).length();
        let inner = (points[1].to_vec2() - Vec2::new(10.0, 10.0)).length();
        assert!((outer - 8.0).abs() < 0.01);
        assert!((inner - 3.0).abs() < 0.01);
    }

    #[test]
    fn filled_polygon_is_a_triangle_fan() {
        let points = star_points(pos2(10.0, 10.0), 8.0, 3.0);
        let Shape::Mesh(mesh) = filled_polygon(&points, Color32::WHITE) else {
            panic!("ожидался mesh");
        };
        assert_eq!(mesh.vertices.len(), 11);
        assert_eq!(mesh.indices.len(), 30);
    }
}
