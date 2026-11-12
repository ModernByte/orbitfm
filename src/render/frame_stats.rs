//! Замер бюджета кадра глобуса: фиксированные кольцевые буферы без аллокаций
//! в горячем пути (план `docs/plans/render-frame-budget.md` § 3.2, ADR-0059 § 4).
//!
//! Включается переменной окружения `ORBITFM_FRAME_STATS=1`: тогда `frame_tick`
//! раз в [`REPORT_PERIOD`] печатает в лог p50/p95/макс по метрикам и число
//! кадров за окно (fps). Каждый `end` дополнительно отдаёт trace-событие
//! `orbitfm::frame`, поэтому границы кадра видны и профилировщику
//! (`RUST_LOG=orbitfm::frame=trace`).
//!
//! Модуль живёт на потоке UI: eframe вызывает `update` и `paint` (включая
//! `egui_wgpu::CallbackTrait::prepare`) на одном потоке, поэтому состояние
//! хранится в `thread_local` и не требует блокировок. Когда замер выключен,
//! `begin` не читает часы, а `end`/`frame_tick` выходят по флагу — на горячий
//! путь остаётся одна проверка.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use tracing::trace;

/// Сколько последних кадров держит кольцевой буфер одной метрики.
const CAPACITY: usize = 600;
/// Период отчёта в лог при включённом замере: 600 кадров покрывают окно
/// с запасом даже на 15 fps.
const REPORT_PERIOD: Duration = Duration::from_secs(5);
/// Флаг включает замер; выключен по умолчанию, как ручные пробы ADR-0059 § 4.
const ENV_FLAG: &str = "ORBITFM_FRAME_STATS";

/// Границы кадра, между которыми замеряется время (план § 3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Metric {
    /// `MotionMixer::update` в `app::draw_layers`.
    MotionUpdate,
    /// `globe_view::show` целиком.
    GlobeShow,
    /// Проекция маркеров (`marker_screens`/`marker_visuals_batch`).
    MarkersProject,
    /// `paint_markers`.
    MarkersPaint,
    /// `paint_country_highlights`.
    CountriesPaint,
    /// `GlobeRenderer::prepare` в wgpu-callback.
    GlobePrepare,
    /// `GlobeCallback::prepare` целиком: надстройка над [`Metric::GlobePrepare`].
    GlobeCallback,
}

impl Metric {
    /// Порядок — соответствие `slot()`; тест закрепляет длину.
    const ALL: [Metric; 7] = [
        Metric::MotionUpdate,
        Metric::GlobeShow,
        Metric::MarkersProject,
        Metric::MarkersPaint,
        Metric::CountriesPaint,
        Metric::GlobePrepare,
        Metric::GlobeCallback,
    ];

    /// Стабильное имя метрики в логе; тест закрепляет набор.
    fn as_str(self) -> &'static str {
        match self {
            Metric::MotionUpdate => "motion_update",
            Metric::GlobeShow => "globe_show",
            Metric::MarkersProject => "markers_project",
            Metric::MarkersPaint => "markers_paint",
            Metric::CountriesPaint => "countries_paint",
            Metric::GlobePrepare => "globe_prepare",
            Metric::GlobeCallback => "globe_callback",
        }
    }

    /// Индекс серии; держим явно, чтобы `Metric::ALL` и `slot` не разъехались.
    fn slot(self) -> usize {
        match self {
            Metric::MotionUpdate => 0,
            Metric::GlobeShow => 1,
            Metric::MarkersProject => 2,
            Metric::MarkersPaint => 3,
            Metric::CountriesPaint => 4,
            Metric::GlobePrepare => 5,
            Metric::GlobeCallback => 6,
        }
    }
}

/// Кольцевой буфер длительностей одного кадра в наносекундах.
///
/// `u32` хватает с запасом: 4,29 с на кадр — это уже не бюджет, а авария;
/// сортировка копии, а не буфера, поэтому снимок для персентилей не портит
/// данные.
struct Series {
    samples: [u32; CAPACITY],
    len: usize,
    cursor: usize,
}

impl Series {
    fn new() -> Self {
        Self {
            samples: [0; CAPACITY],
            len: 0,
            cursor: 0,
        }
    }

    fn push(&mut self, nanos: u32) {
        self.samples[self.cursor] = nanos;
        self.cursor = (self.cursor + 1) % CAPACITY;
        self.len = (self.len + 1).min(CAPACITY);
    }

    /// Персентиль в миллисекундах; пустая серия — ноль.
    fn quantile_ms(&self, quantile: f64) -> f64 {
        if self.len == 0 {
            return 0.0;
        }
        let mut sorted = [0u32; CAPACITY];
        sorted[..self.len].copy_from_slice(&self.samples[..self.len]);
        sorted[..self.len].sort_unstable();
        let rank = ((self.len - 1) as f64 * quantile).round() as usize;
        f64::from(sorted[rank.min(self.len - 1)]) / 1e6
    }

    fn max_ms(&self) -> f64 {
        if self.len == 0 {
            return 0.0;
        }
        f64::from(self.samples[..self.len].iter().copied().max().unwrap_or(0)) / 1e6
    }
}

/// Сборщик метрик кадра; тестируется напрямую, в коде — через thread-local.
pub(crate) struct FrameStats {
    enabled: bool,
    series: [Series; Metric::ALL.len()],
    frames: u32,
    window_start: Instant,
}

impl FrameStats {
    fn new_with(enabled: bool) -> Self {
        Self {
            enabled,
            series: std::array::from_fn(|_| Series::new()),
            frames: 0,
            window_start: Instant::now(),
        }
    }

    fn record(&mut self, metric: Metric, elapsed: Duration) {
        let nanos = u32::try_from(elapsed.as_nanos()).unwrap_or(u32::MAX);
        self.series[metric.slot()].push(nanos);
    }

    /// Отчёт для лога: fps окна и персентили по каждой метрике.
    fn report(&self, now: Instant) -> String {
        let seconds = now.duration_since(self.window_start).as_secs_f64();
        let fps = if seconds > 0.0 {
            f64::from(self.frames) / seconds
        } else {
            0.0
        };
        let mut out = format!("кадр: {fps:.1} fps за {seconds:.1} с");
        for metric in Metric::ALL {
            let series = &self.series[metric.slot()];
            out.push_str(&format!(
                "; {} p50={:.2} p95={:.2} макс={:.2} мс",
                metric.as_str(),
                series.quantile_ms(0.5),
                series.quantile_ms(0.95),
                series.max_ms(),
            ));
        }
        out
    }

    fn reset_window(&mut self, now: Instant) {
        self.window_start = now;
        self.frames = 0;
    }
}

impl Default for FrameStats {
    fn default() -> Self {
        Self::new_with(env_enabled())
    }
}

fn env_enabled() -> bool {
    std::env::var_os(ENV_FLAG).is_some_and(|value| value.to_str() != Some("0"))
}

thread_local! {
    static FRAME_STATS: RefCell<FrameStats> = RefCell::new(FrameStats::default());
}

/// Начинает замер; `None`, когда замер выключен — тогда часы не читаются.
pub(crate) fn begin() -> Option<Instant> {
    FRAME_STATS.with(|stats| stats.borrow().enabled.then(Instant::now))
}

/// Завершает замер и отдаёт trace-событие; `None` беззвучно игнорируется.
pub(crate) fn end(metric: Metric, start: Option<Instant>) {
    let Some(start) = start else {
        return;
    };
    let elapsed = start.elapsed();
    trace!(
        target: "orbitfm::frame",
        metric = metric.as_str(),
        micros = elapsed.as_micros(),
        "кадр"
    );
    FRAME_STATS.with(|stats| stats.borrow_mut().record(metric, elapsed));
}

/// Отмечает отрисованный кадр стола; раз в [`REPORT_PERIOD`] печатает отчёт.
pub(crate) fn frame_tick() {
    FRAME_STATS.with(|stats| {
        let mut stats = stats.borrow_mut();
        if !stats.enabled {
            return;
        }
        let now = Instant::now();
        stats.frames += 1;
        if now.duration_since(stats.window_start) >= REPORT_PERIOD {
            let report = stats.report(now);
            tracing::info!(target: "orbitfm::frame", "{report}");
            stats.reset_window(now);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_slots_cover_every_variant() {
        for (index, metric) in Metric::ALL.iter().enumerate() {
            assert_eq!(metric.slot(), index, "{} вне порядка", metric.as_str());
        }
    }

    #[test]
    fn quantile_reports_milliseconds() {
        let mut series = Series::new();
        for micros in 1..=100 {
            series.push(micros * 1_000);
        }
        // p50 на возрастающем ряду 1..100 мкс ≈ 50 мкс.
        assert!((series.quantile_ms(0.5) - 0.050).abs() < 0.001);
        assert!((series.max_ms() - 0.100).abs() < 0.001);
        assert_eq!(Series::new().quantile_ms(0.5), 0.0);
    }

    #[test]
    fn ring_keeps_the_last_window() {
        let mut series = Series::new();
        for value in 1..=(CAPACITY as u32 + 10) {
            series.push(value * 1_000);
        }
        assert_eq!(series.len, CAPACITY);
        // В окне остались последние 600 значений (11..=610 мкс).
        assert!((series.max_ms() - 0.610).abs() < 0.001);
    }

    #[test]
    fn report_counts_frames_and_names_metrics() {
        let mut stats = FrameStats::new_with(true);
        stats.frames = 150;
        stats.record(Metric::MarkersProject, Duration::from_micros(120));
        let report = stats.report(Instant::now());
        assert!(report.contains("markers_project"), "{report}");
        assert!(report.starts_with("кадр: "), "{report}");
    }
}
