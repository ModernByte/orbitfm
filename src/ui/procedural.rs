//! Детерминированные процедурные обложки станций (ADR-0044).
//!
//! Чистая функция метаданных [`Station`] и акцента темы: ни сети, ни файлов,
//! ни окружения. Один UUID всегда даёт одну и ту же картинку, поэтому кэш и
//! скриншоты воспроизводимы. Пульсация эфира сюда не входит — её рисует UI
//! поверх готовой текстуры (ADR-0044 §3).

use std::f32::consts::TAU;

use eframe::egui;

use crate::services::Station;

use super::images::FAVICON_SIZE;

/// Префикс ключа процедурной обложки в общем favicon-кэше (ADR-0044 §1).
pub(crate) const KEY_PREFIX: &str = "procedural:";

type Rgb = [f32; 3];

/// Жанровые оттенки: фиксированный словарь ADR-0044 §2, без внешних данных.
const GENRES: [(&str, [u8; 3]); 8] = [
    ("jazz", [217, 142, 56]),
    ("classical", [201, 191, 163]),
    ("electronic", [56, 189, 248]),
    ("rock", [239, 68, 68]),
    ("news", [100, 116, 139]),
    ("ambient", [45, 212, 191]),
    ("pop", [236, 72, 153]),
    ("folk", [132, 204, 22]),
];

/// Ключ процедурной обложки станции в общем favicon-кэше.
pub(crate) fn key(station_uuid: &str) -> String {
    format!("{KEY_PREFIX}{station_uuid}")
}

/// Квадратная обложка [`FAVICON_SIZE`] из метаданных станции.
///
/// UUID задаёт геометрию, жанр — оттенок, голоса — насыщенность, битрейт —
/// контраст. Таблицы ISO→RGB нет: страна остаётся на акценте темы, как
/// допускает ADR-0044 §2.
pub(crate) fn cover(station: &Station, accent: egui::Color32) -> egui::ColorImage {
    let seed = fnv1a(&station.stationuuid);
    let angle = unit(seed, 0) * TAU;
    let frequency = 1.4 + unit(seed, 8) * 2.6;
    let wave = 2.5 + unit(seed, 16) * 4.0;
    let phase = unit(seed, 24) * TAU;
    let orbits = 2 + ((seed >> 32) & 0b11) as usize;
    let orbit_phase = unit(seed, 40) * TAU;
    let ring_base = 0.16 + unit(seed, 44) * 0.18;
    let ring_step = 0.12 + unit(seed, 52) * 0.07;
    let center_x = (unit(seed.rotate_left(13), 0) - 0.5) * 0.24;
    let center_y = (unit(seed.rotate_left(29), 8) - 0.5) * 0.24;
    let grain = (seed >> 48) as u32;

    let accent = rgb(accent);
    let genre = genre_tint(&station.tags).unwrap_or(accent);
    let saturation = 0.50 + 0.40 * smooth(station.votes, 4_000.0);
    let contrast = smooth(station.bitrate, 160.0);
    let top = mix(
        saturate(mix(accent, genre, 0.80), saturation),
        [1.0; 3],
        0.08 + 0.10 * contrast,
    );
    let bottom = mix(
        saturate(mix(accent, genre, 0.45), saturation),
        [0.0; 3],
        0.32 + 0.22 * contrast,
    );
    let glow = mix(saturate(genre, 0.95), [1.0; 3], 0.22);

    let side = FAVICON_SIZE as f32;
    let mut pixels = Vec::with_capacity((FAVICON_SIZE * FAVICON_SIZE) as usize);
    for y in 0..FAVICON_SIZE {
        for x in 0..FAVICON_SIZE {
            let u = (x as f32 + 0.5) / side * 2.0 - 1.0;
            let v = (y as f32 + 0.5) / side * 2.0 - 1.0;
            let dx = u - center_x;
            let dy = v - center_y;
            let radius = (dx * dx + dy * dy).sqrt();
            let theta = dy.atan2(dx);
            let swirl = 0.5 + 0.5 * (theta * frequency + radius * (frequency + 1.2) + phase).sin();
            let sweep = 0.5 + 0.5 * ((u * angle.cos() + v * angle.sin()) * wave - phase).sin();
            let mut color = mix(bottom, top, (swirl * 0.62 + sweep * 0.38).clamp(0.0, 1.0));

            for orbit in 0..orbits {
                let ring = ring_base + orbit as f32 * ring_step;
                let wobble = 0.018 * (theta * (2.0 + orbit as f32) + orbit_phase).sin();
                let band = (1.0 - (radius - ring - wobble).abs() / 0.045).clamp(0.0, 1.0);
                color = mix(color, glow, band * 0.5);
            }
            let core = (1.0 - radius / 0.28).clamp(0.0, 1.0);
            color = mix(color, glow, core * 0.24);

            let noise = (hash_noise(x, y, grain) - 0.5) * 0.035;
            let vignette = 1.0 - 0.22 * (radius * 0.7).min(1.0);
            let shade =
                |channel: f32| ((channel + noise) * vignette * 255.0).clamp(0.0, 255.0) as u8;
            pixels.push(egui::Color32::from_rgb(
                shade(color[0]),
                shade(color[1]),
                shade(color[2]),
            ));
        }
    }
    egui::ColorImage::new([FAVICON_SIZE as usize, FAVICON_SIZE as usize], pixels)
}

/// FNV-1a 64: тот же хеш, что у синтетических станций и снимка.
fn fnv1a(text: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Байт хеша как доля единицы — материал для угла, частоты и фазы.
fn unit(seed: u64, shift: u32) -> f32 {
    ((seed >> shift) & 0xFF) as f32 / 255.0
}

fn genre_tint(tags: &str) -> Option<Rgb> {
    let lower = tags.to_ascii_lowercase();
    for (genre, color) in GENRES {
        if lower
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .any(|word| word == genre)
        {
            return Some([
                f32::from(color[0]) / 255.0,
                f32::from(color[1]) / 255.0,
                f32::from(color[2]) / 255.0,
            ]);
        }
    }
    None
}

fn rgb(color: egui::Color32) -> Rgb {
    [
        f32::from(color.r()) / 255.0,
        f32::from(color.g()) / 255.0,
        f32::from(color.b()) / 255.0,
    ]
}

fn mix(from: Rgb, to: Rgb, amount: f32) -> Rgb {
    let amount = amount.clamp(0.0, 1.0);
    [
        from[0] + (to[0] - from[0]) * amount,
        from[1] + (to[1] - from[1]) * amount,
        from[2] + (to[2] - from[2]) * amount,
    ]
}

fn saturate(color: Rgb, amount: f32) -> Rgb {
    let gray = 0.2126 * color[0] + 0.7152 * color[1] + 0.0722 * color[2];
    mix([gray; 3], color, amount)
}

/// `value / (value + pivot)`: монотонно и без потолка, который пришлось бы
/// выбирать под неизвестный диапазон голосов и битрейтов Radio Browser.
fn smooth(value: u32, pivot: f32) -> f32 {
    let value = value as f32;
    value / (value + pivot)
}

fn hash_noise(x: u32, y: u32, seed: u32) -> f32 {
    let mut value = x.wrapping_mul(0x9E37_79B9) ^ y.wrapping_mul(0x85EB_CA6B) ^ seed;
    value ^= value >> 13;
    value = value.wrapping_mul(0xC2B2_AE35);
    value ^= value >> 16;
    (value & 0xFFFF) as f32 / 0xFFFF as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::markers::sample_station;

    fn accent() -> egui::Color32 {
        egui::Color32::from_rgb(80, 150, 255)
    }

    #[test]
    fn same_uuid_gives_the_same_cover() {
        let station = sample_station();
        let first = cover(&station, accent());
        assert_eq!(first.size, [FAVICON_SIZE as usize, FAVICON_SIZE as usize]);
        assert_eq!(first.pixels.len(), first.size[0] * first.size[1]);
        assert!(first.pixels.iter().all(|pixel| pixel.a() == 255));
        assert_eq!(first, cover(&station, accent()));
    }

    #[test]
    fn uuid_drives_geometry() {
        let first = sample_station();
        let mut second = sample_station();
        second.stationuuid = "1c3c1f9e-6f0a-4c78-9d5b-0e8a8f0d6f11".to_owned();
        assert_ne!(cover(&first, accent()), cover(&second, accent()));
    }

    #[test]
    fn genre_votes_and_bitrate_repaint_the_palette() {
        let station = sample_station();
        let base = cover(&station, accent());

        let mut ambient = station.clone();
        ambient.tags = "ambient, chillout".to_owned();
        assert_ne!(base, cover(&ambient, accent()));

        let mut popular = station.clone();
        popular.votes = 50_000;
        assert_ne!(base, cover(&popular, accent()));

        let mut dense = station;
        dense.bitrate = 320;
        assert_ne!(base, cover(&dense, accent()));
    }

    #[test]
    fn genre_matches_whole_words_only() {
        assert!(genre_tint("jazz, smooth").is_some());
        assert!(genre_tint("JAZZ").is_some());
        assert!(genre_tint("rockabilly").is_none());
        assert!(genre_tint("").is_none());
    }

    #[test]
    fn key_is_namespaced() {
        assert_eq!(key("abc"), "procedural:abc");
    }
}
