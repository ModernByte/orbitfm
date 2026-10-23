use eframe::egui::{Color32, FontId, Mesh, Painter, Rect, Shape, Stroke, pos2};

const MIN_FREQ_HZ: f32 = 30.0;
const MAX_FREQ_HZ: f32 = 20_000.0;
const MIN_RMS_DB: f32 = -60.0;
const RMS_RANGE_DB: f32 = 60.0;
const POINTS: usize = 96;

/// Целевые значения визуализатора, уже не зависящие от аудио-модуля.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpectrumTarget {
    pub centroid_hz: f32,
    pub spread_hz: f32,
    pub flux: f32,
    pub flatness: f32,
    pub rms_db: f32,
}

/// Сглаженное состояние и отрисовка экспериментальной спектральной огибающей.
#[derive(Debug, Clone, Copy, Default)]
pub struct SpectrumOverlay {
    level: f32,
    centroid_log: f32,
    spread_log: f32,
    flux: f32,
    flatness: f32,
    rms_db: f32,
    initialized: bool,
}

impl SpectrumOverlay {
    /// Обновляет сглаженные значения; `None` запускает затухание.
    pub fn update(&mut self, target: Option<SpectrumTarget>, dt: f32) {
        let dt = dt.clamp(0.0, 0.1);

        let Some(target) = target else {
            self.level *= (-dt * 4.0).exp();
            if self.level < 0.005 {
                self.level = 0.0;
                self.initialized = false;
            }
            return;
        };

        let target = normalize(target);
        if !self.initialized {
            *self = target;
            return;
        }

        let slow = 1.0 - (-dt * 6.0).exp();
        let fast = 1.0 - (-dt * 10.0).exp();
        self.level += (target.level - self.level) * fast;
        self.centroid_log += (target.centroid_log - self.centroid_log) * slow;
        self.spread_log += (target.spread_log - self.spread_log) * slow;
        self.flux += (target.flux - self.flux) * fast;
        self.flatness += (target.flatness - self.flatness) * slow;
        self.rms_db += (target.rms_db - self.rms_db) * slow;
    }

    /// Есть ли что рисовать.
    pub fn is_visible(&self) -> bool {
        self.level > 0.005
    }

    /// Сигнал эфира 0..1 для Fresnel-обода: сглаженные RMS и flux.
    ///
    /// Без данных возвращает 0, поэтому атмосфера глобуса затухает
    /// вместе с оверлеем.
    pub fn pulse(&self) -> f32 {
        if !self.is_visible() {
            return 0.0;
        }
        (self.level * 0.75 + self.flux * 0.25).clamp(0.0, 1.0)
    }

    /// Рисует полупрозрачную спектральную ленту в переданном прямоугольнике.
    pub fn paint(&self, painter: &Painter, rect: Rect, time: f64) {
        if !self.is_visible() || rect.width() < 40.0 || rect.height() < 24.0 {
            return;
        }

        painter.rect_filled(rect, 12.0, Color32::from_rgba_unmultiplied(5, 8, 16, 110));
        painter.rect_stroke(
            rect,
            12.0,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 14)),
            eframe::egui::StrokeKind::Inside,
        );

        let baseline = rect.bottom() - 16.0;
        let height = (rect.height() - 24.0).max(8.0);
        let min_log = MIN_FREQ_HZ.log10();
        let max_log = MAX_FREQ_HZ.log10();
        let alpha = (40.0 + 160.0 * self.level).clamp(0.0, 255.0) as u8;

        let mut top = Vec::with_capacity(POINTS);
        let mut bottom = Vec::with_capacity(POINTS);
        for index in 0..POINTS {
            let t = index as f32 / (POINTS - 1) as f32;
            let log_freq = min_log + (max_log - min_log) * t;
            let z = (log_freq - self.centroid_log) / self.spread_log.max(0.05);
            let envelope = (-0.5 * z * z).exp();
            let ripple = 1.0 + 0.08 * (t * 41.0 + time as f32 * 2.2).sin() * self.flux;
            let amplitude =
                (self.level * envelope * (0.65 + 0.35 * self.flatness) * ripple).clamp(0.0, 1.0);

            let x = rect.left() + rect.width() * t;
            top.push(pos2(x, baseline - amplitude * height));
            bottom.push(pos2(x, baseline));
        }

        let mut mesh = Mesh::default();
        for (index, (&top_point, &bottom_point)) in top.iter().zip(&bottom).enumerate() {
            let t = index as f32 / (POINTS - 1) as f32;
            mesh.colored_vertex(top_point, gradient(t, alpha));
            mesh.colored_vertex(bottom_point, Color32::TRANSPARENT);
        }
        for index in 0..POINTS as u32 - 1 {
            let top_a = index * 2;
            let bottom_a = top_a + 1;
            let top_b = top_a + 2;
            let bottom_b = top_a + 3;
            mesh.add_triangle(top_a, bottom_a, top_b);
            mesh.add_triangle(top_b, bottom_a, bottom_b);
        }
        painter.add(Shape::mesh(mesh));

        let line_color = gradient(self.centroid_ratio(), 230);
        painter.add(Shape::line(
            top.clone(),
            Stroke::new(5.0, line_color.gamma_multiply(0.12)),
        ));
        painter.add(Shape::line(top, Stroke::new(1.4, line_color)));
        painter.line_segment(
            [
                pos2(rect.left() + 8.0, baseline),
                pos2(rect.right() - 8.0, baseline),
            ],
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 18)),
        );

        let centroid_hz = 10f32.powf(self.centroid_log);
        let frequency = if centroid_hz >= 1000.0 {
            format!("{:.1} кГц", centroid_hz / 1000.0)
        } else {
            format!("{centroid_hz:.0} Гц")
        };
        painter.text(
            pos2(rect.left() + 10.0, baseline + 2.0),
            eframe::egui::Align2::LEFT_TOP,
            format!("FFT · {frequency} · {:.0} дБ", self.rms_db),
            FontId::monospace(10.0),
            Color32::from_rgba_unmultiplied(185, 200, 225, 190),
        );
    }

    fn centroid_ratio(&self) -> f32 {
        ((self.centroid_log - MIN_FREQ_HZ.log10()) / (MAX_FREQ_HZ.log10() - MIN_FREQ_HZ.log10()))
            .clamp(0.0, 1.0)
    }
}

fn normalize(target: SpectrumTarget) -> SpectrumOverlay {
    let centroid = target.centroid_hz.clamp(MIN_FREQ_HZ, MAX_FREQ_HZ);
    let upper = (target.centroid_hz + target.spread_hz).clamp(MIN_FREQ_HZ + 1.0, MAX_FREQ_HZ * 2.0);

    SpectrumOverlay {
        level: ((target.rms_db - MIN_RMS_DB) / RMS_RANGE_DB).clamp(0.0, 1.0),
        centroid_log: centroid.log10(),
        spread_log: (upper.log10() - centroid.log10()).clamp(0.02, 1.3),
        flux: (target.flux.max(0.0) * 500.0).sqrt().clamp(0.0, 1.0),
        flatness: target.flatness.clamp(0.0, 1.0),
        rms_db: target.rms_db,
        initialized: true,
    }
}

fn gradient(t: f32, alpha: u8) -> Color32 {
    let low = Color32::from_rgba_unmultiplied(88, 148, 255, alpha);
    let high = Color32::from_rgba_unmultiplied(92, 232, 208, alpha);
    mix(low, high, t)
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let blend = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t) as u8;
    Color32::from_rgba_unmultiplied(
        blend(a.r(), b.r()),
        blend(a.g(), b.g()),
        blend(a.b(), b.b()),
        blend(a.a(), b.a()),
    )
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
        }
    }

    #[test]
    fn snaps_to_first_frame() {
        let mut overlay = SpectrumOverlay::default();
        overlay.update(Some(target(-20.0)), 0.016);

        assert!(overlay.is_visible());
        assert!((overlay.level - 40.0 / 60.0).abs() < 1e-6);
        assert!((overlay.centroid_log - 1700f32.log10()).abs() < 1e-6);
    }

    #[test]
    fn decays_without_data() {
        let mut overlay = SpectrumOverlay::default();
        overlay.update(Some(target(-20.0)), 0.016);
        for _ in 0..120 {
            overlay.update(None, 1.0 / 30.0);
        }
        assert!(!overlay.is_visible());
        assert_eq!(overlay.level, 0.0);
    }

    #[test]
    fn clamps_out_of_range_frequencies() {
        let mut overlay = SpectrumOverlay::default();
        overlay.update(
            Some(SpectrumTarget {
                centroid_hz: 1.0e9,
                spread_hz: 1.0e9,
                ..target(-10.0)
            }),
            0.016,
        );
        assert!(overlay.centroid_log <= MAX_FREQ_HZ.log10());
        assert!(overlay.spread_log <= 1.3);
    }

    #[test]
    fn ignores_non_positive_time_step() {
        let mut overlay = SpectrumOverlay::default();
        overlay.update(Some(target(-20.0)), 0.0);
        assert!(overlay.is_visible());
    }

    #[test]
    fn pulse_is_zero_without_data() {
        assert_eq!(SpectrumOverlay::default().pulse(), 0.0);

        let mut overlay = SpectrumOverlay::default();
        overlay.update(Some(target(-20.0)), 0.016);
        for _ in 0..120 {
            overlay.update(None, 1.0 / 30.0);
        }
        assert_eq!(overlay.pulse(), 0.0);
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
}
