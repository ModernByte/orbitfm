//! Асинхронный кэш favicon станций (ADR-0016, ADR-0025 ред. 3).
//!
//! Presentation-слой: чистая стейт-машина без сети. Спавн загрузки и
//! `Context::load_texture` выполняет App-оркестратор; сюда приходят уже
//! декодированные [`egui::ColorImage`]. Границы — инварианты, а не ориентиры:
//! `entries.len() <= MAX_CACHED`, `in_flight <= MAX_IN_FLIGHT` и
//! `queued == count(Queued)` выполняются после каждого публичного вызова.

use std::collections::HashMap;

use eframe::egui;
use tokio::sync::mpsc;
use tracing::debug;

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

/// Результат фоновой загрузки: `(url, поколение, образ)`, `None` — ошибка.
pub(crate) type FaviconResult = (String, u32, Option<egui::ColorImage>);

enum FaviconState {
    Queued,
    Loading,
    Ready(egui::TextureHandle),
    Failed,
}

struct Entry {
    state: FaviconState,
    last_used: u64,
    /// Поколение записи: перезасланный после вытеснения тот же URL различим.
    generation: u32,
}

/// Счётчики для диагностики: почему вместо иконки инициалы (ADR-0025, п. 7).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CacheStats {
    pub(crate) entries: usize,
    pub(crate) served: u32,
    pub(crate) failed: u32,
    pub(crate) evicted: u32,
}

pub(crate) struct FaviconCache {
    entries: HashMap<String, Entry>,
    /// Буфер скана: переиспользуется, потому что забор вызывается раз на пачку.
    scan: Vec<(String, u32, u64)>,
    in_flight: usize,
    queued: usize,
    generation: u32,
    frame: u64,
    stats: CacheStats,
    sender: mpsc::Sender<FaviconResult>,
    receiver: mpsc::Receiver<FaviconResult>,
}

impl Default for FaviconCache {
    fn default() -> Self {
        Self::new()
    }
}

impl FaviconCache {
    pub(crate) fn new() -> Self {
        // Буфер результатов ровно по числу слотов: задача не может ждать и не
        // может переполнить канал (ADR-0025, п. 5).
        let (sender, receiver) = mpsc::channel(MAX_IN_FLIGHT);
        Self {
            entries: HashMap::new(),
            scan: Vec::new(),
            in_flight: 0,
            queued: 0,
            generation: 0,
            frame: 0,
            stats: CacheStats::default(),
            sender,
            receiver,
        }
    }

    /// Страховка слота: `Drop` отправляет результат всегда, поэтому паника,
    /// `?` и ранний выход не оставляют `Loading` навечно (ADR-0025, п. 5).
    pub(crate) fn guard(&self, url: String, generation: u32) -> FetchGuard {
        FetchGuard {
            sender: self.sender.clone(),
            result: Some((url, generation, None)),
        }
    }

    /// Новый кадр: сдвигает счётчик для политики вытеснения.
    pub(crate) fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// Текстура favicon; при неизвестном URL ставит его в очередь загрузки.
    /// При исчерпанном потолке отказывает во вставке, а не превышает его.
    pub(crate) fn texture(&mut self, url: &str) -> Option<egui::TextureHandle> {
        if let Some(entry) = self.entries.get_mut(url) {
            entry.last_used = self.frame;
            return match &entry.state {
                FaviconState::Ready(texture) => Some(texture.clone()),
                _ => None,
            };
        }
        if self.entries.len() >= MAX_CACHED && !self.evict() {
            return None;
        }
        self.generation = self.generation.wrapping_add(1);
        self.entries.insert(
            url.to_owned(),
            Entry {
                state: FaviconState::Queued,
                last_used: self.frame,
                generation: self.generation,
            },
        );
        self.queued += 1;
        None
    }

    /// Забывает неудачные загрузки: после смены сетевой политики (opt-in
    /// HTTP-обложек) станции получают второй шанс, не дожидаясь вытеснения.
    pub(crate) fn retry_failed(&mut self) {
        self.entries
            .retain(|_, entry| !matches!(entry.state, FaviconState::Failed));
    }

    /// Есть незавершённые загрузки — окно стоит перерисовать.
    pub(crate) fn is_busy(&self) -> bool {
        self.in_flight > 0 || self.queued > 0
    }

    /// Текущие счётчики (PERF-01).
    pub(crate) fn stats(&self) -> CacheStats {
        CacheStats {
            entries: self.entries.len(),
            ..self.stats
        }
    }

    /// Забирает пары `(url, поколение)` для спавна задач, пока есть слоты.
    /// Порядок — LRU-first: сначала ближайшее к обзору.
    pub(crate) fn take_fetches(&mut self) -> Vec<(String, u32)> {
        let limit = MAX_IN_FLIGHT.saturating_sub(self.in_flight);
        if limit == 0 || self.queued == 0 {
            return Vec::new();
        }
        self.scan.clear();
        for (url, entry) in &self.entries {
            if matches!(entry.state, FaviconState::Queued) {
                self.scan
                    .push((url.clone(), entry.generation, entry.last_used));
            }
        }
        self.scan.sort_by_key(|(_, _, last_used)| *last_used);
        let mut batch = Vec::with_capacity(limit);
        for (url, generation, _) in self.scan.iter().take(limit) {
            let Some(entry) = self.entries.get_mut(url) else {
                continue;
            };
            if !matches!(entry.state, FaviconState::Queued) || entry.generation != *generation {
                continue;
            }
            entry.state = FaviconState::Loading;
            self.queued -= 1;
            self.in_flight += 1;
            batch.push((url.clone(), *generation));
        }
        batch
    }

    /// Применяет готовые результаты и освобождает слоты.
    pub(crate) fn poll(&mut self, ctx: &egui::Context) {
        while let Ok((url, generation, image)) = self.receiver.try_recv() {
            // Слот освобождается ровно один раз: либо здесь, либо при вытеснении
            // `Loading`. Поэтому сначала проверка записи, и только потом аллокация
            // текстуры — чужой результат не должен ни есть память, ни считать слот.
            let Some(entry) = self.entries.get_mut(&url) else {
                continue;
            };
            if entry.generation != generation || !matches!(entry.state, FaviconState::Loading) {
                continue;
            }
            self.in_flight -= 1;
            entry.state = if let Some(image) = image {
                self.stats.served += 1;
                FaviconState::Ready(ctx.load_texture(&url, image, egui::TextureOptions::LINEAR))
            } else {
                self.stats.failed += 1;
                FaviconState::Failed
            };
            entry.last_used = self.frame;
        }
    }

    /// Ступенчатое вытеснение: `Failed` → давно не видевшаяся `Ready` →
    /// `Queued` → `Loading` (её слот освобождается здесь, а поздний результат
    /// отвергается проверкой поколения). `false` — вытеснять нечего.
    fn evict(&mut self) -> bool {
        let victim = self
            .oldest(|state| matches!(state, FaviconState::Failed))
            .or_else(|| self.oldest(|state| matches!(state, FaviconState::Ready(_))))
            .or_else(|| self.oldest(|state| matches!(state, FaviconState::Queued)))
            .or_else(|| self.oldest(|state| matches!(state, FaviconState::Loading)));
        let Some(url) = victim else {
            return false;
        };
        let Some(entry) = self.entries.remove(&url) else {
            return false;
        };
        match entry.state {
            FaviconState::Queued => self.queued -= 1,
            FaviconState::Loading => self.in_flight -= 1,
            _ => {}
        }
        self.stats.evicted += 1;
        true
    }

    /// Самый давно не видевшийся URL в состоянии, которое принимает предикат.
    fn oldest(&self, accept: impl Fn(&FaviconState) -> bool) -> Option<String> {
        self.entries
            .iter()
            .filter(|(_, entry)| accept(&entry.state))
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(url, _)| url.clone())
    }
}

/// RAII-страховка слота загрузки (ADR-0025, п. 5).
pub(crate) struct FetchGuard {
    sender: mpsc::Sender<FaviconResult>,
    result: Option<FaviconResult>,
}

impl FetchGuard {
    /// Отмечает успешную декодировку.
    pub(crate) fn succeed(&mut self, image: egui::ColorImage) {
        if let Some((_, _, slot)) = self.result.as_mut() {
            *slot = Some(image);
        }
    }
}

impl Drop for FetchGuard {
    fn drop(&mut self) {
        if let Some(result) = self.result.take() {
            let _ = self.sender.try_send(result);
        }
    }
}

/// Декодирует байты favicon в квадратную RGBA-текстуру (ADR-0016, ADR-0025).
/// Отказ always безопасен: `None` означает инициалы станции, а причину видно в
/// логе одной строкой (ADR-0025, п. 7).
pub(crate) fn decode_favicon(bytes: &[u8]) -> Option<egui::ColorImage> {
    decode_image(bytes).map_or_else(
        |cause| {
            debug!(%cause, "favicon: декодирование отклонено");
            None
        },
        Some,
    )
}

/// Причина отказа декодера: ограниченный перечень, новых строк i18n нет.
#[derive(Debug, Clone, Copy)]
enum DecodeReject {
    /// Пустое тело ответа.
    Empty,
    /// Байтов больше, чем разрешено (ADR-0025, п. 2).
    TooLarge { bytes: usize },
    /// Формат не опознан по сигнатуре или заголовки не читаются.
    Unrecognized,
    /// Формат вне allowlist: SVG, GIF, ICO и прочее (ADR-0025, п. 1).
    DisallowedFormat(image::ImageFormat),
    /// Сторона больше потолка разрешения (ADR-0025, п. 3).
    OutOfBounds { width: u32, height: u32 },
    /// Декодер вернул ошибку: битые данные или decompression bomb.
    Undecodable,
}

impl std::fmt::Display for DecodeReject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("пустой ответ"),
            Self::TooLarge { bytes } => {
                write!(f, "ответ {bytes} байт превышает лимит {MAX_FAVICON_BYTES}")
            }
            Self::Unrecognized => f.write_str("формат не опознан по сигнатуре"),
            Self::DisallowedFormat(format) => write!(f, "формат {format:?} вне allowlist"),
            Self::OutOfBounds { width, height } => write!(
                f,
                "размер {width}×{height} превышает потолок {MAX_FAVICON_DIMENSION} px"
            ),
            Self::Undecodable => f.write_str("декодер не смог разобрать изображение"),
        }
    }
}

fn decode_image(bytes: &[u8]) -> Result<egui::ColorImage, DecodeReject> {
    if bytes.is_empty() {
        return Err(DecodeReject::Empty);
    }
    if bytes.len() > MAX_FAVICON_BYTES {
        return Err(DecodeReject::TooLarge { bytes: bytes.len() });
    }

    let mut probe = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| DecodeReject::Unrecognized)?;
    let format = probe.format().ok_or(DecodeReject::Unrecognized)?;
    if !is_allowed_format(format) {
        return Err(DecodeReject::DisallowedFormat(format));
    }
    probe.limits(decode_limits());
    let (width, height) = probe
        .into_dimensions()
        .map_err(|_| DecodeReject::Unrecognized)?;
    if width == 0 || height == 0 || width > MAX_FAVICON_DIMENSION || height > MAX_FAVICON_DIMENSION
    {
        return Err(DecodeReject::OutOfBounds { width, height });
    }

    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| DecodeReject::Unrecognized)?;
    reader.limits(decode_limits());
    let decoded = reader.decode().map_err(|_| DecodeReject::Undecodable)?;
    let rgba = decoded.to_rgba8();
    let resized = image::imageops::resize(
        &rgba,
        FAVICON_SIZE,
        FAVICON_SIZE,
        image::imageops::FilterType::Lanczos3,
    );
    Ok(egui::ColorImage::from_rgba_unmultiplied(
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

    fn ctx() -> egui::Context {
        egui::Context::default()
    }

    /// Кэш, собранный в обход публичного API: все `MAX_CACHED` записей в
    /// `Loading` с поколением 0. Нужен потому, что ступень `Loading` в лестнице
    /// вытеснения последняя и через `texture()`/`take_fetches()` недостижима
    /// (`MAX_IN_FLIGHT` меньше потолка записей).
    fn all_loading() -> FaviconCache {
        let mut cache = FaviconCache::new();
        for index in 0..MAX_CACHED {
            cache.entries.insert(
                format!("https://l/{index}"),
                Entry {
                    state: FaviconState::Loading,
                    last_used: 0,
                    generation: 0,
                },
            );
        }
        cache.in_flight = MAX_CACHED;
        cache
    }

    #[test]
    fn unknown_url_is_queued_once() {
        let mut cache = FaviconCache::new();
        assert!(cache.texture("https://a/icon.png").is_none());
        assert!(cache.texture("https://a/icon.png").is_none());
        assert_eq!(cache.entries.len(), 1);
        let taken = cache.take_fetches();
        assert_eq!(taken.len(), 1);
        assert_eq!(taken[0].0, "https://a/icon.png");
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
        assert_eq!(cache.queued, 4);
        assert_eq!(cache.stats().entries, MAX_IN_FLIGHT + 4);
    }

    #[test]
    fn failed_result_is_cached_and_frees_slot() {
        let mut cache = FaviconCache::new();
        let taken = {
            cache.texture("https://a/broken");
            cache.take_fetches()
        };
        let (url, generation) = taken[0].clone();
        cache
            .sender
            .try_send((url, generation, None))
            .expect("слот свободен");
        cache.poll(&ctx());
        assert_eq!(cache.in_flight, 0);
        assert!(cache.texture("https://a/broken").is_none());
        assert_eq!(cache.queued, 0, "Failed не должен становиться в очередь");
        assert_eq!(cache.stats().failed, 1);
    }

    /// Смена сетевой политики (opt-in HTTP) забывает отказы: иконка снова
    /// попадает в очередь под новым поколением, а готовые текстуры остаются.
    #[test]
    fn retry_failed_requeues_only_rejected_urls() {
        let mut cache = FaviconCache::new();
        let taken = {
            cache.texture("https://a/broken");
            cache.take_fetches()
        };
        let (url, generation) = taken[0].clone();
        cache
            .sender
            .try_send((url, generation, None))
            .expect("слот свободен");
        cache.poll(&ctx());
        let old_generation = cache.entries["https://a/broken"].generation;

        cache.retry_failed();
        assert!(
            !cache.entries.contains_key("https://a/broken"),
            "отказ забыт, а не оставлен навсегда"
        );
        assert!(cache.texture("https://a/broken").is_none());
        let entry = &cache.entries["https://a/broken"];
        assert!(matches!(entry.state, FaviconState::Queued));
        assert!(entry.generation > old_generation, "поколение новое");
        assert_eq!(cache.take_fetches().len(), 1, "URL снова в очереди");
    }

    #[test]
    fn entry_count_never_exceeds_max() {
        let mut cache = FaviconCache::new();
        for index in 0..MAX_CACHED * 10 {
            cache.begin_frame();
            cache.texture(&format!("https://a/{index}"));
            assert!(
                cache.entries.len() <= MAX_CACHED,
                "потолок нарушен на вставке {index}"
            );
        }
    }

    #[test]
    fn cap_holds_when_a_full_cache_needs_a_slot() {
        let mut cache = FaviconCache::new();
        for index in 0..MAX_CACHED {
            cache.begin_frame();
            cache.texture(&format!("https://a/{index}"));
        }
        cache.take_fetches();
        let before = cache.stats();
        cache.begin_frame();
        assert!(cache.texture("https://a/overflow").is_none());
        assert_eq!(cache.entries.len(), MAX_CACHED);
        assert_eq!(cache.stats().evicted, before.evicted + 1);
    }

    /// Ступень `Loading` в лестнице вытеснения — последняя, и через публичный API
    /// к ней не дойти: `in_flight ≤ MAX_IN_FLIGHT < MAX_CACHED`, поэтому ниже по
    /// лестнице всегда найдётся кандидат. Состояние собрано вручную именно
    /// поэтому — проверяется ветка, а не недостижимый сценарий.
    #[test]
    fn loading_is_evicted_only_as_last_stage_and_frees_slot() {
        let mut cache = all_loading();
        let loading = cache.in_flight;
        cache.begin_frame();
        assert!(cache.texture("https://l/fresh").is_none());
        assert_eq!(cache.entries.len(), MAX_CACHED, "потолок не превышен");
        assert_eq!(
            cache.in_flight,
            loading - 1,
            "слот вытесненной загрузки возвращён"
        );
    }

    /// Поздний результат вытесненной загрузки не уносит уже освобождённый слот:
    /// проверка записи стоит до `load_texture` и до арифметики `in_flight`.
    #[test]
    fn cancelled_slot_is_not_double_decremented() {
        let mut cache = all_loading();
        let before: Vec<String> = cache.entries.keys().cloned().collect();
        cache.begin_frame();
        cache.texture("https://l/fresh");
        let after: Vec<String> = cache.entries.keys().cloned().collect();
        let victim = before
            .iter()
            .find(|url| !after.contains(url))
            .expect("одна запись вытеснена");
        let in_flight = cache.in_flight;
        cache
            .sender
            .try_send((victim.clone(), 0, Some(image())))
            .expect("канал не полон");
        cache.poll(&ctx());
        assert_eq!(cache.in_flight, in_flight, "слот освобождён ровно один раз");
        assert!(
            !cache.entries.contains_key(victim),
            "мёртвая запись не воскресает"
        );
    }

    #[test]
    fn stale_generation_is_not_applied_to_recreated_entry() {
        let mut cache = FaviconCache::new();
        cache.texture("https://a/loop");
        let (url, generation) = cache.take_fetches()[0].clone();
        // Запись умерла, тот же URL встал заново — поколение другое.
        cache.entries.remove(&url);
        cache.texture(&url);
        let fresh = cache.take_fetches()[0].1;
        assert_ne!(generation, fresh);
        cache
            .sender
            .try_send((url.clone(), generation, Some(image())))
            .expect("канал не полон");
        cache.poll(&ctx());
        assert!(
            cache.texture(&url).is_none(),
            "результат старого поколения не должен становиться текстурой"
        );
        assert_eq!(cache.stats().served, 0);
    }

    #[test]
    fn no_texture_for_absent_entry() {
        let cache_context = ctx();
        let mut cache = FaviconCache::new();
        cache
            .sender
            .try_send(("https://a/ghost".to_owned(), 1, Some(image())))
            .expect("канал не полон");
        cache.poll(&cache_context);
        assert!(cache.entries.is_empty());
        assert_eq!(cache.in_flight, 0);
        assert_eq!(cache.stats().served, 0);
    }

    #[test]
    fn guard_releases_slot_on_drop_without_send() {
        let mut cache = FaviconCache::new();
        cache.texture("https://a/dying");
        let (url, generation) = cache.take_fetches()[0].clone();
        assert_eq!(cache.in_flight, 1);
        let guard = cache.guard(url, generation);
        drop(guard);
        cache.poll(&ctx());
        assert_eq!(cache.in_flight, 0);
        assert_eq!(cache.stats().failed, 1, "паника равносильна отказу");
    }

    #[test]
    fn result_channel_capacity_matches_in_flight_limit() {
        let mut cache = FaviconCache::new();
        for index in 0..MAX_IN_FLIGHT {
            cache.texture(&format!("https://a/{index}"));
        }
        let taken = cache.take_fetches();
        assert_eq!(taken.len(), MAX_IN_FLIGHT);
        for (url, generation) in taken {
            cache
                .sender
                .try_send((url, generation, Some(image())))
                .expect("бюфер результатов равен числу слотов");
        }
    }

    #[test]
    fn queued_counter_tracks_state_after_scripted_sequence() {
        let mut cache = FaviconCache::new();
        for frame in 0..MAX_CACHED * 3 {
            cache.begin_frame();
            cache.texture(&format!("https://a/{frame}"));
            cache.texture("https://a/pinned");
            for (url, generation) in cache.take_fetches() {
                let mut guard = cache.guard(url, generation);
                if generation % 3 == 0 {
                    guard.succeed(image());
                }
                drop(guard);
            }
            cache.poll(&ctx());
            let queued = cache
                .entries
                .values()
                .filter(|entry| matches!(entry.state, FaviconState::Queued))
                .count();
            assert_eq!(cache.queued, queued, "счётчик очереди разошёлся");
            assert_eq!(
                cache.in_flight,
                cache
                    .entries
                    .values()
                    .filter(|entry| matches!(entry.state, FaviconState::Loading))
                    .count()
            );
            assert!(cache.entries.len() <= MAX_CACHED);
            assert!(cache.in_flight <= MAX_IN_FLIGHT);
        }
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
