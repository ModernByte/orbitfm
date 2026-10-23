//! Контуры и заливка стран поверх глобуса.

use eframe::egui::{Color32, Mesh, Painter, Pos2, Rect, Shape, Stroke};
use glam::Vec3;

use super::{GlobeCamera, sphere_point};

/// Максимальный шаг между точками контура при проецировании (градусы дуги).
const MAX_ARC_STEP_DEGREES: f32 = 2.0;
/// Минимальная площадь проецированного кольца для заливки, px².
const MIN_FILL_AREA: f32 = 1.0;

/// Стиль отрисовки страны.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CountryStyle {
    /// Полупрозрачная заливка; `None` — только контур.
    pub fill: Option<Color32>,
    /// Линия контура, включая толщину.
    pub stroke: Stroke,
}

/// Рисует кольца страны (`[lon, lat]`, градусы) поверх области глобуса.
///
/// Заливка рисуется только для колец, целиком видимых на передней полусфере
/// и попадающих в viewport; контур обрезается по горизонту.
pub fn paint_country(
    painter: &Painter,
    viewport: Rect,
    camera: &GlobeCamera,
    rings: &[Vec<[f64; 2]>],
    style: CountryStyle,
) {
    if viewport.width() <= 0.0 || viewport.height() <= 0.0 {
        return;
    }

    for ring in rings {
        let points = sphere_ring(ring);
        if points.len() < 3 {
            continue;
        }
        paint_ring(painter, viewport, camera, &points, style);
    }
}

fn paint_ring(
    painter: &Painter,
    viewport: Rect,
    camera: &GlobeCamera,
    points: &[Vec3],
    style: CountryStyle,
) {
    if let Some(fill) = style.fill
        && let Some(polygon) = project_fill(camera, viewport, points)
    {
        let triangles = triangulate(&polygon);
        if !triangles.is_empty() {
            let mut mesh = Mesh::default();
            for point in &polygon {
                mesh.colored_vertex(*point, fill);
            }
            for [a, b, c] in triangles {
                mesh.add_triangle(a, b, c);
            }
            painter.add(Shape::mesh(mesh));
        }
    }

    let (runs, _) = visible_runs(points, camera.eye());
    for polyline in project_runs(camera, viewport, &runs) {
        painter.add(Shape::line(polyline, style.stroke));
    }
}

fn sphere_ring(ring: &[[f64; 2]]) -> Vec<Vec3> {
    if ring
        .iter()
        .any(|[lon, lat]| !lon.is_finite() || !lat.is_finite())
    {
        return Vec::new();
    }

    let points: Vec<Vec3> = ring
        .iter()
        .map(|[lon, lat]| sphere_point(*lat as f32, *lon as f32))
        .collect();
    if points.len() < 3 {
        return points;
    }

    let mut dense = Vec::with_capacity(points.len() * 2);
    for (index, start) in points.iter().enumerate() {
        let end = points[(index + 1) % points.len()];
        dense.push(*start);

        let angle = start.dot(end).clamp(-1.0, 1.0).acos();
        let steps = (angle.to_degrees() / MAX_ARC_STEP_DEGREES).ceil() as usize;
        for step in 1..steps {
            let t = step as f32 / steps as f32;
            dense.push(slerp(*start, end, t, angle));
        }
    }
    dense
}

fn slerp(start: Vec3, end: Vec3, t: f32, angle: f32) -> Vec3 {
    if angle < 1e-4 {
        return start.lerp(end, t).normalize_or_zero();
    }
    let sin = angle.sin();
    (start * ((1.0 - t) * angle).sin() / sin + end * (t * angle).sin() / sin).normalize_or_zero()
}

fn project_fill(camera: &GlobeCamera, viewport: Rect, points: &[Vec3]) -> Option<Vec<Pos2>> {
    let eye = camera.eye();
    let mut projected = Vec::with_capacity(points.len());
    for point in points {
        if point.dot(eye) <= 1.0 {
            return None;
        }
        projected.push(camera.project_on_sphere(*point, viewport)?);
    }
    (projected.len() >= 3).then_some(projected)
}

fn visible(point: Vec3, eye: Vec3) -> bool {
    point.dot(eye) >= 1.0
}

fn horizon_crossing(start: Vec3, end: Vec3, eye: Vec3) -> Vec3 {
    let denominator = (end - start).dot(eye);
    if denominator.abs() < f32::EPSILON {
        return start;
    }
    let t = ((1.0 - start.dot(eye)) / denominator).clamp(0.0, 1.0);
    start.lerp(end, t).normalize_or_zero()
}

/// Разбивает замкнутое кольцо на видимые дуги, обрезая их по горизонту.
fn visible_runs(points: &[Vec3], eye: Vec3) -> (Vec<Vec<Vec3>>, bool) {
    let count = points.len();
    let mut runs: Vec<Vec<Vec3>> = Vec::new();
    let mut current: Vec<Vec3> = Vec::new();
    let mut clipped = false;

    for (index, start) in points.iter().enumerate() {
        let end = points[(index + 1) % count];
        let start_visible = visible(*start, eye);
        let end_visible = visible(end, eye);

        if start_visible {
            push_point(&mut current, *start);
        }

        match (start_visible, end_visible) {
            (true, false) => {
                push_point(&mut current, horizon_crossing(*start, end, eye));
                if current.len() >= 2 {
                    runs.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
                clipped = true;
            }
            (false, true) => {
                if current.is_empty() {
                    push_point(&mut current, horizon_crossing(*start, end, eye));
                }
                push_point(&mut current, end);
                clipped = true;
            }
            (false, false) => {
                if !current.is_empty() {
                    runs.push(std::mem::take(&mut current));
                }
            }
            (true, true) => {}
        }
    }

    if !current.is_empty() {
        if !clipped && current.len() >= 2 {
            let first = current[0];
            push_point(&mut current, first);
        }
        runs.push(current);
    }

    (runs, clipped)
}

fn push_point(points: &mut Vec<Vec3>, point: Vec3) {
    if points.last().copied() != Some(point) {
        points.push(point);
    }
}

fn project_runs(camera: &GlobeCamera, viewport: Rect, runs: &[Vec<Vec3>]) -> Vec<Vec<Pos2>> {
    let mut polylines = Vec::new();
    for run in runs {
        let mut line = Vec::new();
        for point in run {
            match camera.project_on_sphere(*point, viewport) {
                Some(position) => line.push(position),
                None => {
                    if line.len() >= 2 {
                        polylines.push(std::mem::take(&mut line));
                    } else {
                        line.clear();
                    }
                }
            }
        }
        if line.len() >= 2 {
            polylines.push(line);
        }
    }
    polylines
}

fn triangulate(points: &[Pos2]) -> Vec<[u32; 3]> {
    let count = points.len();
    if count < 3 {
        return Vec::new();
    }

    let area = signed_area(points);
    if area.abs() < MIN_FILL_AREA {
        return Vec::new();
    }
    let orientation = area.signum();

    let mut indices: Vec<usize> = (0..count).collect();
    let mut triangles = Vec::with_capacity(count - 2);

    while indices.len() > 3 {
        let length = indices.len();
        let mut ear = None;
        for position in 0..length {
            let previous = indices[(position + length - 1) % length];
            let current = indices[position];
            let next = indices[(position + 1) % length];

            if cross(points[previous], points[current], points[next]) * orientation <= 0.0 {
                continue;
            }
            let contains = indices.iter().any(|&index| {
                index != previous
                    && index != current
                    && index != next
                    && point_in_triangle(
                        points[index],
                        points[previous],
                        points[current],
                        points[next],
                    )
            });
            if !contains {
                ear = Some((position, [previous as u32, current as u32, next as u32]));
                break;
            }
        }

        let Some((position, triangle)) = ear else {
            break;
        };
        triangles.push(triangle);
        indices.remove(position);
    }

    if indices.len() == 3 {
        triangles.push([indices[0] as u32, indices[1] as u32, indices[2] as u32]);
    }
    triangles
}

fn signed_area(points: &[Pos2]) -> f32 {
    let mut sum = 0.0;
    for (index, point) in points.iter().enumerate() {
        let next = points[(index + 1) % points.len()];
        sum += point.x * next.y - next.x * point.y;
    }
    sum / 2.0
}

fn cross(a: Pos2, b: Pos2, c: Pos2) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn point_in_triangle(point: Pos2, a: Pos2, b: Pos2, c: Pos2) -> bool {
    let ab = cross(a, b, point);
    let bc = cross(b, c, point);
    let ca = cross(c, a, point);
    (ab >= 0.0 && bc >= 0.0 && ca >= 0.0) || (ab <= 0.0 && bc <= 0.0 && ca <= 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Vec2;

    fn viewport() -> Rect {
        Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))
    }

    fn small_ring(lat: f64, lon: f64) -> Vec<[f64; 2]> {
        vec![
            [lon - 4.0, lat - 3.0],
            [lon + 4.0, lat - 3.0],
            [lon + 4.0, lat + 3.0],
            [lon - 4.0, lat + 3.0],
        ]
    }

    #[test]
    fn triangulates_simple_polygon() {
        let points = vec![
            Pos2::new(0.0, 0.0),
            Pos2::new(10.0, 0.0),
            Pos2::new(10.0, 10.0),
            Pos2::new(0.0, 10.0),
        ];
        let triangles = triangulate(&points);
        assert_eq!(triangles.len(), 2);

        let total: f32 = triangles
            .iter()
            .map(|[a, b, c]| {
                cross(
                    points[*a as usize],
                    points[*b as usize],
                    points[*c as usize],
                )
                .abs()
                    / 2.0
            })
            .sum();
        assert!((total - 100.0).abs() < 1e-3, "площадь: {total}");
    }

    #[test]
    fn triangulates_concave_polygon() {
        let points = vec![
            Pos2::new(0.0, 0.0),
            Pos2::new(10.0, 0.0),
            Pos2::new(10.0, 10.0),
            Pos2::new(5.0, 4.0),
            Pos2::new(0.0, 10.0),
        ];
        let triangles = triangulate(&points);
        assert_eq!(triangles.len(), 3);
    }

    #[test]
    fn fill_requires_fully_visible_ring() {
        let camera = GlobeCamera::default();
        assert!(
            project_fill(&camera, viewport(), &sphere_ring(&small_ring(0.0, 0.0))).is_some(),
            "кольцо перед камерой должно заливаться"
        );
        assert!(
            project_fill(&camera, viewport(), &sphere_ring(&small_ring(0.0, 180.0))).is_none(),
            "кольцо на обратной стороне не заливается"
        );
    }

    #[test]
    fn outline_is_split_at_horizon() {
        let camera = GlobeCamera::default();
        let eye = camera.eye();
        let ring = sphere_ring(&[[-40.0, 0.0], [0.0, 0.0], [40.0, 0.0], [140.0, 0.0]]);
        let (runs, clipped) = visible_runs(&ring, eye);

        assert!(clipped, "кольцо должно пересекать горизонт");
        assert!(!runs.is_empty());
        assert!(
            runs.iter().flatten().all(|point| visible(*point, eye)),
            "в видимых дугах не должно быть точек задней полусферы"
        );
    }

    #[test]
    fn fully_visible_outline_is_closed() {
        let camera = GlobeCamera::default();
        let ring = sphere_ring(&small_ring(0.0, 0.0));
        let (runs, clipped) = visible_runs(&ring, camera.eye());

        assert!(!clipped);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].first(), runs[0].last(), "контур должен замыкаться");
    }

    #[test]
    fn invalid_coordinates_are_skipped() {
        let camera = GlobeCamera::default();
        let ring = vec![[f64::NAN, 0.0], [0.0, 0.0], [1.0, 1.0], [2.0, 0.0]];
        assert!(sphere_ring(&ring).is_empty());
        assert!(
            project_fill(&camera, viewport(), &sphere_ring(&ring)).is_none(),
            "невалидное кольцо не должно заливаться"
        );
    }
}
