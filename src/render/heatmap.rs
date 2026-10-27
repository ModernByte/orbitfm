//! Тепловая карта стран: заливка государств на текстуре глобуса.
//!
//! Кольца Natural Earth растеризуются в эквидистантную карту (долгота → X,
//! широта → Y) с чётно-нечётным правилом заливки: отверстия и антарктическое
//! кольцо на антимеридиане работают без специальных случаев. Текстура
//! пересобирается только при смене выборки станций или режима карты, поэтому
//! в кадровом пути остаётся один `textureSample`.

use std::collections::HashMap;

use eframe::egui::Color32;

use crate::geo::{CountryShape, GeoService};

/// Разрешение карты: 0.18° на тексель — край контура не отстаёт от глобуса
/// даже в режиме приближения.
pub(crate) const HEAT_SIZE: [u32; 2] = [2048, 1024];

/// Цвет страны: непрозрачный оттенок рампы и его прозрачность.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CountryColor {
    pub color: Color32,
    pub alpha: f32,
}

/// Готовая текстура тепловой карты в линейном premultiplied RGBA.
#[derive(Debug)]
pub(crate) struct HeatFrame {
    pub size: [u32; 2],
    pub pixels: Vec<u8>,
}

/// Растеризует страны, для которых есть цвет. `None` — красить нечего.
pub(crate) fn build(
    geo: &GeoService,
    colors: &HashMap<String, CountryColor>,
    size: [u32; 2],
) -> Option<HeatFrame> {
    let [width, height] = size;
    if width == 0 || height == 0 || colors.is_empty() {
        return None;
    }

    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    let mut painted = false;
    for (country, shape) in geo.countries() {
        let Some(color) = colors.get(&country.iso_a2) else {
            continue;
        };
        let rgba = premultiplied_linear(*color);
        if rgba[3] == 0 {
            continue;
        }
        fill_shape(&mut pixels, width as usize, height as usize, shape, rgba);
        painted = true;
    }

    painted.then_some(HeatFrame { size, pixels })
}

fn premultiplied_linear(color: CountryColor) -> [u8; 4] {
    let alpha = color.alpha.clamp(0.0, 1.0);
    let linear = |value: u8| {
        let value = f32::from(value) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    let byte = |value: f32| (value * alpha * 255.0).round().clamp(0.0, 255.0) as u8;

    [
        byte(linear(color.color.r())),
        byte(linear(color.color.g())),
        byte(linear(color.color.b())),
        (alpha * 255.0).round() as u8,
    ]
}

fn fill_shape(pixels: &mut [u8], width: usize, height: usize, shape: &CountryShape, rgba: [u8; 4]) {
    fill_rings(pixels, width, height, &shape.rings, rgba);
}

/// Заливает все кольца страны как одну фигуру с отверстиями.
fn fill_rings(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    rings: &[Vec<[f64; 2]>],
    rgba: [u8; 4],
) {
    let width_f = width as f64;
    let height_f = height as f64;
    let mapped: Vec<Vec<[f64; 2]>> = rings
        .iter()
        .filter(|ring| ring.len() >= 3)
        .map(|ring| {
            let base = ring[0][0];
            ring.iter()
                .map(|[lon, lat]| {
                    let mut lon = *lon;
                    while lon - base > 180.0 {
                        lon -= 360.0;
                    }
                    while lon - base < -180.0 {
                        lon += 360.0;
                    }
                    [
                        (lon + 180.0) / 360.0 * width_f,
                        (90.0 - lat) / 180.0 * height_f,
                    ]
                })
                .collect()
        })
        .collect();

    let (mut top, mut bottom) = (f64::INFINITY, f64::NEG_INFINITY);
    for ring in &mapped {
        for [_, y] in ring {
            top = top.min(*y);
            bottom = bottom.max(*y);
        }
    }
    if !top.is_finite() || !bottom.is_finite() {
        return;
    }

    let start = ((top - 0.5).ceil() as i64).clamp(0, height as i64) as usize;
    let end = ((bottom - 0.5).ceil() as i64).clamp(0, height as i64) as usize;

    let mut crossings: [Vec<f64>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for row in start..end {
        let y = row as f64 + 0.5;
        for slot in &mut crossings {
            slot.clear();
        }

        for ring in &mapped {
            for index in 0..ring.len() {
                let a = ring[index];
                let b = ring[(index + 1) % ring.len()];
                if !((a[1] <= y && y < b[1]) || (b[1] <= y && y < a[1])) {
                    continue;
                }
                let t = (y - a[1]) / (b[1] - a[1]);
                let x = a[0] + (b[0] - a[0]) * t;
                // Три периодических образа кольца: без них страны на
                // антимеридиане заливаются через весь континент.
                for (slot, shift) in crossings.iter_mut().zip([-width_f, 0.0, width_f]) {
                    slot.push(x + shift);
                }
            }
        }

        for slot in &mut crossings {
            slot.sort_by(f64::total_cmp);
            for [left, right] in slot.as_chunks::<2>().0 {
                fill_span(pixels, width, row, *left, *right, rgba);
            }
        }
    }
}

fn fill_span(pixels: &mut [u8], width: usize, row: usize, from: f64, to: f64, rgba: [u8; 4]) {
    let start = (from - 0.5).ceil().max(0.0).min(width as f64) as usize;
    let end = (to - 0.5).ceil().max(0.0).min(width as f64) as usize;
    if end <= start {
        return;
    }
    let row_start = row * width * 4;
    for column in start..end {
        let index = row_start + column * 4;
        pixels[index..index + 4].copy_from_slice(&rgba);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red(alpha: f32) -> [u8; 4] {
        premultiplied_linear(CountryColor {
            color: Color32::from_rgb(255, 0, 0),
            alpha,
        })
    }

    fn alpha_at(pixels: &[u8], width: usize, x: usize, y: usize) -> u8 {
        pixels[(y * width + x) * 4 + 3]
    }

    #[test]
    fn color_is_stored_premultiplied_in_linear_space() {
        let rgba = red(0.5);
        assert_eq!(rgba[0], 128, "красный канал умножен на альфу");
        assert_eq!(rgba[1], 0);
        assert_eq!(rgba[3], 128);
    }

    #[test]
    fn ring_fills_its_own_area_only() {
        let rings = vec![vec![
            [-20.0, -10.0],
            [20.0, -10.0],
            [20.0, 10.0],
            [-20.0, 10.0],
        ]];
        let width = 100;
        let height = 50;
        let mut pixels = vec![0u8; width * height * 4];
        fill_rings(&mut pixels, width, height, &rings, red(1.0));

        // (0°, 0°) — центр кольца; (90°, 0°) — Тихий океан.
        let center = (width / 2, height / 2);
        assert_eq!(alpha_at(&pixels, width, center.0, center.1), 255);
        assert_eq!(alpha_at(&pixels, width, 75, height / 2), 0);
    }

    #[test]
    fn hole_stays_transparent() {
        let rings = vec![
            vec![[-20.0, -20.0], [20.0, -20.0], [20.0, 20.0], [-20.0, 20.0]],
            vec![[-5.0, -5.0], [5.0, -5.0], [5.0, 5.0], [-5.0, 5.0]],
        ];
        let width = 100;
        let height = 50;
        let mut pixels = vec![0u8; width * height * 4];
        fill_rings(&mut pixels, width, height, &rings, red(1.0));

        assert_eq!(alpha_at(&pixels, width, 50, 25), 0, "дырка прозрачна");
        assert_eq!(alpha_at(&pixels, width, 46, 25), 255, "кольцо залито");
    }

    #[test]
    fn antimeridian_ring_wraps_across_both_edges() {
        let rings = vec![vec![
            [175.0, -10.0],
            [185.0, -10.0],
            [185.0, 10.0],
            [175.0, 10.0],
        ]];
        let width = 100;
        let height = 50;
        let mut pixels = vec![0u8; width * height * 4];
        fill_rings(&mut pixels, width, height, &rings, red(1.0));

        // Долгота 179° — у правого края, −179° — у левого.
        assert_eq!(alpha_at(&pixels, width, 99, height / 2), 255);
        assert_eq!(alpha_at(&pixels, width, 0, height / 2), 255);
        assert_eq!(
            alpha_at(&pixels, width, 50, height / 2),
            0,
            "центр карты пуст"
        );
    }

    #[test]
    fn embedded_countries_are_painted_by_iso() {
        let geo = GeoService::from_embedded().expect("встроенный GeoJSON");
        let colors = HashMap::from([(
            "DE".to_owned(),
            CountryColor {
                color: Color32::from_rgb(255, 0, 0),
                alpha: 1.0,
            },
        )]);
        let frame = build(&geo, &colors, [512, 256]).expect("карта не пуста");

        let x = ((13.405 + 180.0) / 360.0 * 512.0) as usize;
        let y = ((90.0 - 52.52) / 180.0 * 256.0) as usize;
        assert_eq!(alpha_at(&frame.pixels, 512, x, y), 255, "Берлин");

        let ocean_x = ((-140.0 + 180.0) / 360.0 * 512.0) as usize;
        assert_eq!(alpha_at(&frame.pixels, 512, ocean_x, 128), 0, "океан");
    }

    #[test]
    fn empty_colors_make_no_frame() {
        let geo = GeoService::from_embedded().expect("встроенный GeoJSON");
        assert!(build(&geo, &HashMap::new(), [64, 32]).is_none());
    }
}
