//! Процедурная иконка трея (ADR-0020, п. 6).
//!
//! Рисуем тот же орбитальный знак, что `ui::widgets::logo_mark` — диск планеты,
//! наклонённое кольцо и точка эфира, — но растеризуем сами: у модуля трея нет
//! зависимости от egui и от presentation-слоя. Полупрозрачных линий из 22 px
//! хватило бы только на векторном рендере, поэтому планета заливается целиком.
//!
//! Буферы строятся один раз (`LazyLock`): хост вызывает `icon_pixmap()` на
//! каждое событие трея, и пересчёт пикселей в этом пути — чистая потеря времени.

use std::sync::LazyLock;

use ksni::Icon;

/// Размерности, которые запрашивают типовые SNI-хосты.
const SIZES: [i32; 3] = [22, 32, 48];
/// Проб на пиксель по каждой оси: тонкие штрихи иначе рассыпаются.
const SUPERSAMPLE: i32 = 4;
/// Планета — акцентный синий OrbitFM.
const PLANET: [u8; 3] = [96, 165, 250];
/// Орбитальное кольцо.
const RING: [u8; 3] = [190, 215, 255];
/// Точка эфира (LIVE).
const LIVE: [u8; 3] = [52, 211, 153];

/// Иконки во всех размерностях.
pub(crate) static ICONS: LazyLock<Vec<Icon>> = LazyLock::new(build_icons);

/// Порядок наложения совпадает с `logo_mark`: планета, кольцо поверх, точка поверх.
const LAYERS: [Layer; 3] = [Layer::Planet, Layer::Ring, Layer::Dot];

#[derive(Clone, Copy)]
enum Layer {
    Planet,
    Ring,
    Dot,
}

impl Layer {
    fn color(self) -> [u8; 3] {
        match self {
            Self::Planet => PLANET,
            Self::Ring => RING,
            Self::Dot => LIVE,
        }
    }

    fn alpha(self) -> f32 {
        match self {
            Self::Planet | Self::Dot => 1.0,
            Self::Ring => 0.85,
        }
    }
}

/// Геометрия знака в пикселях иконки.
struct Mark {
    center: (f32, f32),
    radius: f32,
    ring: (f32, f32),
    tilt: f32,
    dot: (f32, f32),
    dot_radius: f32,
    ring_stroke: f32,
}

impl Mark {
    /// Пропорции `logo_mark`, масштабированные от базовых 22 px.
    fn new(size: i32) -> Self {
        let side = size as f32;
        let scale = side / 22.0;
        let center = (side / 2.0, side / 2.0);
        let radius = side * 0.32;
        let ring = (radius * 1.25, radius * 0.5);
        let dot = (center.0 + ring.0 * 0.7, center.1 - radius * 0.55);
        Self {
            center,
            radius,
            ring,
            tilt: -0.5,
            dot,
            dot_radius: 2.2 * scale,
            ring_stroke: 1.2 * scale,
        }
    }
}

fn build_icons() -> Vec<Icon> {
    SIZES.iter().map(|&size| icon(size)).collect()
}

fn icon(size: i32) -> Icon {
    let mark = Mark::new(size);
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            data.extend_from_slice(&pixel(x, y, &mark));
        }
    }
    Icon {
        width: size,
        height: size,
        data,
    }
}

/// Пиксель в ARGB32 (network byte order), как требует SNI.
fn pixel(x: i32, y: i32, mark: &Mark) -> [u8; 4] {
    let samples = (SUPERSAMPLE * SUPERSAMPLE) as f32;
    let step = 1.0 / SUPERSAMPLE as f32;
    let mut rgba = [0.0f32; 4];
    for layer in LAYERS {
        let mut hits = 0u32;
        for sy in 0..SUPERSAMPLE {
            for sx in 0..SUPERSAMPLE {
                let p = (
                    x as f32 + (sx as f32 + 0.5) * step,
                    y as f32 + (sy as f32 + 0.5) * step,
                );
                if contains(p, mark, layer) {
                    hits += 1;
                }
            }
        }
        let coverage = hits as f32 / samples;
        if coverage > 0.0 {
            let color = layer.color();
            let source = [
                color[0] as f32 / 255.0,
                color[1] as f32 / 255.0,
                color[2] as f32 / 255.0,
                layer.alpha() * coverage,
            ];
            rgba = over(rgba, source);
        }
    }
    to_argb([
        (rgba[0] * 255.0).round() as u8,
        (rgba[1] * 255.0).round() as u8,
        (rgba[2] * 255.0).round() as u8,
        (rgba[3] * 255.0).round() as u8,
    ])
}

/// Принадлежность точки слою знака.
fn contains(p: (f32, f32), mark: &Mark, layer: Layer) -> bool {
    match layer {
        Layer::Planet => distance(p, mark.center) <= mark.radius,
        Layer::Ring => ring_distance(p, mark) <= mark.ring_stroke / 2.0,
        Layer::Dot => distance(p, mark.dot) <= mark.dot_radius,
    }
}

fn distance(p: (f32, f32), c: (f32, f32)) -> f32 {
    ((p.0 - c.0).powi(2) + (p.1 - c.1).powi(2)).sqrt()
}

/// Расстояние до наклонённого эллипса: first-order approximation от неявного
/// уравнения — точное аналитически для эллипса вычислять нечем.
fn ring_distance(p: (f32, f32), mark: &Mark) -> f32 {
    let (dx, dy) = (p.0 - mark.center.0, p.1 - mark.center.1);
    let (sin, cos) = (mark.tilt.sin(), mark.tilt.cos());
    let a = dx * cos + dy * sin;
    let b = -dx * sin + dy * cos;
    let (rx, ry) = mark.ring;
    let offset = (a / rx).powi(2) + (b / ry).powi(2) - 1.0;
    let gradient = ((a / rx / rx).powi(2) + (b / ry / ry).powi(2)).sqrt() * 2.0;
    offset.abs() / gradient.max(f32::EPSILON)
}

/// src-over для непонемноженных цветов в каналах `0.0..=1.0`.
fn over(dst: [f32; 4], src: [f32; 4]) -> [f32; 4] {
    let source_alpha = src[3];
    let target = source_alpha + dst[3] * (1.0 - source_alpha);
    if target <= 0.0 {
        return [0.0; 4];
    }
    let mix = |s: f32, d: f32| (s * source_alpha + d * dst[3] * (1.0 - source_alpha)) / target;
    [
        mix(src[0], dst[0]),
        mix(src[1], dst[1]),
        mix(src[2], dst[2]),
        target,
    ]
}

/// RGBA → ARGB32 network byte order: тот же поворот, что в примере crate `ksni`.
fn to_argb(rgba: [u8; 4]) -> [u8; 4] {
    [rgba[3], rgba[0], rgba[1], rgba[2]]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(icon: &Icon, x: i32, y: i32) -> u8 {
        let index = ((y * icon.width + x) * 4) as usize;
        icon.data[index]
    }

    #[test]
    fn icons_are_built_for_every_host_size() {
        let icons = &*ICONS;
        assert_eq!(icons.len(), SIZES.len());
        for (icon, size) in icons.iter().zip(SIZES) {
            assert_eq!(icon.width, size);
            assert_eq!(icon.height, size);
            assert_eq!(icon.data.len(), (size * size * 4) as usize);
        }
    }

    #[test]
    fn argb_puts_alpha_first() {
        assert_eq!(to_argb([10, 20, 30, 40]), [40, 10, 20, 30]);
    }

    #[test]
    fn planet_core_is_opaque_and_corners_are_empty() {
        let small = &ICONS[0];
        let center = small.width / 2;
        assert_eq!(alpha(small, center, center), 255);
        assert_eq!(alpha(small, 0, 0), 0);
        assert_eq!(alpha(small, small.width - 1, small.height - 1), 0);
    }

    #[test]
    fn orbit_reaches_beyond_the_planet() {
        let mark = Mark::new(48);
        // Дальний конец большой оси эллипса: за планетой, но на самой орбите.
        let (rx, _) = mark.ring;
        let right = (
            mark.center.0 + rx * mark.tilt.cos(),
            mark.center.1 + rx * mark.tilt.sin(),
        );
        assert!(contains(right, &mark, Layer::Ring));
        assert!(!contains(right, &mark, Layer::Planet));
    }

    #[test]
    fn live_dot_is_part_of_the_mark() {
        let mark = Mark::new(48);
        assert!(contains(mark.dot, &mark, Layer::Dot));
    }
}
