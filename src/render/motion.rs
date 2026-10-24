//! Аудио-реактивное движение глобуса (ADR-0028): spring-squash, idle-дыхание,
//! onset/beat, шесть характеров и общая деформация для меша, маркеров, стран
//! и hit-test. Отрисовки здесь нет.

use std::cmp::Ordering;
use std::collections::VecDeque;

use glam::Vec3;

use crate::settings::{MotionReactivity, MotionStyle};

use super::SpectrumTarget;

/// Частота idle-дыхания, Гц.
const IDLE_BREATH_HZ: f32 = 0.12;
/// Постоянная возврата к idle после Stop/паузы, с.
pub const RELEASE_TAU: f32 = 0.10;
/// Атака огибающей полос, с.
const ATTACK_TAU: f32 = 0.03;
/// Спад огибающей полос, с.
const BAND_RELEASE_TAU: f32 = 0.22;
/// Выход аудио-вклада на полную после появления сигнала, с.
const RELEASE_RISE_TAU: f32 = 0.05;
/// Жёсткость и затухание пружины squash (околокритическое).
const SQUASH_STIFFNESS: f32 = 120.0;
const SQUASH_DAMPING: f32 = 14.0;
/// Максимальный шаг полунеявного интегратора, с.
const MAX_STEP: f32 = 1.0 / 240.0;
/// Верхняя граница `dt`: после сна окна цель не должна прыгать.
const MAX_DT: f32 = 0.1;
/// Частота бегущей ряби, Гц.
const RIPPLE_RATE: f32 = 0.6;
/// Частота очень медленного дрейфа Ambient Drift, Гц.
const DRIFT_HZ: f32 = 0.05;
const SILENCE_LEVEL: f32 = -60.0;
const SILENCE_RANGE: f32 = 60.0;

/// Шаг onset-детектора, с: анализ эфира идёт с частотой 20 Гц.
const ONSET_STEP: f32 = 0.05;
/// История детектора, кадров: 2,1 с.
const HISTORY_FRAMES: usize = 43;
/// Окно базовой линии: последняя секунда без последних 200 мс.
const BASELINE_FRAMES: usize = 20;
const BASELINE_SKIP: usize = 4;
const BASELINE_MIN: usize = 6;
/// Минимальный и максимальный период бита: 180–60 BPM.
const MIN_PERIOD: f32 = 60.0 / 180.0;
const MAX_PERIOD: f32 = 1.0;
const DEFAULT_PERIOD: f32 = 0.5;
/// Сколько интервалов усредняет оценка периода.
const PERIOD_SAMPLES: usize = 4;
/// Рефрактерный интервал, с.
const REFRACTORY: f32 = 0.15;
/// Порог баса над медианой: абсолютный и в сигмах.
const BASS_MARGIN: f32 = 0.04;
const BASS_SIGMA: f32 = 0.35;
/// Порог flux над медианой: абсолютный и в сигмах.
const FLUX_MARGIN: f32 = 0.02;
const FLUX_SIGMA: f32 = 0.45;
/// Превышение над порогом, дающее полную силу onset.
const ONSET_RANGE: f32 = 0.25;
const CONFIDENCE_GAIN: f32 = 0.30;
const CONFIDENCE_DECAY_TAU: f32 = 1.5;
const BEAT_DECAY_TAU: f32 = 0.18;
/// Уверенность, ниже которой фаза бита не применяется.
const CONFIDENCE_GATE_LOW: f32 = 0.20;
const CONFIDENCE_GATE_HIGH: f32 = 0.50;

/// Постоянная усреднения признаков для Auto, с.
const FEATURE_TAU: f32 = 2.5;
/// Минимальное удержание стиля Auto, с.
const STYLE_HOLD: f32 = 3.0;
/// Кроссфейд параметров стиля, с.
const STYLE_FADE: f32 = 1.0;
/// Границы логарифмической нормировки центроида, Гц.
const CENTROID_LOW_HZ: f32 = 200.0;
const CENTROID_HIGH_HZ: f32 = 4_000.0;
const EQUAL_SHARES: (f32, f32, f32) = (1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0);

/// Линейная деформация сферы: сжатие по вертикали с сохранением объёма.
///
/// `scale_xz` компенсирует объём, поэтому планета сжимается по вертикали
/// и расширяется по горизонтали, а не «раздувается».
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlobeDeform {
    scale_y: f32,
    scale_xz: f32,
}

impl Default for GlobeDeform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl GlobeDeform {
    pub const IDENTITY: Self = Self {
        scale_y: 1.0,
        scale_xz: 1.0,
    };

    /// Деформация по текущему растяжению `scale_y`.
    pub fn new(scale_y: f32) -> Self {
        let scale_y = if scale_y.is_finite() {
            scale_y.max(1e-4)
        } else {
            1.0
        };
        Self {
            scale_y,
            scale_xz: 1.0 / scale_y.sqrt(),
        }
    }

    pub fn scale_y(&self) -> f32 {
        self.scale_y
    }

    pub fn scale_xz(&self) -> f32 {
        self.scale_xz
    }

    /// Точка единичной сферы → точка деформированной поверхности.
    pub fn to_world(self, unit: Vec3) -> Vec3 {
        Vec3::new(
            unit.x * self.scale_xz,
            unit.y * self.scale_y,
            unit.z * self.scale_xz,
        )
    }

    /// Обратное преобразование; видимые точки мира возвращаются на сферу.
    pub fn to_unit(self, world: Vec3) -> Vec3 {
        Vec3::new(
            world.x / self.scale_xz,
            world.y / self.scale_y,
            world.z / self.scale_xz,
        )
    }
}

/// Снимок движения на кадр. Все поля — чистые числа, без ссылок на GPU/UI.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlobeMotion {
    pub deform: GlobeDeform,
    /// Амплитуда радиальной ряби, 0..=0.004.
    pub ripple: f32,
    pub ripple_phase: f32,
    /// Музыкальная добавка к yaw, рад; камера решает, применять ли её.
    pub yaw_sway: f32,
    /// Переворот камеры; включается в фазе 3.
    pub roll: f32,
    /// Усиление Fresnel-кромки, 0..=0.15.
    pub flash: f32,
    /// Фаза бита 0..1; 0 — момент onset.
    pub beat_phase: f32,
    /// Время микшера, с.
    pub time: f32,
    /// Эффективный характер движения (Auto разрешён в один из шести).
    pub style: MotionStyle,
    /// Аудио-интенсивность 0..1 для HUD и выбора частоты кадров.
    pub intensity: f32,
}

impl Default for GlobeMotion {
    fn default() -> Self {
        Self {
            deform: GlobeDeform::IDENTITY,
            ripple: 0.0,
            ripple_phase: 0.0,
            yaw_sway: 0.0,
            roll: 0.0,
            flash: 0.0,
            beat_phase: 0.0,
            time: 0.0,
            style: MotionStyle::Auto,
            intensity: 0.0,
        }
    }
}

impl GlobeMotion {
    pub fn disabled(style: MotionStyle) -> Self {
        Self {
            style,
            ..Self::default()
        }
    }

    /// Значения `CameraUniform.motion`: `scale_y-1`, `scale_xz-1`, ripple, phase.
    pub fn uniform_values(&self) -> [f32; 4] {
        [
            self.deform.scale_y() - 1.0,
            self.deform.scale_xz() - 1.0,
            self.ripple,
            self.ripple_phase,
        ]
    }

    /// Значения `CameraUniform.motion2`: flash, style_id, beat_phase, time.
    pub fn uniform2_values(&self) -> [f32; 4] {
        [self.flash, self.style.id(), self.beat_phase, self.time]
    }
}

/// Параметры характера движения; Auto смешивает их кроссфейдом.
#[derive(Debug, Clone, Copy, PartialEq)]
struct StyleParams {
    /// Множитель squash от Bass.
    squash: f32,
    /// Амплитуда добавляемой качки yaw, рад.
    sway: f32,
    /// Множитель ряби от High.
    ripple: f32,
    /// Максимальное усиление Fresnel-кромки.
    flash: f32,
    /// Вклад импульса бита в squash.
    beat: f32,
    /// Вес очень медленного дрейфа.
    drift: f32,
    /// Добавка к скорости ряби.
    twitch: f32,
}

impl StyleParams {
    const SOFT_PULSE: Self = Self {
        squash: 1.0,
        sway: 0.0,
        ripple: 0.45,
        flash: 0.0,
        beat: 0.0,
        drift: 0.0,
        twitch: 0.0,
    };
    const GROOVE_SWAY: Self = Self {
        squash: 0.65,
        sway: 0.015,
        ripple: 0.55,
        flash: 0.0,
        beat: 0.15,
        drift: 0.0,
        twitch: 0.0,
    };
    const BASS_DROP: Self = Self {
        squash: 1.25,
        sway: 0.0,
        ripple: 0.50,
        flash: 0.10,
        beat: 0.35,
        drift: 0.0,
        twitch: 0.15,
    };
    const AMBIENT_DRIFT: Self = Self {
        squash: 0.45,
        sway: 0.006,
        ripple: 0.70,
        flash: 0.0,
        beat: 0.0,
        drift: 1.0,
        twitch: 0.0,
    };
    const SPARKLE_TWITCH: Self = Self {
        squash: 0.50,
        sway: 0.0,
        ripple: 1.0,
        flash: 0.05,
        beat: 0.10,
        drift: 0.0,
        twitch: 1.8,
    };
    const BEAT_MARCH: Self = Self {
        squash: 0.80,
        sway: 0.008,
        ripple: 0.45,
        flash: 0.04,
        beat: 0.80,
        drift: 0.0,
        twitch: 0.3,
    };

    fn for_style(style: MotionStyle) -> Self {
        match style {
            MotionStyle::Auto | MotionStyle::SoftPulse => Self::SOFT_PULSE,
            MotionStyle::GrooveSway => Self::GROOVE_SWAY,
            MotionStyle::BassDrop => Self::BASS_DROP,
            MotionStyle::AmbientDrift => Self::AMBIENT_DRIFT,
            MotionStyle::SparkleTwitch => Self::SPARKLE_TWITCH,
            MotionStyle::BeatMarch => Self::BEAT_MARCH,
        }
    }

    fn blend(self, other: Self, t: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Self {
            squash: mix(self.squash, other.squash),
            sway: mix(self.sway, other.sway),
            ripple: mix(self.ripple, other.ripple),
            flash: mix(self.flash, other.flash),
            beat: mix(self.beat, other.beat),
            drift: mix(self.drift, other.drift),
            twitch: mix(self.twitch, other.twitch),
        }
    }
}

/// Держит эффективный стиль: минимальное удержание Auto и кроссфейд
/// параметров. Без случайности — только сигнал и время.
#[derive(Debug)]
struct StyleDirector {
    /// Стиль, к параметрам которого идёт (или уже завершён) кроссфейд.
    target: MotionStyle,
    hold: f32,
    from: StyleParams,
    to: StyleParams,
    /// Прошедшее время кроссфейда, с.
    fade: f32,
}

impl Default for StyleDirector {
    fn default() -> Self {
        let params = StyleParams::for_style(MotionStyle::SoftPulse);
        Self {
            target: MotionStyle::SoftPulse,
            hold: 0.0,
            from: params,
            to: params,
            fade: STYLE_FADE,
        }
    }
}

impl StyleDirector {
    /// `auto_pick` — лучшее совпадение по сигналу; `None` — эфира нет.
    fn update(
        &mut self,
        requested: MotionStyle,
        auto_pick: Option<MotionStyle>,
        has_bands: bool,
        dt: f32,
    ) -> (MotionStyle, StyleParams) {
        if requested == MotionStyle::Auto {
            if let Some(pick) = auto_pick {
                self.hold += dt;
                // Полосы пропали — стиль, который их требует, сразу уходит
                // в Soft Pulse; иначе ждём минимальное удержание.
                if self.target.requires_bands() && !has_bands {
                    self.fade_to(if pick.requires_bands() {
                        MotionStyle::SoftPulse
                    } else {
                        pick
                    });
                } else if pick != self.target && self.hold >= STYLE_HOLD {
                    self.fade_to(pick);
                }
            }
        } else {
            let effective = if requested.requires_bands() && !has_bands {
                MotionStyle::SoftPulse
            } else {
                requested
            };
            if effective != self.target {
                self.fade_to(effective);
            }
            self.hold = 0.0;
        }

        self.fade = (self.fade + dt).min(STYLE_FADE);
        let t = smoothstep(self.fade / STYLE_FADE);
        (self.target, self.from.blend(self.to, t))
    }

    fn fade_to(&mut self, style: MotionStyle) {
        let current = self.from.blend(self.to, smoothstep(self.fade / STYLE_FADE));
        self.from = current;
        self.to = StyleParams::for_style(style);
        self.fade = 0.0;
        self.target = style;
        self.hold = 0.0;
    }
}

/// Состояние ритма: уверенность, фаза, импульс бита и регулярность.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct BeatState {
    confidence: f32,
    phase: f32,
    pulse: f32,
    regularity: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct OnsetSample {
    bass: f32,
    flux: f32,
}

/// Адаптивный onset-детектор по Bass и flux (ADR-0028, п. 5).
#[derive(Debug)]
struct OnsetDetector {
    history: VecDeque<OnsetSample>,
    accumulator: f32,
    clock: f32,
    last_onset: Option<f32>,
    intervals: VecDeque<f32>,
    period: f32,
    phase: f32,
    confidence: f32,
    pulse: f32,
}

impl Default for OnsetDetector {
    fn default() -> Self {
        Self {
            history: VecDeque::with_capacity(HISTORY_FRAMES),
            accumulator: 0.0,
            clock: 0.0,
            last_onset: None,
            intervals: VecDeque::with_capacity(PERIOD_SAMPLES),
            period: DEFAULT_PERIOD,
            phase: 0.0,
            confidence: 0.0,
            pulse: 0.0,
        }
    }
}

impl OnsetDetector {
    fn update(&mut self, live: bool, bass: f32, flux: f32, dt: f32) -> BeatState {
        if !live {
            *self = Self::default();
            return self.state();
        }

        self.accumulator += dt;
        while self.accumulator >= ONSET_STEP {
            self.accumulator -= ONSET_STEP;
            self.step(bass, flux);
        }
        self.state()
    }

    fn step(&mut self, bass: f32, flux: f32) {
        self.clock += ONSET_STEP;
        self.confidence *= (-ONSET_STEP / CONFIDENCE_DECAY_TAU).exp();
        self.pulse *= (-ONSET_STEP / BEAT_DECAY_TAU).exp();
        self.history.push_back(OnsetSample { bass, flux });
        while self.history.len() > HISTORY_FRAMES {
            self.history.pop_front();
        }

        let mut onset = false;
        if let Some((bass_median, bass_sigma, flux_median, flux_sigma)) = self.baseline() {
            let bass_threshold = bass_median + BASS_MARGIN.max(BASS_SIGMA * bass_sigma);
            let flux_threshold = flux_median + FLUX_MARGIN.max(FLUX_SIGMA * flux_sigma);
            let since_onset = self
                .last_onset
                .map_or(f32::INFINITY, |last| self.clock - last);

            if since_onset >= REFRACTORY {
                let bass_strength = ((bass - bass_threshold) / ONSET_RANGE).max(0.0);
                let flux_strength = ((flux - flux_threshold) / ONSET_RANGE).max(0.0);
                let strength = bass_strength.max(flux_strength);
                if strength > 0.0 {
                    onset = true;
                    let strength = strength.clamp(0.15, 1.0);
                    self.confidence = (self.confidence + CONFIDENCE_GAIN * strength).min(1.0);
                    self.pulse = self.pulse.max(strength);

                    if let Some(last) = self.last_onset {
                        let interval = (self.clock - last).clamp(MIN_PERIOD, MAX_PERIOD);
                        self.intervals.push_back(interval);
                        while self.intervals.len() > PERIOD_SAMPLES {
                            self.intervals.pop_front();
                        }
                        self.period =
                            median(self.intervals.iter().copied()).unwrap_or(DEFAULT_PERIOD);
                    }
                    self.last_onset = Some(self.clock);
                    self.phase = 0.0;
                }
            }
        }

        if !onset {
            self.phase = (self.phase + ONSET_STEP / self.period).fract();
        }
    }

    /// Медиана и σ Bass/flux последней секунды без последних 200 мс.
    fn baseline(&self) -> Option<(f32, f32, f32, f32)> {
        let length = self.history.len();
        if length < BASELINE_MIN + BASELINE_SKIP {
            return None;
        }
        let end = length - BASELINE_SKIP;
        let start = end.saturating_sub(BASELINE_FRAMES);
        let window = self.history.range(start..end);
        let (bass_median, bass_sigma) = median_sigma(window.clone().map(|sample| sample.bass));
        let (flux_median, flux_sigma) = median_sigma(window.map(|sample| sample.flux));
        Some((bass_median, bass_sigma, flux_median, flux_sigma))
    }

    fn state(&self) -> BeatState {
        BeatState {
            confidence: self.confidence,
            phase: self.phase,
            pulse: self.pulse,
            regularity: regularity(self.intervals.iter().copied()),
        }
    }
}

/// Скользящие средние признаков сигнала для Auto за 2–3 с.
#[derive(Debug, Clone, Copy, PartialEq)]
struct StyleFeatures {
    level: f32,
    flux: f32,
    bass_share: f32,
    mid_share: f32,
    high_share: f32,
    centroid: f32,
    flatness: f32,
}

impl Default for StyleFeatures {
    fn default() -> Self {
        Self {
            level: 0.0,
            flux: 0.0,
            bass_share: EQUAL_SHARES.0,
            mid_share: EQUAL_SHARES.1,
            high_share: EQUAL_SHARES.2,
            centroid: 0.5,
            flatness: 0.0,
        }
    }
}

impl StyleFeatures {
    fn update(&mut self, target: SpectrumTarget, dt: f32) {
        let coefficient = 1.0 - (-dt / FEATURE_TAU).exp();
        let flux = flux_norm(target.flux);
        let level = target.bands.map_or_else(
            || norm_db(target.rms_db),
            |bands| bands.bass.max(bands.mid).max(bands.high),
        );
        let (bass_share, mid_share, high_share) = match target.bands {
            Some(bands) => {
                let sum = (bands.bass + bands.mid + bands.high).max(1e-3);
                (
                    (bands.bass / sum).clamp(0.0, 1.0),
                    (bands.mid / sum).clamp(0.0, 1.0),
                    (bands.high / sum).clamp(0.0, 1.0),
                )
            }
            None => EQUAL_SHARES,
        };
        let centroid = if target.centroid_hz.is_finite() && target.centroid_hz > CENTROID_LOW_HZ {
            ((target.centroid_hz / CENTROID_LOW_HZ).ln()
                / (CENTROID_HIGH_HZ / CENTROID_LOW_HZ).ln())
            .clamp(0.0, 1.0)
        } else {
            0.5
        };
        let flatness = if target.flatness.is_finite() {
            target.flatness.clamp(0.0, 1.0)
        } else {
            0.0
        };

        self.level += (level.clamp(0.0, 1.0) - self.level) * coefficient;
        self.flux += (flux - self.flux) * coefficient;
        self.bass_share += (bass_share - self.bass_share) * coefficient;
        self.mid_share += (mid_share - self.mid_share) * coefficient;
        self.high_share += (high_share - self.high_share) * coefficient;
        self.centroid += (centroid - self.centroid) * coefficient;
        self.flatness += (flatness - self.flatness) * coefficient;
    }

    fn score(&self, style: MotionStyle, beat: BeatState, has_bands: bool) -> f32 {
        let quiet = 1.0 - self.level;
        let calm = 1.0 - self.flux;
        let soft = 0.55 * quiet + 0.25 * calm + 0.20 * (1.0 - self.max_share());
        if !has_bands {
            let groove = 0.35 * calm + 0.40 * beat.confidence + 0.25 * quiet;
            let ambient = 0.55 * calm + 0.30 * self.flatness + 0.15 * quiet;
            return match style {
                MotionStyle::GrooveSway => groove,
                MotionStyle::AmbientDrift => ambient,
                _ => soft,
            };
        }

        let groove =
            0.45 * self.mid_share + 0.25 * (1.0 - self.high_share) + 0.30 * beat.confidence;
        let bass_drop = 0.50 * self.bass_share + 0.35 * beat.confidence + 0.15 * self.flux;
        let ambient = 0.50 * calm + 0.30 * self.flatness + 0.20 * quiet;
        let sparkle = 0.40 * self.centroid + 0.35 * self.high_share + 0.25 * self.flux;
        let march = 0.40 * beat.confidence + 0.45 * beat.regularity + 0.15 * self.mid_share;
        match style {
            MotionStyle::SoftPulse => soft,
            MotionStyle::GrooveSway => groove,
            MotionStyle::BassDrop => bass_drop,
            MotionStyle::AmbientDrift => ambient,
            MotionStyle::SparkleTwitch => sparkle,
            MotionStyle::BeatMarch => march,
            MotionStyle::Auto => soft,
        }
    }

    fn max_share(&self) -> f32 {
        self.bass_share.max(self.mid_share).max(self.high_share)
    }
}

/// Стиль с наибольшей оценкой; при равенстве выигрывает более ранний.
fn pick_style(features: &StyleFeatures, beat: BeatState, has_bands: bool) -> MotionStyle {
    const CANDIDATES: [MotionStyle; 6] = [
        MotionStyle::SoftPulse,
        MotionStyle::GrooveSway,
        MotionStyle::BassDrop,
        MotionStyle::AmbientDrift,
        MotionStyle::SparkleTwitch,
        MotionStyle::BeatMarch,
    ];

    let mut best = MotionStyle::SoftPulse;
    let mut best_score = f32::NEG_INFINITY;
    for style in CANDIDATES {
        if !has_bands && style.requires_bands() {
            continue;
        }
        let score = features.score(style, beat, has_bands);
        if score > best_score {
            best = style;
            best_score = score;
        }
    }
    best
}

/// Микшер движения: история огибающих, onset/beat, стиль, пружина и дыхание.
///
/// Живёт в presentation-слое рядом с визуализатором одного показа окна.
#[derive(Debug)]
pub struct MotionMixer {
    squash: f32,
    velocity: f32,
    bass_env: f32,
    high_env: f32,
    /// Вес аудио-вклада: 1 при живом спектре, плавно гаснет после Stop.
    release: f32,
    idle_phase: f32,
    ripple_phase: f32,
    drift_phase: f32,
    time: f32,
    detector: OnsetDetector,
    features: StyleFeatures,
    director: StyleDirector,
}

impl Default for MotionMixer {
    fn default() -> Self {
        Self {
            squash: 1.0,
            velocity: 0.0,
            bass_env: 0.0,
            high_env: 0.0,
            release: 0.0,
            idle_phase: 0.0,
            ripple_phase: 0.0,
            drift_phase: 0.0,
            time: 0.0,
            detector: OnsetDetector::default(),
            features: StyleFeatures::default(),
            director: StyleDirector::default(),
        }
    }
}

impl MotionMixer {
    /// Обновляет движение на кадр. `target = None` — нет эфира: возврат в idle.
    ///
    /// `reactivity` — уже эффективная реактивность с поправкой на режим окна.
    pub fn update(
        &mut self,
        target: Option<SpectrumTarget>,
        reactivity: MotionReactivity,
        style: MotionStyle,
        dt: f32,
    ) -> GlobeMotion {
        if !reactivity.is_enabled() {
            *self = Self::default();
            return GlobeMotion::disabled(style);
        }

        let dt = if dt.is_finite() {
            dt.clamp(0.0, MAX_DT)
        } else {
            0.0
        };
        if dt > 0.0 {
            let (bass, high) = energies(target);
            self.bass_env = follow(self.bass_env, bass, dt, ATTACK_TAU, BAND_RELEASE_TAU);
            self.high_env = follow(self.high_env, high, dt, ATTACK_TAU, BAND_RELEASE_TAU);

            let (gain_target, tau) = if target.is_some() {
                (1.0, RELEASE_RISE_TAU)
            } else {
                (0.0, RELEASE_TAU)
            };
            self.release = follow(self.release, gain_target, dt, tau, tau);

            let flux = target.map_or(0.0, |target| flux_norm(target.flux));
            let beat = self
                .detector
                .update(target.is_some(), self.bass_env, flux, dt);
            let has_bands = target.is_some_and(|target| target.bands.is_some());
            if let Some(target) = target {
                self.features.update(target, dt);
            } else {
                self.features = StyleFeatures::default();
            }
            let auto_pick = target.map(|_| pick_style(&self.features, beat, has_bands));
            let (style, params) = self.director.update(style, auto_pick, has_bands, dt);

            self.time += dt;
            self.idle_phase = (self.idle_phase + dt * std::f32::consts::TAU * IDLE_BREATH_HZ)
                .rem_euclid(std::f32::consts::TAU);
            self.drift_phase = (self.drift_phase + dt * std::f32::consts::TAU * DRIFT_HZ)
                .rem_euclid(std::f32::consts::TAU);
            self.ripple_phase = (self.ripple_phase
                + dt * std::f32::consts::TAU
                    * RIPPLE_RATE
                    * (0.6 + self.high_env * (1.0 + params.twitch * 1.5)))
                .rem_euclid(std::f32::consts::TAU);

            let amount = reactivity.squash_limit();
            let gate = beat_gate(beat.confidence);
            let beat_sway = (beat.phase * std::f32::consts::TAU).sin() * gate;
            let sway = (beat_sway + params.drift * self.drift_phase.sin()).clamp(-1.0, 1.0);
            let beat_term = params.beat * beat.pulse * gate;
            let audio =
                (amount * (params.squash * self.bass_env + 0.6 * beat_term)).clamp(0.0, amount);
            let idle = reactivity.idle_amplitude() * self.idle_phase.sin() * (1.0 - self.release);
            let drift = params.drift * amount * 0.4 * self.drift_phase.sin() * self.release;
            let target_scale = (1.0 - audio + idle + drift).clamp(1.0 - amount, 1.0 + amount);
            self.integrate(target_scale, dt);
            self.squash = self.squash.clamp(1.0 - amount, 1.0 + amount);

            let ripple_limit = reactivity.ripple_limit();
            let ripple_signal = params.ripple * self.high_env
                + params.drift * 0.3 * (0.5 + 0.5 * self.drift_phase.sin());
            let ripple = (ripple_limit * self.release * ripple_signal).clamp(0.0, ripple_limit);
            let flash = (params.flash * beat.pulse * gate * self.release).clamp(0.0, 0.15);

            return GlobeMotion {
                deform: GlobeDeform::new(self.squash),
                ripple,
                ripple_phase: self.ripple_phase,
                yaw_sway: params.sway * sway,
                roll: 0.0,
                flash,
                beat_phase: beat.phase,
                time: self.time,
                style,
                intensity: self.bass_env.max(self.high_env) * self.release,
            };
        }

        GlobeMotion {
            deform: GlobeDeform::new(self.squash),
            ripple: 0.0,
            ripple_phase: self.ripple_phase,
            yaw_sway: 0.0,
            roll: 0.0,
            flash: 0.0,
            beat_phase: 0.0,
            time: self.time,
            style,
            intensity: 0.0,
        }
    }

    /// Полунеявная пружина фиксированным шагом: сон окна не выбрасывает
    /// планету в крайнее положение.
    fn integrate(&mut self, target: f32, dt: f32) {
        let mut remaining = dt;
        while remaining > 0.0 {
            let step = remaining.min(MAX_STEP);
            let acceleration =
                SQUASH_STIFFNESS * (target - self.squash) - SQUASH_DAMPING * self.velocity;
            self.velocity += acceleration * step;
            self.squash += self.velocity * step;
            remaining -= step;
        }
    }
}

/// Нормированные Bass/High. Без полос бас — полный RMS, верх — flux.
fn energies(target: Option<SpectrumTarget>) -> (f32, f32) {
    let Some(target) = target else {
        return (0.0, 0.0);
    };
    match target.bands {
        Some(bands) => (bands.bass, bands.high),
        None => (norm_db(target.rms_db), flux_norm(target.flux)),
    }
}

fn follow(current: f32, target: f32, dt: f32, attack_tau: f32, release_tau: f32) -> f32 {
    let tau = if target > current {
        attack_tau
    } else {
        release_tau
    };
    let coefficient = 1.0 - (-dt / tau).exp();
    (current + (target - current) * coefficient).clamp(0.0, 1.0)
}

fn norm_db(db: f32) -> f32 {
    ((db - SILENCE_LEVEL) / SILENCE_RANGE).clamp(0.0, 1.0)
}

fn flux_norm(flux: f32) -> f32 {
    (flux.max(0.0) * 500.0).sqrt().clamp(0.0, 1.0)
}

/// Готовность фазы бита: ниже 0,2 не применяется, выше 0,5 — полностью.
fn beat_gate(confidence: f32) -> f32 {
    ((confidence - CONFIDENCE_GATE_LOW) / (CONFIDENCE_GATE_HIGH - CONFIDENCE_GATE_LOW))
        .clamp(0.0, 1.0)
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Медиана и σ небольшой выборки без аллокаций.
fn median_sigma<I: Iterator<Item = f32>>(values: I) -> (f32, f32) {
    let mut samples = [0.0f32; BASELINE_FRAMES];
    let mut length = 0;
    for value in values {
        if length < BASELINE_FRAMES {
            samples[length] = value;
            length += 1;
        }
    }
    median_of(&mut samples[..length]).map_or((0.0, 0.0), |median| {
        let mean = samples[..length].iter().sum::<f32>() / length as f32;
        let variance = samples[..length]
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f32>()
            / length as f32;
        (median, variance.sqrt())
    })
}

/// Медиана до 4 интервалов бита.
fn median<I: Iterator<Item = f32>>(values: I) -> Option<f32> {
    let mut samples = [0.0f32; PERIOD_SAMPLES];
    let mut length = 0;
    for value in values {
        if length < PERIOD_SAMPLES {
            samples[length] = value;
            length += 1;
        }
    }
    median_of(&mut samples[..length])
}

fn median_of(values: &mut [f32]) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let middle = values.len() / 2;
    Some(if values.len() % 2 == 1 {
        values[middle]
    } else {
        (values[middle - 1] + values[middle]) * 0.5
    })
}

/// Регулярность интервалов 0..1: 1 — идеально ровный бит.
fn regularity<I: Iterator<Item = f32>>(values: I) -> f32 {
    let mut samples = [0.0f32; PERIOD_SAMPLES];
    let mut length = 0;
    for value in values {
        if length < PERIOD_SAMPLES {
            samples[length] = value;
            length += 1;
        }
    }
    if length < 2 {
        return 0.0;
    }
    let mean = samples[..length].iter().sum::<f32>() / length as f32;
    if mean <= f32::EPSILON {
        return 0.0;
    }
    let variance = samples[..length]
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f32>()
        / length as f32;
    (1.0 - variance.sqrt() / mean).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::BandTargets;

    fn loud(bass: f32) -> SpectrumTarget {
        SpectrumTarget {
            centroid_hz: 1_500.0,
            spread_hz: 1_200.0,
            flux: 0.002,
            flatness: 0.3,
            rms_db: -12.0,
            bands: Some(BandTargets {
                bass,
                mid: 0.5,
                high: 0.6,
            }),
        }
    }

    fn pulse(bass: f32) -> SpectrumTarget {
        SpectrumTarget {
            bands: Some(BandTargets {
                bass,
                mid: 0.3,
                high: 0.2,
            }),
            ..loud(bass)
        }
    }

    fn run(
        mixer: &mut MotionMixer,
        target: Option<SpectrumTarget>,
        reactivity: MotionReactivity,
        style: MotionStyle,
        seconds: f32,
    ) -> GlobeMotion {
        let mut motion = GlobeMotion::default();
        let frames = (seconds * 30.0).round() as usize;
        for _ in 0..frames {
            motion = mixer.update(target, reactivity, style, 1.0 / 30.0);
        }
        motion
    }

    #[test]
    fn off_is_identity_and_still() {
        let mut mixer = MotionMixer::default();
        run(
            &mut mixer,
            Some(loud(1.0)),
            MotionReactivity::Normal,
            MotionStyle::Auto,
            1.0,
        );

        let motion = mixer.update(
            Some(loud(1.0)),
            MotionReactivity::Off,
            MotionStyle::Auto,
            0.033,
        );
        assert_eq!(motion.deform, GlobeDeform::IDENTITY);
        assert_eq!(motion.intensity, 0.0);
        assert_eq!(motion.ripple, 0.0);
        assert_eq!(motion.flash, 0.0);
        assert_eq!(motion.yaw_sway, 0.0);
    }

    #[test]
    fn squash_stays_within_reactivity_limit() {
        for (reactivity, limit) in [
            (MotionReactivity::Low, 0.010),
            (MotionReactivity::Normal, 0.020),
            (MotionReactivity::High, 0.035),
        ] {
            let mut mixer = MotionMixer::default();
            let motion = run(
                &mut mixer,
                Some(loud(1.0)),
                reactivity,
                MotionStyle::Auto,
                2.0,
            );
            let scale_y = motion.deform.scale_y();

            assert!(scale_y >= 1.0 - limit - 1e-5, "{reactivity:?}: {scale_y}");
            assert!(
                scale_y < 1.0 - limit * 0.6,
                "{reactivity:?} должен заметно отзываться: {scale_y}"
            );
        }
    }

    #[test]
    fn squash_preserves_volume() {
        let mut mixer = MotionMixer::default();
        let motion = run(
            &mut mixer,
            Some(loud(1.0)),
            MotionReactivity::Normal,
            MotionStyle::Auto,
            1.0,
        );
        let deform = motion.deform;

        assert!((deform.scale_y() * deform.scale_xz() * deform.scale_xz() - 1.0).abs() < 1e-5);
        assert!(
            deform.scale_xz() > 1.0,
            "планета расширяется по горизонтали"
        );
    }

    #[test]
    fn release_reaches_idle_within_half_a_second() {
        let mut mixer = MotionMixer::default();
        let active = run(
            &mut mixer,
            Some(loud(1.0)),
            MotionReactivity::Normal,
            MotionStyle::Auto,
            2.0,
        );
        let initial = active.intensity;
        assert!(initial > 0.5, "сигнал должен быть активным: {initial}");

        let after_300 = run(
            &mut mixer,
            None,
            MotionReactivity::Normal,
            MotionStyle::Auto,
            0.3,
        );
        assert!(
            after_300.intensity <= initial * 0.06,
            "через 300 мс остаётся около 5 %: {}",
            after_300.intensity / initial
        );

        let after_500 = run(
            &mut mixer,
            None,
            MotionReactivity::Normal,
            MotionStyle::Auto,
            0.2,
        );
        assert!(
            after_500.intensity <= initial * 0.011,
            "через 500 мс остаётся меньше 1 %: {}",
            after_500.intensity / initial
        );
        assert_eq!(after_500.flash, 0.0);
    }

    #[test]
    fn idle_breathing_is_small_and_slow() {
        let mut mixer = MotionMixer::default();
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for _ in 0..(10.0 * 30.0) as usize {
            let motion = mixer.update(
                None,
                MotionReactivity::Normal,
                MotionStyle::Auto,
                1.0 / 30.0,
            );
            let scale_y = motion.deform.scale_y();
            min = min.min(scale_y);
            max = max.max(scale_y);
        }

        assert!(
            max - min > 0.004,
            "дыхание должно быть заметно: {}",
            max - min
        );
        assert!(
            max - min < 0.008,
            "дыхание не должно раздувать планету: {}",
            max - min
        );
        assert!((max - 1.0) < 0.0040 && (1.0 - min) < 0.0040);
    }

    #[test]
    fn long_frame_gap_does_not_throw_the_planet() {
        let mut mixer = MotionMixer::default();
        let motion = mixer.update(
            Some(loud(1.0)),
            MotionReactivity::High,
            MotionStyle::Auto,
            10.0,
        );

        assert!(motion.deform.scale_y() <= 1.035 + 1e-5);
        assert!(motion.deform.scale_y().is_finite());
        assert!(motion.flash <= 0.15);
    }

    #[test]
    fn ripple_follows_high_band_and_its_limit() {
        let mut mixer = MotionMixer::default();
        let motion = run(
            &mut mixer,
            Some(loud(1.0)),
            MotionReactivity::High,
            MotionStyle::SparkleTwitch,
            1.0,
        );

        assert!(motion.ripple > 0.0);
        assert!(motion.ripple <= 0.004 + 1e-6);
        assert!(motion.ripple_phase >= 0.0 && motion.ripple_phase < std::f32::consts::TAU);
    }

    #[test]
    fn without_bands_motion_uses_rms_and_flux() {
        let mut mixer = MotionMixer::default();
        let target = SpectrumTarget {
            bands: None,
            rms_db: -12.0,
            flux: 0.002,
            ..loud(0.0)
        };
        let motion = run(
            &mut mixer,
            Some(target),
            MotionReactivity::Normal,
            MotionStyle::Auto,
            1.5,
        );

        assert!(motion.deform.scale_y() < 0.99);
        assert!(motion.intensity > 0.5);
    }

    #[test]
    fn deform_roundtrip_returns_to_unit_sphere() {
        let deform = GlobeDeform::new(0.97);
        let unit = Vec3::new(0.36, -0.48, 0.8).normalize();
        let back = deform.to_unit(deform.to_world(unit));

        assert!((back - unit).length() < 1e-5);
        assert_ne!(deform, GlobeDeform::IDENTITY);
        assert_eq!(GlobeDeform::IDENTITY, GlobeDeform::default());
    }

    #[test]
    fn uniform_values_match_deform_and_ripple() {
        let motion = GlobeMotion {
            deform: GlobeDeform::new(0.98),
            ripple: 0.002,
            ripple_phase: 1.25,
            ..GlobeMotion::default()
        };
        let values = motion.uniform_values();

        assert!((values[0] + 0.02).abs() < 1e-6);
        assert!((values[1] - (1.0 / 0.98f32.sqrt() - 1.0)).abs() < 1e-6);
        assert_eq!(values[2], 0.002);
        assert_eq!(values[3], 1.25);
    }

    #[test]
    fn uniform2_values_match_flash_style_and_phase() {
        let motion = GlobeMotion {
            flash: 0.08,
            style: MotionStyle::BassDrop,
            beat_phase: 0.25,
            time: 12.5,
            ..GlobeMotion::default()
        };

        assert_eq!(motion.uniform2_values(), [0.08, 3.0, 0.25, 12.5]);
    }

    #[test]
    fn detector_tracks_regular_onsets() {
        let mut detector = OnsetDetector::default();
        let mut state = BeatState::default();
        for step in 0..(4.0 / ONSET_STEP) as usize {
            let bass = if step % 10 == 0 { 1.0 } else { 0.08 };
            state = detector.update(true, bass, 0.1, ONSET_STEP);
        }

        assert!(state.confidence > 0.5, "{}", state.confidence);
        assert!(state.regularity > 0.85, "{}", state.regularity);
        assert!((detector.period - 0.5).abs() < 1e-3, "{}", detector.period);
        assert!(state.phase >= 0.0 && state.phase < 1.0);
    }

    #[test]
    fn detector_rejects_rapid_retriggers_and_resets_on_stop() {
        let mut detector = OnsetDetector::default();
        for _ in 0..10 {
            detector.update(true, 0.08, 0.1, ONSET_STEP);
        }
        let first = detector.update(true, 1.0, 0.1, ONSET_STEP);
        assert!(first.pulse > 0.0);
        let second = detector.update(true, 1.0, 0.1, ONSET_STEP);
        assert!(second.pulse > first.pulse * 0.5);

        let stopped = detector.update(false, 1.0, 0.1, ONSET_STEP);
        assert_eq!(stopped, BeatState::default());
        assert!(detector.history.is_empty());
    }

    #[test]
    fn confidence_gates_beat_phase_and_flash() {
        assert_eq!(beat_gate(0.1), 0.0);
        assert_eq!(beat_gate(0.5), 1.0);
        assert!(beat_gate(0.35) > 0.0 && beat_gate(0.35) < 1.0);
    }

    #[test]
    fn auto_style_matches_signal_character() {
        let quiet = StyleFeatures {
            level: 0.05,
            flux: 0.03,
            flatness: 0.9,
            centroid: 0.3,
            ..StyleFeatures::default()
        };
        assert_eq!(
            pick_style(&quiet, BeatState::default(), false),
            MotionStyle::AmbientDrift
        );

        let sparkle = StyleFeatures {
            level: 0.7,
            flux: 0.85,
            bass_share: 0.15,
            mid_share: 0.25,
            high_share: 0.60,
            centroid: 0.9,
            flatness: 0.2,
        };
        let beat = BeatState {
            confidence: 0.6,
            ..BeatState::default()
        };
        assert_eq!(pick_style(&sparkle, beat, true), MotionStyle::SparkleTwitch);

        let bass_heavy = StyleFeatures {
            level: 0.9,
            flux: 0.6,
            bass_share: 0.75,
            mid_share: 0.15,
            high_share: 0.10,
            centroid: 0.35,
            flatness: 0.2,
        };
        let beat = BeatState {
            confidence: 0.8,
            regularity: 0.4,
            ..BeatState::default()
        };
        assert_eq!(pick_style(&bass_heavy, beat, true), MotionStyle::BassDrop);

        let regular = StyleFeatures {
            level: 0.7,
            flux: 0.45,
            bass_share: 0.30,
            mid_share: 0.45,
            high_share: 0.25,
            centroid: 0.45,
            flatness: 0.2,
        };
        let beat = BeatState {
            confidence: 0.9,
            regularity: 0.95,
            ..BeatState::default()
        };
        assert_eq!(pick_style(&regular, beat, true), MotionStyle::BeatMarch);
    }

    #[test]
    fn auto_without_bands_never_picks_band_only_styles() {
        let bass_heavy = StyleFeatures {
            level: 0.9,
            flux: 0.6,
            bass_share: 0.8,
            mid_share: 0.1,
            high_share: 0.1,
            ..StyleFeatures::default()
        };
        let beat = BeatState {
            confidence: 1.0,
            regularity: 1.0,
            ..BeatState::default()
        };

        for _ in 0..6 {
            let picked = pick_style(&bass_heavy, beat, false);
            assert!(!picked.requires_bands(), "{picked:?}");
        }
    }

    #[test]
    fn auto_style_switches_after_hold_and_crossfades() {
        let mut director = StyleDirector::default();
        let dt = 1.0 / 30.0;
        let mut params = StyleParams::SOFT_PULSE;

        for _ in 0..(2.9 * 30.0) as usize {
            let (_, next) =
                director.update(MotionStyle::Auto, Some(MotionStyle::BassDrop), true, dt);
            params = next;
        }
        assert_eq!(director.target, MotionStyle::SoftPulse);
        assert!((params.squash - 1.0).abs() < 1e-6);

        for _ in 0..(0.2 * 30.0) as usize {
            let (_, next) =
                director.update(MotionStyle::Auto, Some(MotionStyle::BassDrop), true, dt);
            params = next;
        }
        assert_eq!(director.target, MotionStyle::BassDrop);
        assert!(params.squash > 1.0 && params.squash < 1.25);

        for _ in 0..(1.1 * 30.0) as usize {
            let (_, next) =
                director.update(MotionStyle::Auto, Some(MotionStyle::BassDrop), true, dt);
            params = next;
        }
        assert!((params.squash - 1.25).abs() < 1e-5);
    }

    #[test]
    fn manual_style_without_bands_falls_back_to_soft_pulse() {
        let mut director = StyleDirector::default();
        let (style, _) = director.update(MotionStyle::BassDrop, None, false, 1.0 / 30.0);
        assert_eq!(style, MotionStyle::SoftPulse);

        let (style, _) = director.update(MotionStyle::BassDrop, None, true, 1.0 / 30.0);
        assert_eq!(style, MotionStyle::BassDrop);
    }

    #[test]
    fn bass_drop_flashes_and_groove_sways_within_limits() {
        let mut flashes = MotionMixer::default();
        let mut swinger = MotionMixer::default();
        let mut max_flash: f32 = 0.0;
        let mut max_sway: f32 = 0.0;

        for frame in 0..(4.0 * 30.0) as usize {
            let bass = if frame % 15 < 3 { 1.0 } else { 0.08 };
            let target = pulse(bass);
            let flash = flashes.update(
                Some(target),
                MotionReactivity::High,
                MotionStyle::BassDrop,
                1.0 / 30.0,
            );
            let sway = swinger.update(
                Some(target),
                MotionReactivity::High,
                MotionStyle::GrooveSway,
                1.0 / 30.0,
            );
            max_flash = max_flash.max(flash.flash);
            max_sway = max_sway.max(sway.yaw_sway.abs());
        }

        assert!(max_flash > 0.02, "flash должен срабатывать: {max_flash}");
        assert!(max_flash <= 0.15, "flash ограничен: {max_flash}");
        assert!(max_sway > 0.004, "качка должна быть заметна: {max_sway}");
        assert!(max_sway <= 0.015 + 1e-6, "качка ограничена: {max_sway}");
    }

    #[test]
    fn onset_pulse_moves_squash_and_decays() {
        let mut mixer = MotionMixer::default();
        let mut motion = GlobeMotion::default();
        let mut min_scale = f32::INFINITY;
        for frame in 0..(3.0 * 30.0) as usize {
            let bass = if frame % 15 < 2 { 1.0 } else { 0.08 };
            motion = mixer.update(
                Some(pulse(bass)),
                MotionReactivity::Normal,
                MotionStyle::BeatMarch,
                1.0 / 30.0,
            );
            min_scale = min_scale.min(motion.deform.scale_y());
        }

        assert!(
            min_scale < 1.0 - 0.01,
            "бит должен сжимать планету: {min_scale}"
        );
        assert!(motion.intensity > 0.0);
        assert_eq!(motion.uniform2_values()[1], MotionStyle::BeatMarch.id());
    }

    #[test]
    fn sway_stays_zero_for_sparkle_twitch() {
        let mut mixer = MotionMixer::default();
        let mut motion = GlobeMotion::default();
        for frame in 0..(3.0 * 30.0) as usize {
            let bass = if frame % 15 < 2 { 1.0 } else { 0.08 };
            motion = mixer.update(
                Some(pulse(bass)),
                MotionReactivity::High,
                MotionStyle::SparkleTwitch,
                1.0 / 30.0,
            );
        }

        assert_eq!(motion.yaw_sway, 0.0);
    }
}
