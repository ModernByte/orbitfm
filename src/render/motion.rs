//! Аудио-реактивное движение глобуса (ADR-0028): spring-squash, idle-дыхание
//! и общая деформация для меша, маркеров, стран и hit-test. Отрисовки здесь нет.

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
const SILENCE_LEVEL: f32 = -60.0;
const SILENCE_RANGE: f32 = 60.0;

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
    /// Музыкальная добавка к yaw; в Phase 1 равна нулю.
    pub yaw_sway: f32,
    /// Переворот камеры; в Phase 1 равен нулю.
    pub roll: f32,
    /// Усиление Fresnel-кромки, 0..=0.15; в Phase 1 равно нулю.
    pub flash: f32,
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
}

/// Микшер движения: история огибающих, пружина и idle-дыхание.
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
            self.squash = 1.0;
            self.velocity = 0.0;
            self.bass_env = 0.0;
            self.high_env = 0.0;
            self.release = 0.0;
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

            self.idle_phase = (self.idle_phase + dt * std::f32::consts::TAU * IDLE_BREATH_HZ)
                .rem_euclid(std::f32::consts::TAU);
            self.ripple_phase = (self.ripple_phase
                + dt * std::f32::consts::TAU * RIPPLE_RATE * (0.6 + self.high_env))
                .rem_euclid(std::f32::consts::TAU);

            let amount = reactivity.squash_limit();
            let audio = amount * self.bass_env * self.release;
            let idle = reactivity.idle_amplitude() * self.idle_phase.sin() * (1.0 - self.release);
            self.integrate(1.0 - audio + idle, dt);
            self.squash = self.squash.clamp(1.0 - amount, 1.0 + amount);
        }

        GlobeMotion {
            deform: GlobeDeform::new(self.squash),
            ripple: reactivity.ripple_limit() * self.high_env * self.release,
            ripple_phase: self.ripple_phase,
            yaw_sway: 0.0,
            roll: 0.0,
            flash: 0.0,
            style,
            intensity: self.bass_env.max(self.high_env) * self.release,
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
    let flux = (target.flux.max(0.0) * 500.0).sqrt().clamp(0.0, 1.0);
    match target.bands {
        Some(bands) => (bands.bass, bands.high),
        None => {
            let rms = ((target.rms_db - SILENCE_LEVEL) / SILENCE_RANGE).clamp(0.0, 1.0);
            (rms, flux)
        }
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

    fn run(
        mixer: &mut MotionMixer,
        target: Option<SpectrumTarget>,
        reactivity: MotionReactivity,
        seconds: f32,
    ) -> GlobeMotion {
        let mut motion = GlobeMotion::default();
        let frames = (seconds * 30.0).round() as usize;
        for _ in 0..frames {
            motion = mixer.update(target, reactivity, MotionStyle::Auto, 1.0 / 30.0);
        }
        motion
    }

    #[test]
    fn off_is_identity_and_still() {
        let mut mixer = MotionMixer::default();
        run(&mut mixer, Some(loud(1.0)), MotionReactivity::Normal, 1.0);

        let motion = mixer.update(
            Some(loud(1.0)),
            MotionReactivity::Off,
            MotionStyle::Auto,
            0.033,
        );
        assert_eq!(motion.deform, GlobeDeform::IDENTITY);
        assert_eq!(motion.intensity, 0.0);
        assert_eq!(motion.ripple, 0.0);
    }

    #[test]
    fn squash_stays_within_reactivity_limit() {
        for (reactivity, limit) in [
            (MotionReactivity::Low, 0.010),
            (MotionReactivity::Normal, 0.020),
            (MotionReactivity::High, 0.035),
        ] {
            let mut mixer = MotionMixer::default();
            let motion = run(&mut mixer, Some(loud(1.0)), reactivity, 2.0);
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
        let motion = run(&mut mixer, Some(loud(1.0)), MotionReactivity::Normal, 1.0);
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
        let active = run(&mut mixer, Some(loud(1.0)), MotionReactivity::Normal, 2.0);
        let initial = active.intensity;
        assert!(initial > 0.5, "сигнал должен быть активным: {initial}");

        let after_300 = run(&mut mixer, None, MotionReactivity::Normal, 0.3);
        assert!(
            after_300.intensity <= initial * 0.06,
            "через 300 мс остаётся около 5 %: {}",
            after_300.intensity / initial
        );

        let after_500 = run(&mut mixer, None, MotionReactivity::Normal, 0.2);
        assert!(
            after_500.intensity <= initial * 0.011,
            "через 500 мс остаётся меньше 1 %: {}",
            after_500.intensity / initial
        );
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
    }

    #[test]
    fn ripple_follows_high_band_and_its_limit() {
        let mut mixer = MotionMixer::default();
        let motion = run(&mut mixer, Some(loud(1.0)), MotionReactivity::High, 1.0);

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
        let motion = run(&mut mixer, Some(target), MotionReactivity::Normal, 1.5);

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
}
