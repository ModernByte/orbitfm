//! Синтетический аудиовизуализатор (ADR-0019): вертикальные столбцы — оценка
//! огибающей спектра из шести агрегатов mpv, а не бинный FFT.

use eframe::egui::Color32;

use super::spectrum::{MIN_RMS_DB, RMS_RANGE_DB, SpectrumTarget};

/// Диапазон частот, который покрывают столбцы, Гц.
const MIN_FREQ_HZ: f32 = 30.0;
const MAX_FREQ_HZ: f32 = 20_000.0;
/// Постоянная времени нарастания столбца, с.
const ATTACK_SECONDS: f32 = 0.03;
/// Постоянная времени спада, с (80–150 мс по ТЗ).
const DECAY_SECONDS: f32 = 0.12;
/// Скорость падения удержанного пика, единиц уровня в секунду.
const PEAK_FALL_PER_SECOND: f32 = 1.2;
/// «Дыхание» без эфира: уровень, размах и частота волны.
const IDLE_LEVEL: f32 = 0.06;
const IDLE_SWING: f32 = 0.035;
const IDLE_HZ: f32 = 0.45;
/// Наклон огибающей к высоким частотам: низ у эфира всегда плотнее верха.
const TILT: f32 = 0.28;

/// Разумные пределы числа столбцов; настройка выдаётся в этот диапазон.
pub const MIN_BARS: usize = 8;
pub const MAX_BARS: usize = 64;

/// Состояние визуализатора: уровни и пики столбцов. Буферы аллоцируются только
/// в [`VisualizerModel::new`], кадр не выделяет память.
#[derive(Debug, Clone)]
pub struct VisualizerModel {
    levels: Box<[f32]>,
    peaks: Box<[f32]>,
    targets: Box<[f32]>,
}

impl Default for VisualizerModel {
    fn default() -> Self {
        Self::new(DEFAULT_BARS)
    }
}

/// Число столбцов по умолчанию (24 по ТЗ).
pub const DEFAULT_BARS: usize = 24;

impl VisualizerModel {
    pub fn new(bars: usize) -> Self {
        let bars = bars.clamp(MIN_BARS, MAX_BARS);
        Self {
            levels: vec![0.0; bars].into_boxed_slice(),
            peaks: vec![0.0; bars].into_boxed_slice(),
            targets: vec![0.0; bars].into_boxed_slice(),
        }
    }

    pub fn levels(&self) -> &[f32] {
        &self.levels
    }

    pub fn peaks(&self) -> &[f32] {
        &self.peaks
    }

    /// Один кадр: цели полос → attack/decay → peak-hold. `None` — idle-дыхание.
    pub fn advance(&mut self, target: Option<&SpectrumTarget>, dt: f32, time: f64) {
        let dt = dt.clamp(0.0, 0.1);
        let time = time as f32;
        match target {
            Some(target) if target.is_finite() => band_targets(target, time, &mut self.targets),
            _ => idle_targets(time, &mut self.targets),
        }

        for index in 0..self.levels.len() {
            let goal = self.targets[index];
            let level = self.levels[index];
            let tau = if goal > level {
                ATTACK_SECONDS
            } else {
                DECAY_SECONDS
            };
            let blended = level + (goal - level) * (1.0 - (-dt / tau).exp());
            self.levels[index] = blended;
            self.peaks[index] = (self.peaks[index] - PEAK_FALL_PER_SECOND * dt)
                .max(blended)
                .clamp(0.0, 1.0);
        }
    }
}

/// Оценка огибающей: гауссиана в лог-шкале вокруг центроида, ширина от spread,
/// высота от RMS, «живость» от flux и flatness.
fn band_targets(target: &SpectrumTarget, time: f32, out: &mut [f32]) {
    let bars = out.len() as f32;
    let min_log = MIN_FREQ_HZ.log10();
    let span = (MAX_FREQ_HZ.log10() - min_log).max(1e-3);
    let centroid = target.centroid_hz.clamp(MIN_FREQ_HZ, MAX_FREQ_HZ);
    let center = (centroid.log10() - min_log) / span;
    let upper = (centroid + target.spread_hz.max(0.0)).clamp(centroid * 1.01, MAX_FREQ_HZ * 4.0);
    let sigma = ((upper.log10() - centroid.log10()) / span).clamp(0.02, 0.6);

    let flatness = target.flatness.clamp(0.0, 1.0);
    let flux = (target.flux.max(0.0) * 500.0).sqrt().clamp(0.0, 1.0);
    let level = ((target.rms_db - MIN_RMS_DB) / RMS_RANGE_DB).clamp(0.0, 1.0);
    // Плоский (шумовой) спектр шире тонального, но не уже четвёртой части диапазона.
    let width = (sigma * (0.6 + 1.2 * flatness)).clamp(0.16, 0.7);

    for (index, slot) in out.iter_mut().enumerate() {
        let t = (index as f32 + 0.5) / bars;
        let z = (t - center) / width;
        let envelope = (-0.5 * z * z).exp();
        // Широкополосная подложка: у живого эфира есть энергия во всех полосах,
        // иначе половина столбцов стоит на idle-уровне.
        let floor = (0.08 + 0.32 * flatness) * (1.0 - 0.55 * t);
        let ripple = 1.0 + 0.22 * (t * 11.0 + time * 3.1).sin() * flux;
        *slot = (level * (envelope * (1.0 - TILT * t) + floor) * ripple * (0.7 + 0.3 * flatness))
            .clamp(0.0, 1.0);
    }
}

/// «Дыхание» в простое: низкий уровень с медленной бегущей волной.
fn idle_targets(time: f32, out: &mut [f32]) {
    let phase = time * std::f32::consts::TAU * IDLE_HZ;
    for (index, slot) in out.iter_mut().enumerate() {
        let wave = (phase + index as f32 * 0.7).sin();
        *slot = (IDLE_LEVEL + IDLE_SWING * wave).max(0.0);
    }
}

/// Остановки градиента столбца снизу вверх: зелёный → жёлтый → красный.
const BAR_STOPS: [Color32; 5] = [
    Color32::from_rgb(0x16, 0xA3, 0x4A),
    Color32::from_rgb(0x4A, 0xDE, 0x80),
    Color32::from_rgb(0xFB, 0xBF, 0x24),
    Color32::from_rgb(0xFB, 0x92, 0x3C),
    Color32::from_rgb(0xEF, 0x44, 0x44),
];

/// Цвет столбца на относительной высоте `t` (0 — низ, 1 — верх).
pub fn bar_color(t: f32) -> Color32 {
    let stops = BAR_STOPS.len() - 1;
    let scaled = t.clamp(0.0, 1.0) * stops as f32;
    let index = (scaled.floor() as usize).min(stops - 1);
    let local = scaled - index as f32;
    let a = BAR_STOPS[index];
    let b = BAR_STOPS[index + 1];
    let channel = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * local) as u8;
    Color32::from_rgb(
        channel(a.r(), b.r()),
        channel(a.g(), b.g()),
        channel(a.b(), b.b()),
    )
}

impl SpectrumTarget {
    fn is_finite(self) -> bool {
        self.centroid_hz.is_finite()
            && self.spread_hz.is_finite()
            && self.flux.is_finite()
            && self.flatness.is_finite()
            && self.rms_db.is_finite()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(rms_db: f32, centroid_hz: f32) -> SpectrumTarget {
        SpectrumTarget {
            centroid_hz,
            spread_hz: 1_600.0,
            flux: 0.001,
            flatness: 0.4,
            rms_db,
        }
    }

    fn targets(target: SpectrumTarget) -> Vec<f32> {
        let mut out = vec![0.0; 24];
        band_targets(&target, 0.0, &mut out);
        out
    }

    #[test]
    fn louder_signal_lifts_every_band() {
        let quiet = targets(frame(-45.0, 1_700.0));
        let loud = targets(frame(-12.0, 1_700.0));
        assert!(
            loud.iter()
                .zip(&quiet)
                .all(|(loud, quiet)| *loud >= *quiet - 1e-6)
        );
        assert!(loud.iter().sum::<f32>() > quiet.iter().sum::<f32>());
    }

    #[test]
    fn every_band_carries_some_energy() {
        let values = targets(frame(-12.0, 1_700.0));
        assert!(values.iter().all(|value| *value > IDLE_LEVEL));
    }

    #[test]
    fn centroid_moves_the_peak_up_the_bands() {
        let peak_of = |values: &[f32]| {
            values
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                .map(|(index, _)| index)
                .unwrap()
        };
        let bass = targets(frame(-15.0, 90.0));
        let treble = targets(frame(-15.0, 9_000.0));
        assert!(peak_of(&bass) < peak_of(&treble));
    }

    #[test]
    fn targets_stay_finite_and_normalized() {
        let mut model = VisualizerModel::new(24);
        for step in 0..300 {
            let target = frame(-10.0 + (step as f32) * 0.7, 400.0 * step as f32);
            model.advance(Some(&target), 1.0 / 30.0, f64::from(step) / 30.0);
        }
        assert!(
            model
                .levels()
                .iter()
                .all(|l| l.is_finite() && (0.0..=1.0).contains(l))
        );
        assert!(
            model
                .peaks()
                .iter()
                .all(|p| p.is_finite() && (0.0..=1.0).contains(p))
        );
    }

    #[test]
    fn attack_outpaces_decay() {
        let loud = frame(-6.0, 1_700.0);
        let silent = frame(-59.0, 1_700.0);

        let mut rising = VisualizerModel::new(24);
        rising.advance(Some(&loud), 1.0 / 60.0, 0.0);
        let rise = rising.levels()[12];

        let mut settling = VisualizerModel::new(24);
        for step in 0..40 {
            settling.advance(Some(&loud), 1.0 / 60.0, f64::from(step) / 60.0);
        }
        let before = settling.levels()[12];
        settling.advance(Some(&silent), 1.0 / 60.0, 40.0 / 60.0);
        let fall = before - settling.levels()[12];

        assert!(before > 0.0, "столбец должен был подняться");
        assert!(rise > 0.0 && rise > fall, "rise {rise}, fall {fall}");
    }

    #[test]
    fn idle_breathes_in_a_low_band() {
        let mut model = VisualizerModel::new(24);
        for step in 0..200 {
            model.advance(None, 1.0 / 30.0, f64::from(step) / 30.0);
        }
        assert!(
            model
                .levels()
                .iter()
                .all(|l| (0.01..=0.15).contains(l) && l.is_finite())
        );
    }

    #[test]
    fn peaks_hold_above_levels_and_fall() {
        let mut model = VisualizerModel::new(16);
        let loud = frame(-5.0, 1_700.0);
        for step in 0..10 {
            model.advance(Some(&loud), 1.0 / 30.0, f64::from(step) / 30.0);
        }
        let held = model.peaks().to_vec();
        for step in 0..30 {
            model.advance(
                Some(&frame(-59.0, 1_700.0)),
                1.0 / 30.0,
                f64::from(step) / 30.0,
            );
        }
        assert!(
            model
                .peaks()
                .iter()
                .zip(&held)
                .all(|(now, was)| *now <= *was)
        );
        assert!(
            model
                .peaks()
                .iter()
                .zip(model.levels())
                .all(|(peak, level)| *peak >= *level - 1e-6)
        );
    }

    #[test]
    fn bar_count_is_clamped() {
        assert_eq!(VisualizerModel::new(2).levels().len(), MIN_BARS);
        assert_eq!(VisualizerModel::new(4096).peaks().len(), MAX_BARS);
        assert_eq!(VisualizerModel::default().levels().len(), DEFAULT_BARS);
    }

    #[test]
    fn non_finite_frame_falls_back_to_idle() {
        let mut model = VisualizerModel::new(24);
        model.advance(Some(&frame(f32::NAN, 1_700.0)), 0.016, 0.5);
        assert!(model.levels().iter().all(|l| *l < 0.2 && l.is_finite()));
    }

    #[test]
    fn bar_gradient_runs_green_to_red() {
        let bottom = bar_color(0.0);
        let top = bar_color(1.0);
        assert_eq!(bottom, BAR_STOPS[0]);
        assert_eq!(top, BAR_STOPS[BAR_STOPS.len() - 1]);
        assert!(bottom.g() > bottom.r() && top.r() > top.g());
        // за пределы диапазона не выходим
        assert_eq!(bar_color(-1.0), bottom);
        assert_eq!(bar_color(2.0), top);
    }
}
