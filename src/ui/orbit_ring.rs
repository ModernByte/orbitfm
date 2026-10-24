//! Орбитальное аудиокольцо за глобусом (ADR-0029).
//!
//! Слой живёт только в Focus и рисуется painter'ом до wgpu-callback'а
//! глобуса, поэтому планета естественно перекрывает внутреннюю часть кольца.
//! Данные — шесть агрегатов эфира; PCM и полосы не нужны.

use std::f32::consts::TAU;

use eframe::egui::{self, Color32, Mesh, Pos2, Shape, Stroke, Vec2, vec2};

use crate::render::{MIN_RMS_DB, RMS_RANGE_DB, SpectrumTarget};

use super::palette::{Palette, mix_colors};
use super::widgets::faded;

const BAR_COUNT: usize = 120;
const WAVE_SEGMENTS: usize = 96;
const COMET_TRAIL: usize = 30;
const MAX_SPARKS: usize = 64;
const TRACK_TICKS: usize = 64;
const HAZE_SEGMENTS: usize = 120;
/// Внутренний радиус кольца в радиусах планеты.
const INNER_RADIUS: f32 = 1.035;
const ONSET_COOLDOWN: f32 = 0.14;

/// Нормированные и сглаженные метрики одного кадра.
#[derive(Debug, Clone, Copy)]
struct Metrics {
    level: f32,
    flux: f32,
    centroid: f32,
    spread: f32,
    flatness: f32,
}

/// Искра, выброшенная из кольца на удар.
#[derive(Debug, Clone, Copy)]
struct Spark {
    angle: f32,
    radius: f32,
    radial_speed: f32,
    drift: f32,
    life: f32,
    max_life: f32,
    size: f32,
}

/// Состояние орбитального кольца: живёт кадрами прогона.
#[derive(Debug)]
pub(crate) struct OrbitRing {
    presence: f32,
    level: f32,
    flux: f32,
    flux_slow: f32,
    centroid: f32,
    spread: f32,
    flatness: f32,
    bars: [f32; BAR_COUNT],
    sparks: Vec<Spark>,
    spin: f32,
    comet: f32,
    comet_glow: f32,
    beat: f32,
    onset_cooldown: f32,
    clock: f32,
    rng: u64,
    initialized: bool,
}

impl Default for OrbitRing {
    fn default() -> Self {
        Self {
            presence: 0.0,
            level: 0.0,
            flux: 0.0,
            flux_slow: 0.0,
            centroid: 0.0,
            spread: 0.0,
            flatness: 0.0,
            bars: [0.0; BAR_COUNT],
            sparks: Vec::new(),
            spin: 0.0,
            comet: 0.0,
            comet_glow: 0.0,
            beat: 0.0,
            onset_cooldown: 0.0,
            clock: 0.0,
            rng: 0x9E37_79B9_7F4A_7C15,
            initialized: false,
        }
    }
}

impl OrbitRing {
    /// Обновляет кольцо за кадр. `live` — эфир реально идёт; без него
    /// кольцо плавно гаснет, а искры догорают.
    pub(crate) fn update(&mut self, target: Option<SpectrumTarget>, live: bool, dt: f32) {
        let dt = dt.clamp(0.0, 0.1);
        self.clock = (self.clock + dt) % 1_000.0;
        self.onset_cooldown = (self.onset_cooldown - dt).max(0.0);
        self.beat = (self.beat - dt / 0.55).max(0.0);
        self.comet_glow *= (-dt / 0.30).exp();

        let Some(target) = target.filter(|_| live) else {
            self.presence = follow(self.presence, 0.0, dt, 0.70, 0.70);
            self.level = follow(self.level, 0.0, dt, 0.25, 0.25);
            self.flux = follow(self.flux, 0.0, dt, 0.20, 0.20);
            self.flux_slow = follow(self.flux_slow, 0.0, dt, 1.10, 1.10);
            self.advance_bars(dt, None);
            self.update_sparks(dt);
            if self.presence < 0.004 {
                self.reset();
            }
            return;
        };

        let metrics = normalize(target);
        if self.initialized {
            self.level = follow(self.level, metrics.level, dt, 0.03, 0.22);
            self.flux = follow(self.flux, metrics.flux, dt, 0.015, 0.18);
            self.flux_slow = follow(self.flux_slow, metrics.flux, dt, 1.10, 1.10);
            self.centroid = follow(self.centroid, metrics.centroid, dt, 0.30, 0.30);
            self.spread = follow(self.spread, metrics.spread, dt, 0.35, 0.35);
            self.flatness = follow(self.flatness, metrics.flatness, dt, 0.40, 0.40);
        } else {
            self.level = metrics.level;
            self.flux = metrics.flux;
            self.flux_slow = metrics.flux;
            self.centroid = metrics.centroid;
            self.spread = metrics.spread;
            self.flatness = metrics.flatness;
            self.seed_bars(metrics);
            self.initialized = true;
        }
        self.presence = follow(self.presence, 1.0, dt, 0.25, 0.70);

        let onset =
            self.flux - self.flux_slow > 0.06 && self.flux > 0.10 && self.onset_cooldown <= 0.0;
        if onset {
            self.onset_cooldown = ONSET_COOLDOWN;
            self.beat = 1.0;
            self.comet_glow = (self.comet_glow + 0.9).min(1.6);
            let count = 4 + (self.level * 10.0).round() as usize;
            self.spawn_sparks(count);
        }

        let energy = self.level * 0.65 + self.flux * 0.35;
        self.spin = (self.spin + dt * (0.10 + energy * 0.45)) % TAU;
        self.comet = (self.comet
            + dt * (0.45 + self.centroid * 0.9 + self.level * 0.7 + self.flux * 0.5))
            % TAU;

        self.advance_bars(dt, Some(metrics));
        self.update_sparks(dt);
    }

    /// Нужны ли кадры: кольцо видно, догорают искры или инерция удара.
    pub(crate) fn is_animating(&self) -> bool {
        self.presence > 0.004
            || !self.sparks.is_empty()
            || self.beat > 0.004
            || self.comet_glow > 0.01
    }

    /// Рисует кольцо в экранных координатах. Вызывается до `paint_globe`.
    pub(crate) fn paint(
        &self,
        painter: &egui::Painter,
        center: Pos2,
        planet_radius: f32,
        palette: &Palette,
    ) {
        if self.presence <= 0.004
            || !center.x.is_finite()
            || !center.y.is_finite()
            || !planet_radius.is_finite()
            || planet_radius < 8.0
        {
            return;
        }

        let accent = palette.accent;
        let cyan = mix_colors(accent, Color32::from_rgb(74, 220, 255), 0.58);
        let hot = mix_colors(cyan, palette.text, 0.58);
        let scene = RingScene {
            center,
            planet_radius,
            base: planet_radius * INNER_RADIUS,
            accent,
            cyan,
            hot,
            presence: self.presence.clamp(0.0, 1.0),
            level: self.level.clamp(0.0, 1.0),
            flux: self.flux.clamp(0.0, 1.0),
            energy: (self.level * 0.65 + self.flux * 0.35).clamp(0.0, 1.0),
        };

        self.paint_haze(painter, &scene);
        self.paint_tracks(painter, &scene);
        self.paint_wave(painter, &scene);
        self.paint_bars(painter, &scene);
        self.paint_comet(painter, &scene);
        self.paint_beat(painter, &scene);
        self.paint_sparks(painter, &scene);
    }

    fn advance_bars(&mut self, dt: f32, metrics: Option<Metrics>) {
        let attack = 1.0 - (-dt / 0.045).exp();
        let release = 1.0 - (-dt / 0.22).exp();
        for index in 0..BAR_COUNT {
            let target = metrics.map_or(0.0, |metrics| {
                bar_target(index, metrics, self.clock, self.comet, self.spin)
            });
            let bar = &mut self.bars[index];
            let factor = if target > *bar { attack } else { release };
            *bar = (*bar + (target - *bar) * factor).clamp(0.0, 1.0);
        }
    }

    fn seed_bars(&mut self, metrics: Metrics) {
        for (index, bar) in self.bars.iter_mut().enumerate() {
            *bar = bar_target(index, metrics, self.clock, self.comet, self.spin);
        }
    }

    fn spawn_sparks(&mut self, count: usize) {
        for _ in 0..count {
            let angle = self.rand01() * TAU;
            let radius = 1.01 + self.rand01() * 0.14;
            let radial_speed = 0.05 + self.rand01() * 0.20;
            let drift = (self.rand01() - 0.5) * 0.55;
            let max_life = 0.55 + self.rand01() * 0.95;
            let size = 0.8 + self.rand01() * 1.5;
            self.sparks.push(Spark {
                angle,
                radius,
                radial_speed,
                drift,
                life: max_life,
                max_life,
                size,
            });
        }
        if self.sparks.len() > MAX_SPARKS {
            let excess = self.sparks.len() - MAX_SPARKS;
            self.sparks.drain(..excess);
        }
    }

    fn update_sparks(&mut self, dt: f32) {
        for spark in &mut self.sparks {
            spark.life -= dt;
            spark.radius += spark.radial_speed * dt;
            spark.angle = (spark.angle + spark.drift * dt).rem_euclid(TAU);
        }
        self.sparks.retain(|spark| spark.life > 0.0);
    }

    fn reset(&mut self) {
        self.presence = 0.0;
        self.level = 0.0;
        self.flux = 0.0;
        self.flux_slow = 0.0;
        self.centroid = 0.0;
        self.spread = 0.0;
        self.flatness = 0.0;
        self.bars = [0.0; BAR_COUNT];
        self.sparks.clear();
        self.spin = 0.0;
        self.comet = 0.0;
        self.comet_glow = 0.0;
        self.beat = 0.0;
        self.initialized = false;
    }

    fn rand01(&mut self) -> f32 {
        let mut state = self.rng;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        self.rng = state;
        (state >> 40) as f32 / (1u64 << 24) as f32
    }

    fn paint_haze(&self, painter: &egui::Painter, scene: &RingScene) {
        let bands = [
            (
                1.015,
                1.18 + 0.03 * scene.level,
                mix_colors(scene.accent, scene.cyan, 0.25),
                0.07 + 0.13 * scene.level,
            ),
            (
                1.08,
                1.36 + 0.04 * scene.level,
                scene.cyan,
                0.045 + 0.08 * scene.flux,
            ),
        ];
        for (inner_scale, outer_scale, color, alpha) in bands {
            let inner = scene.planet_radius * inner_scale;
            let outer = scene.planet_radius * outer_scale;
            let inner_color = faded(color, alpha * scene.presence);
            let outer_color = faded(color, 0.0);

            let mut mesh = Mesh::default();
            mesh.reserve_vertices((HAZE_SEGMENTS + 1) * 2);
            mesh.reserve_triangles(HAZE_SEGMENTS * 2);
            for index in 0..=HAZE_SEGMENTS {
                let angle = index as f32 / HAZE_SEGMENTS as f32 * TAU;
                let direction = direction(angle);
                mesh.colored_vertex(scene.center + direction * inner, inner_color);
                mesh.colored_vertex(scene.center + direction * outer, outer_color);
            }
            for index in 0..HAZE_SEGMENTS as u32 {
                let a = index * 2;
                mesh.add_triangle(a, a + 1, a + 2);
                mesh.add_triangle(a + 1, a + 3, a + 2);
            }
            painter.add(Shape::mesh(mesh));
        }

        painter.circle_stroke(
            scene.center,
            scene.base,
            Stroke::new(
                1.0 + 1.4 * scene.level,
                faded(scene.cyan, (0.10 + 0.30 * scene.level) * scene.presence),
            ),
        );
    }

    fn paint_tracks(&self, painter: &egui::Painter, scene: &RingScene) {
        let tick_color = faded(
            mix_colors(scene.accent, scene.cyan, 0.35),
            (0.20 + 0.15 * scene.level) * scene.presence,
        );
        for index in 0..TRACK_TICKS {
            let seed = hash01(index as u32 + 331);
            let angle = index as f32 / TRACK_TICKS as f32 * TAU + self.spin * 0.12;
            let direction = direction(angle);
            let inner =
                scene.planet_radius * (1.155 + 0.012 * (angle * 3.0 + self.clock * 0.4).sin());
            let length = scene.planet_radius * (0.010 + 0.018 * seed);
            painter.line_segment(
                [
                    scene.center + direction * inner,
                    scene.center + direction * (inner + length),
                ],
                Stroke::new(1.0, tick_color),
            );
        }

        let arc_color = faded(
            mix_colors(scene.accent, scene.cyan, 0.55),
            (0.14 + 0.12 * scene.energy) * scene.presence,
        );
        for (ring, speed) in [(1.245_f32, 0.05_f32), (1.315, -0.032)] {
            let radius = scene.planet_radius * ring;
            let segments = 48;
            for index in 0..segments {
                if index % 3 == 0 {
                    continue;
                }
                let start = index as f32 / segments as f32 * TAU + self.spin * speed;
                let end = start + TAU / segments as f32 * 0.72;
                painter.line_segment(
                    [
                        point_at(scene.center, radius, start),
                        point_at(scene.center, radius, end),
                    ],
                    Stroke::new(1.0, arc_color),
                );
            }
        }
    }

    fn paint_wave(&self, painter: &egui::Painter, scene: &RingScene) {
        let amplitude = scene.planet_radius * (0.010 + 0.024 * scene.level + 0.012 * scene.flux);
        let radius = scene.planet_radius * (1.115 + 0.022 * scene.level);
        let harmonic = 5.0 + self.centroid * 9.0;
        let phase = self.clock * 0.8 + self.spin * 0.5;
        let mut points = Vec::with_capacity(WAVE_SEGMENTS + 1);
        for index in 0..=WAVE_SEGMENTS {
            let angle = index as f32 / WAVE_SEGMENTS as f32 * TAU;
            let ripple = (angle * harmonic + phase).sin()
                + 0.45 * (angle * (harmonic * 0.55 + 2.0) - phase * 1.4).sin();
            points.push(point_at(scene.center, radius + amplitude * ripple, angle));
        }
        let color = mix_colors(scene.accent, scene.cyan, 0.35 + 0.45 * self.centroid);
        painter.add(Shape::line(
            points.clone(),
            Stroke::new(5.5, faded(color, 0.11 * scene.presence)),
        ));
        painter.add(Shape::line(
            points,
            Stroke::new(
                1.5,
                faded(color, (0.30 + 0.42 * scene.energy) * scene.presence),
            ),
        ));
    }

    fn paint_bars(&self, painter: &egui::Painter, scene: &RingScene) {
        let mut glow = Mesh::default();
        let mut main = Mesh::default();
        for index in 0..BAR_COUNT {
            let bar = self.bars[index];
            if bar < 0.02 {
                continue;
            }
            let angle = index as f32 / BAR_COUNT as f32 * TAU + self.spin * 0.06;
            let direction = direction(angle);
            let normal = vec2(-direction.y, direction.x);
            let inner = scene.center + direction * scene.base;
            let length = scene.planet_radius * (0.012 + 0.235 * bar.powf(1.2));
            let tip = inner + direction * length;
            let width = 0.85 + 1.9 * bar;
            let base_color = mix_colors(scene.accent, scene.cyan, 0.40);
            let tip_color = mix_colors(scene.cyan, scene.hot, bar);
            let presence = scene.presence;

            let start = main.vertices.len() as u32;
            main.colored_vertex(
                inner + normal * width,
                faded(base_color, (0.12 + 0.55 * bar) * presence),
            );
            main.colored_vertex(
                inner - normal * width,
                faded(base_color, (0.12 + 0.55 * bar) * presence),
            );
            main.colored_vertex(tip, faded(tip_color, (0.22 + 0.78 * bar) * presence));
            main.add_triangle(start, start + 1, start + 2);

            let glow_tip = tip + direction * scene.planet_radius * 0.018;
            let glow_start = glow.vertices.len() as u32;
            glow.colored_vertex(inner + normal * width * 2.8, faded(scene.accent, 0.0));
            glow.colored_vertex(inner - normal * width * 2.8, faded(scene.accent, 0.0));
            glow.colored_vertex(glow_tip, faded(scene.cyan, 0.14 * bar * presence));
            glow.add_triangle(glow_start, glow_start + 1, glow_start + 2);

            if bar > 0.55 {
                painter.circle_filled(
                    tip,
                    0.7 + 1.3 * bar,
                    faded(scene.hot, (0.18 + 0.45 * bar) * presence),
                );
            }
        }
        painter.add(Shape::mesh(glow));
        painter.add(Shape::mesh(main));
    }

    fn paint_comet(&self, painter: &egui::Painter, scene: &RingScene) {
        let intensity = (0.35 + 0.65 * scene.level + 0.25 * scene.flux).clamp(0.0, 1.0);
        let radius = scene.planet_radius * 1.165;
        for step in 0..COMET_TRAIL {
            let fade = 1.0 - step as f32 / COMET_TRAIL as f32;
            let angle = self.comet - step as f32 * 0.028;
            let wobble = 1.0 + 0.012 * (angle * 3.0 + self.clock * 0.6).sin();
            let position = point_at(scene.center, radius * wobble, angle);
            let color = mix_colors(scene.hot, scene.cyan, step as f32 / COMET_TRAIL as f32);
            let alpha = (0.05 + 0.78 * fade * fade) * (0.30 + 0.70 * intensity) * scene.presence;
            painter.circle_filled(position, 0.55 + 2.8 * fade, faded(color, alpha));
        }

        let head = point_at(scene.center, radius, self.comet);
        let glow = (2.6 + 2.8 * scene.level + 1.6 * self.comet_glow).min(8.5);
        painter.circle_filled(
            head,
            glow * 2.8,
            faded(scene.cyan, 0.11 * scene.presence * intensity),
        );
        painter.circle_filled(
            head,
            glow * 1.5,
            faded(scene.cyan, 0.22 * scene.presence * intensity),
        );
        painter.circle_filled(
            head,
            glow * 0.55,
            faded(scene.hot, (0.45 + 0.35 * scene.level) * scene.presence),
        );
    }

    fn paint_beat(&self, painter: &egui::Painter, scene: &RingScene) {
        if self.beat <= 0.004 {
            return;
        }
        let progress = 1.0 - self.beat;
        for index in 0..2 {
            let radius =
                scene.base + scene.planet_radius * (0.06 + 0.26 * progress + 0.045 * index as f32);
            painter.circle_stroke(
                scene.center,
                radius,
                Stroke::new(
                    1.0 + 2.2 * self.beat,
                    faded(scene.cyan, 0.20 * self.beat * scene.presence),
                ),
            );
        }
        painter.circle_stroke(
            scene.center,
            scene.base,
            Stroke::new(
                1.0 + 1.5 * self.beat,
                faded(scene.hot, 0.22 * self.beat * scene.presence),
            ),
        );
    }

    fn paint_sparks(&self, painter: &egui::Painter, scene: &RingScene) {
        for spark in &self.sparks {
            let life = (spark.life / spark.max_life).clamp(0.0, 1.0);
            let direction = direction(spark.angle);
            let position = scene.center + direction * (scene.planet_radius * spark.radius);
            let trail = scene.center
                + direction * (scene.planet_radius * (spark.radius - spark.radial_speed * 0.10));
            let color = mix_colors(scene.hot, scene.cyan, 1.0 - life);
            let alpha = life.powf(1.4) * 0.75 * scene.presence;
            painter.line_segment(
                [trail, position],
                Stroke::new(0.8 + spark.size * 0.3, faded(color, alpha * 0.65)),
            );
            painter.circle_filled(
                position,
                spark.size * (0.45 + 0.55 * life),
                faded(color, alpha),
            );
        }
    }
}

/// Экранные координаты и цвета одного кадра кольца.
struct RingScene {
    center: Pos2,
    planet_radius: f32,
    base: f32,
    accent: Color32,
    cyan: Color32,
    hot: Color32,
    presence: f32,
    level: f32,
    flux: f32,
    energy: f32,
}

fn direction(angle: f32) -> Vec2 {
    vec2(angle.cos(), angle.sin())
}

fn point_at(center: Pos2, radius: f32, angle: f32) -> Pos2 {
    center + direction(angle) * radius
}

fn normalize(target: SpectrumTarget) -> Metrics {
    let centroid = ((target.centroid_hz - 300.0) / 5_700.0).clamp(0.0, 1.0);
    let spread = ((target.spread_hz - 400.0) / 4_600.0).clamp(0.0, 1.0);
    let flatness = target.flatness.clamp(0.0, 1.0);
    Metrics {
        level: ((target.rms_db - MIN_RMS_DB) / RMS_RANGE_DB).clamp(0.0, 1.0),
        flux: (target.flux.max(0.0) * 500.0).sqrt().clamp(0.0, 1.0),
        centroid: finite_or_zero(centroid),
        spread: finite_or_zero(spread),
        flatness: finite_or_zero(flatness),
    }
}

fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

fn follow(current: f32, target: f32, dt: f32, attack_tau: f32, release_tau: f32) -> f32 {
    let tau = if target > current {
        attack_tau
    } else {
        release_tau
    };
    let factor = if dt > 0.0 {
        1.0 - (-dt / tau).exp()
    } else {
        0.0
    };
    current + (target - current) * factor
}

/// Форма короны: медленные лепестки, бегущая волна и «горячая» сторона
/// вокруг кометы. Детерминированная функция, чтобы кольцо не мигало.
fn bar_target(index: usize, metrics: Metrics, clock: f32, comet: f32, spin: f32) -> f32 {
    let angle = index as f32 / BAR_COUNT as f32 * TAU + spin * 0.06;
    let seed = hash01(index as u32);
    let phase = seed * TAU;
    let lobe = 0.5 + 0.5 * (angle * 2.0 + clock * (0.55 + metrics.centroid * 0.9) + phase).sin();
    let sub = 0.5 + 0.5 * (angle * 5.0 - clock * (0.85 + metrics.spread * 1.5) + phase * 1.7).sin();
    let shimmer = 0.5
        + 0.5
            * (angle * (13.0 + metrics.spread * 16.0)
                + clock * (1.8 + metrics.flux * 5.0)
                + phase * 3.1)
                .sin();
    let bias = 0.72 + 0.28 * (angle - comet).cos();
    let base = 0.16 + 0.58 * lobe.powf(1.5) * sub;
    let target = (metrics.level * (0.45 + 0.65 * base)
        + metrics.flux * (0.15 + 0.45 * shimmer)
        + metrics.flatness * 0.12 * seed)
        * bias;
    target.clamp(0.0, 1.0)
}

fn hash01(mut value: u32) -> f32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7FEB_352D);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846C_A68B);
    value ^= value >> 16;
    (value & 0x00FF_FFFF) as f32 / 0x0100_0000 as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(rms_db: f32, flux: f32) -> SpectrumTarget {
        SpectrumTarget {
            centroid_hz: 1_700.0,
            spread_hz: 1_600.0,
            flux,
            flatness: 0.4,
            rms_db,
        }
    }

    #[test]
    fn ring_fades_in_and_out_with_playback() {
        let mut ring = OrbitRing::default();
        ring.update(Some(target(-20.0, 0.002)), true, 0.05);

        assert!(ring.presence > 0.0);
        assert!(ring.is_animating());
        assert!(ring.bars.iter().any(|bar| *bar > 0.0));

        for _ in 0..240 {
            ring.update(None, false, 1.0 / 30.0);
        }

        assert_eq!(ring.presence, 0.0);
        assert!(!ring.is_animating());
        assert!(ring.sparks.is_empty());
    }

    #[test]
    fn data_without_live_audio_is_ignored() {
        let mut ring = OrbitRing::default();
        ring.update(Some(target(-10.0, 0.02)), false, 0.05);

        assert_eq!(ring.presence, 0.0);
        assert!(!ring.is_animating());
    }

    #[test]
    fn bars_stay_bounded_on_extreme_metrics() {
        let mut ring = OrbitRing::default();
        ring.update(
            Some(SpectrumTarget {
                centroid_hz: 90_000.0,
                spread_hz: 90_000.0,
                flux: 90_000.0,
                flatness: 9.0,
                rms_db: 40.0,
            }),
            true,
            0.05,
        );

        assert!(
            ring.bars
                .iter()
                .all(|bar| bar.is_finite() && (0.0..=1.0).contains(bar))
        );
    }

    #[test]
    fn onset_spawns_sparks_within_limit() {
        let mut ring = OrbitRing::default();
        ring.update(Some(target(-50.0, 0.0)), true, 0.05);
        ring.update(Some(target(-10.0, 2.0)), true, 0.05);
        assert!(!ring.sparks.is_empty());

        for _ in 0..120 {
            ring.update(Some(target(-10.0, 2.0)), true, 0.05);
            assert!(ring.sparks.len() <= MAX_SPARKS);
        }
    }
}
