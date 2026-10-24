//! Сглаженное состояние аудио-огибающей и срез метрик для визуализатора
//! (ADR-0019, ADR-0029). Отрисовки здесь нет.

pub(crate) const MIN_RMS_DB: f32 = -60.0;
pub(crate) const RMS_RANGE_DB: f32 = 60.0;
/// Нижняя граница полос: тише неё полоса считается подавленной (ADR-0027).
pub(crate) const MIN_BAND_DB: f32 = -90.0;
const BAND_RANGE_DB: f32 = 80.0;

/// Нормированные уровни Bass/Mid/High, `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BandTargets {
    pub bass: f32,
    pub mid: f32,
    pub high: f32,
}

impl BandTargets {
    /// Переводит сырые дБFS в нормированные уровни без знания аудио-модуля.
    pub fn from_db(bass_db: f32, mid_db: f32, high_db: f32) -> Self {
        Self {
            bass: normalize_band(bass_db),
            mid: normalize_band(mid_db),
            high: normalize_band(high_db),
        }
    }
}

/// Целевые значения визуализатора, уже не зависящие от аудио-модуля.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpectrumTarget {
    pub centroid_hz: f32,
    pub spread_hz: f32,
    pub flux: f32,
    pub flatness: f32,
    pub rms_db: f32,
    /// Нормированные Bass/Mid/High; `None` — полосы недоступны.
    pub bands: Option<BandTargets>,
}

fn normalize_band(db: f32) -> f32 {
    if !db.is_finite() {
        return 0.0;
    }
    ((db - MIN_BAND_DB) / BAND_RANGE_DB).clamp(0.0, 1.0)
}

/// Сглаженные метрики эфира.
#[derive(Debug, Clone, Copy, Default)]
pub struct SpectrumOverlay {
    level: f32,
    flux: f32,
    initialized: bool,
}

impl SpectrumOverlay {
    /// Обновляет сглаженные значения; `None` запускает затухание.
    pub fn update(&mut self, target: Option<SpectrumTarget>, dt: f32) {
        let dt = dt.clamp(0.0, 0.1);

        let Some(target) = target else {
            self.level *= (-dt * 4.0).exp();
            self.flux *= (-dt * 4.0).exp();
            if self.level < 0.005 {
                self.level = 0.0;
                self.flux = 0.0;
                self.initialized = false;
            }
            return;
        };

        let normalized = normalize(target);
        if !self.initialized {
            self.level = normalized.level;
            self.flux = normalized.flux;
            self.initialized = true;
            return;
        }

        let fast = 1.0 - (-dt * 10.0).exp();
        self.level += (normalized.level - self.level) * fast;
        self.flux += (normalized.flux - self.flux) * fast;
    }

    /// Сигнал эфира 0..1 для Fresnel-обода: сглаженные RMS и flux.
    pub fn pulse(&self) -> f32 {
        (self.level * 0.75 + self.flux * 0.25).clamp(0.0, 1.0)
    }
}

fn normalize(target: SpectrumTarget) -> SpectrumOverlay {
    SpectrumOverlay {
        level: ((target.rms_db - MIN_RMS_DB) / RMS_RANGE_DB).clamp(0.0, 1.0),
        flux: (target.flux.max(0.0) * 500.0).sqrt().clamp(0.0, 1.0),
        initialized: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(rms_db: f32) -> SpectrumTarget {
        SpectrumTarget {
            centroid_hz: 1_700.0,
            spread_hz: 1_600.0,
            flux: 0.001,
            flatness: 0.4,
            rms_db,
            bands: None,
        }
    }

    #[test]
    fn snaps_to_first_frame() {
        let mut overlay = SpectrumOverlay::default();
        overlay.update(Some(target(-20.0)), 0.016);

        assert!((overlay.level - 40.0 / 60.0).abs() < 1e-6);
    }

    #[test]
    fn decays_without_data() {
        let mut overlay = SpectrumOverlay::default();
        overlay.update(Some(target(-20.0)), 0.016);
        for _ in 0..120 {
            overlay.update(None, 1.0 / 30.0);
        }
        assert_eq!(overlay.level, 0.0);
        assert_eq!(overlay.pulse(), 0.0);
    }

    #[test]
    fn clamps_out_of_range_levels() {
        let mut overlay = SpectrumOverlay::default();
        overlay.update(Some(target(-400.0)), 0.016);
        assert_eq!(overlay.level, 0.0);
        overlay.update(Some(target(40.0)), 0.016);
        assert!(overlay.level <= 1.0);
    }

    #[test]
    fn ignores_non_positive_time_step() {
        let mut overlay = SpectrumOverlay::default();
        overlay.update(Some(target(-20.0)), 0.0);
        assert!(overlay.pulse() > 0.0);
    }

    #[test]
    fn pulse_follows_level_and_flux() {
        let mut quiet = SpectrumOverlay::default();
        quiet.update(Some(target(-50.0)), 0.016);

        let mut loud = SpectrumOverlay::default();
        loud.update(
            Some(SpectrumTarget {
                flux: 0.004,
                ..target(-10.0)
            }),
            0.016,
        );

        assert!(quiet.pulse() > 0.0 && quiet.pulse() < loud.pulse());
        assert!(loud.pulse() <= 1.0);
    }

    #[test]
    fn normalizes_band_levels_to_unit_range() {
        let bands = BandTargets::from_db(-90.0, -50.0, -10.0);
        assert_eq!(bands.bass, 0.0);
        assert!((bands.mid - 0.5).abs() < 1e-6);
        assert_eq!(bands.high, 1.0);

        let invalid = BandTargets::from_db(f32::NAN, f32::INFINITY, -200.0);
        assert_eq!(
            invalid,
            BandTargets {
                bass: 0.0,
                mid: 0.0,
                high: 0.0,
            }
        );
    }
}
