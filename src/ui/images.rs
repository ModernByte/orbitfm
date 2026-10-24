//! Асинхронный кэш favicon станций (ADR-0016).
//!
//! Presentation-слой: чистый стейт-машина без сети. Спавн загрузки и
//! `Context::load_texture` выполняет App-оркестратор; сюда приходят уже
//! декодированные [`egui::ColorImage`].

use std::collections::{HashMap, VecDeque};

use eframe::egui;
use tokio::sync::mpsc;

/// Сторона кэшируемой текстуры, px (ADR-0016, п. 6).
pub(crate) const FAVICON_SIZE: u32 = 64;
/// Максимум записей в кэше; дальше вытесняется давно не использовавшаяся.
const MAX_CACHED: usize = 256;
/// Одновременно летящих запросов (ADR-0016, п. 4).
const MAX_IN_FLIGHT: usize = 8;
/// Максимальный принятый размер favicon, байт (ADR-0025).
pub(crate) const MAX_FAVICON_BYTES: usize = 512 * 1024;
/// Максимальная сторона исходного изображения, px (ADR-0025).
pub(crate) const MAX_FAVICON_DIMENSION: u32 = 512;
/// Потолок аллокаций декодера, байт (ADR-0025).
const MAX_DECODE_ALLOC: u64 = 8 * 1024 * 1024;

/// Результат фоновой загрузки: `None` — ошибка сети/декода.
pub(crate) type FaviconResult = (String, Option<egui::ColorImage>);

enum FaviconState {
    Queued,
    Loading,
    Ready(egui::TextureHandle),
    Failed,
}

struct Entry {
    state: FaviconState,
    last_used: u64,
}

pub(crate) struct FaviconCache {
    entries: HashMap<String, Entry>,
    pending: VecDeque<String>,
    in_flight: usize,
    frame: u64,
    sender: mpsc::UnboundedSender<FaviconResult>,
    receiver: mpsc::UnboundedReceiver<FaviconResult>,
}

impl Default for FaviconCache {
    fn default() -> Self {
        Self::new()
    }
}

impl FaviconCache {
    pub(crate) fn new() -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        Self {
            entries: HashMap::new(),
            pending: VecDeque::new(),
            in_flight: 0,
            frame: 0,
            sender,
            receiver,
        }
    }

    /// Отправитель для фоновых задач App: задача шлёт `(url, image|None)`.
    pub(crate) fn sender(&self) -> mpsc::UnboundedSender<FaviconResult> {
        self.sender.clone()
    }

    /// Новый кадр: сдвигает счётчик для политики вытеснения.
    pub(crate) fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// Текстura favicon; при неизвестном URL ставит его в очередь загрузки.
    pub(crate) fn texture(&mut self, url: &str) -> Option<egui::TextureHandle> {
        if let Some(entry) = self.entries.get_mut(url) {
            entry.last_used = self.frame;
            return match &entry.state {
                FaviconState::Ready(texture) => Some(texture.clone()),
                _ => None,
            };
        }
        if self.entries.len() >= MAX_CACHED {
            self.evict();
        }
        self.entries.insert(
            url.to_owned(),
            Entry {
                state: FaviconState::Queued,
                last_used: self.frame,
            },
        );
        self.pending.push_back(url.to_owned());
        None
    }

    /// Есть незавершённые загрузки — окно стоит перерисовать.
    pub(crate) fn is_busy(&self) -> bool {
        self.in_flight > 0 || !self.pending.is_empty()
    }

    /// Забирает URLи для спавна задач, пока есть свободные слоты.
    pub(crate) fn take_fetches(&mut self) -> Vec<String> {
        let mut batch = Vec::new();
        while self.in_flight < MAX_IN_FLIGHT {
            let Some(url) = self.pending.pop_front() else {
                break;
            };
            let Some(entry) = self.entries.get_mut(&url) else {
                continue;
            };
            if !matches!(entry.state, FaviconState::Queued) {
                continue;
            }
            entry.state = FaviconState::Loading;
            self.in_flight += 1;
            batch.push(url);
        }
        batch
    }

    /// Применяет готовые результаты и освобождает слоты.
    pub(crate) fn poll(&mut self, ctx: &egui::Context) {
        while let Ok((url, image)) = self.receiver.try_recv() {
            if self.in_flight > 0 {
                self.in_flight -= 1;
            }
            let state = match image {
                Some(image) => {
                    let texture = ctx.load_texture(&url, image, egui::TextureOptions::LINEAR);
                    FaviconState::Ready(texture)
                }
                None => FaviconState::Failed,
            };
            if let Some(entry) = self.entries.get_mut(&url) {
                entry.state = state;
                entry.last_used = self.frame;
            }
        }
    }

    fn evict(&mut self) {
        let frame = self.frame;
        let victim = self
            .entries
            .iter()
            .filter(|(_, entry)| {
                !matches!(entry.state, FaviconState::Queued | FaviconState::Loading)
            })
            .min_by_key(|(_, entry)| entry.last_used)
            .filter(|(_, entry)| entry.last_used < frame)
            .map(|(url, _)| url.clone());
        if let Some(url) = victim {
            self.entries.remove(&url);
        }
    }
}

/// Декодирует байты favicon в квадратную RGBA-текстуру (ADR-0016, ADR-0025).
pub(crate) fn decode_favicon(bytes: &[u8]) -> Option<egui::ColorImage> {
    if bytes.is_empty() || bytes.len() > MAX_FAVICON_BYTES {
        return None;
    }

    let mut probe = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    if !is_allowed_format(probe.format()?) {
        return None;
    }
    probe.limits(decode_limits());
    let (width, height) = probe.into_dimensions().ok()?;
    if width == 0 || height == 0 || width > MAX_FAVICON_DIMENSION || height > MAX_FAVICON_DIMENSION
    {
        return None;
    }

    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    reader.limits(decode_limits());
    let decoded = reader.decode().ok()?;
    let rgba = decoded.to_rgba8();
    let resized = image::imageops::resize(
        &rgba,
        FAVICON_SIZE,
        FAVICON_SIZE,
        image::imageops::FilterType::Lanczos3,
    );
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [FAVICON_SIZE as usize, FAVICON_SIZE as usize],
        resized.as_raw(),
    ))
}

fn is_allowed_format(format: image::ImageFormat) -> bool {
    matches!(
        format,
        image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP
    )
}

fn decode_limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_FAVICON_DIMENSION);
    limits.max_image_height = Some(MAX_FAVICON_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    limits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image() -> egui::ColorImage {
        egui::ColorImage::new([2, 2], vec![egui::Color32::WHITE; 4])
    }

    #[test]
    fn unknown_url_is_queued_once() {
        let mut cache = FaviconCache::new();
        assert!(cache.texture("https://a/icon.png").is_none());
        assert!(cache.texture("https://a/icon.png").is_none());
        assert_eq!(cache.entries.len(), 1);
        assert_eq!(cache.take_fetches(), vec!["https://a/icon.png".to_owned()]);
        assert!(
            cache.take_fetches().is_empty(),
            "повторный забор того же URL недопустим"
        );
    }

    #[test]
    fn in_flight_limit_batches_fetches() {
        let mut cache = FaviconCache::new();
        for index in 0..MAX_IN_FLIGHT + 4 {
            cache.texture(&format!("https://a/{index}"));
        }
        assert_eq!(cache.take_fetches().len(), MAX_IN_FLIGHT);
        assert_eq!(cache.pending.len(), 4);
    }

    #[test]
    fn failed_result_is_cached_and_frees_slot() {
        let mut cache = FaviconCache::new();
        cache.texture("https://a/broken");
        assert_eq!(cache.take_fetches().len(), 1);
        cache
            .sender
            .send(("https://a/broken".to_owned(), None))
            .unwrap();
        cache.poll(&egui::Context::default());
        assert_eq!(cache.in_flight, 0);
        assert!(cache.texture("https://a/broken").is_none());
        assert!(
            cache.pending.is_empty(),
            "Failed не должен повторно становиться в очередь"
        );
    }

    #[test]
    fn evictions_keep_active_entries() {
        let mut cache = FaviconCache::new();
        for index in 0..MAX_CACHED {
            cache.begin_frame();
            cache.texture(&format!("https://a/{index}"));
            cache.take_fetches();
            cache
                .sender
                .send((format!("https://a/{index}"), Some(image())))
                .unwrap();
            cache.poll(&egui::Context::default());
        }
        assert_eq!(cache.entries.len(), MAX_CACHED);
        cache.begin_frame();
        cache.texture("https://a/fresh");
        assert_eq!(
            cache.entries.len(),
            MAX_CACHED,
            "свежий URL вытесняет давно не видевшийся"
        );
        assert!(cache.texture("https://a/1").is_some());
        assert!(cache.texture("https://a/fresh").is_none());
    }

    #[test]
    fn decode_rejects_garbage() {
        assert!(decode_favicon(b"not an image").is_none());
    }

    #[test]
    fn decode_resizes_to_square() {
        let source = image::RgbImage::from_pixel(120, 40, image::Rgb([10, 20, 30]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgb8(source)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .expect("png должен кодироваться");
        let decoded = decode_favicon(&bytes).expect("png должен декодироваться");
        assert_eq!(decoded.size, [FAVICON_SIZE as usize, FAVICON_SIZE as usize]);
    }

    #[test]
    fn decode_rejects_oversized_payload() {
        let bytes = vec![0u8; MAX_FAVICON_BYTES + 1];
        assert!(decode_favicon(&bytes).is_none());
    }

    #[test]
    fn decode_rejects_oversized_dimensions() {
        let source =
            image::RgbImage::from_pixel(MAX_FAVICON_DIMENSION + 1, 1, image::Rgb([10, 20, 30]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgb8(source)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .expect("png должен кодироваться");
        assert!(decode_favicon(&bytes).is_none());
    }

    #[test]
    fn decode_rejects_svg_and_gif() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#;
        assert!(decode_favicon(svg).is_none());

        let gif = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff\
                    !\xf9\x04\x01\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\
                    \x00\x02\x02D\x01\x00;";
        assert!(decode_favicon(gif).is_none());
    }

    #[test]
    fn decode_accepts_webp() {
        let source = image::RgbImage::from_pixel(8, 8, image::Rgb([10, 20, 30]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgb8(source)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::WebP,
            )
            .expect("webp должен кодироваться");
        assert!(decode_favicon(&bytes).is_some());
    }
}
