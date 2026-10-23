//! Векторные иконки, не зависящие от шрифтов egui.

use eframe::egui::{self, Color32, Pos2, Response, Sense, Shape, Stroke, Vec2, pos2};

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

pub(crate) fn play_pause_button(
    ui: &mut egui::Ui,
    playing: bool,
    enabled: bool,
    hint: &str,
) -> Response {
    icon_button(
        ui,
        Vec2::new(30.0, 24.0),
        enabled,
        hint,
        move |painter, rect, color| {
            let center = rect.center();
            if playing {
                let bar = Vec2::new(3.5, 11.0);
                painter.rect_filled(
                    egui::Rect::from_center_size(center - Vec2::new(4.0, 0.0), bar),
                    egui::CornerRadius::same(1),
                    color,
                );
                painter.rect_filled(
                    egui::Rect::from_center_size(center + Vec2::new(4.0, 0.0), bar),
                    egui::CornerRadius::same(1),
                    color,
                );
            } else {
                painter.add(Shape::convex_polygon(
                    vec![
                        pos2(center.x - 4.5, center.y - 6.5),
                        pos2(center.x - 4.5, center.y + 6.5),
                        pos2(center.x + 6.5, center.y),
                    ],
                    color,
                    Stroke::NONE,
                ));
            }
        },
    )
}

pub(crate) fn stop_button(ui: &mut egui::Ui, enabled: bool) -> Response {
    icon_button(
        ui,
        Vec2::new(28.0, 24.0),
        enabled,
        "Остановить",
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
