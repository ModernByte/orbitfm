//! Камера глобуса: орбитальное вращение, зум и преобразование экран → сфера.

use std::time::Duration;

use eframe::egui::{Pos2, Rect, Vec2};
use glam::camera::rh::{proj::directx, view};
use glam::{Mat4, Vec3, Vec4};

use super::{GlobeDeform, GlobeInput, GlobeMotion};

const FOV_Y_RADIANS: f32 = std::f32::consts::FRAC_PI_4;
const NEAR_PLANE: f32 = 0.1;
const FAR_PLANE: f32 = 50.0;

/// Дистанция по умолчанию оставляет вокруг планеты место для орбитального
/// аудиокольца (ADR-0029); ближе можно приблизиться колесом.
const DEFAULT_DISTANCE: f32 = 3.2;
const MIN_DISTANCE: f32 = 1.4;
const MAX_DISTANCE: f32 = 6.0;
const ZOOM_SENSITIVITY: f32 = 0.002;

const DRAG_SENSITIVITY: f32 = 0.006;
const INERTIA_DAMPING: f32 = 6.0;
const INERTIA_STOP: f32 = 0.01;
const MAX_PITCH: f32 = 85.0 * std::f32::consts::PI / 180.0;
const MIN_FLY_DURATION: f32 = 0.05;

/// Простой до старта авто-вращения, с.
const IDLE_SPIN_DELAY: f32 = 4.0;
/// Выход на полную скорость авто-вращения, с.
const IDLE_SPIN_RAMP: f32 = 2.0;
/// Скорость авто-вращения, рад/с: полный оборот примерно за 17 минут.
/// Земля в действительности делает оборот за сутки, но такое движение
/// глазом не различить; здесь темп замедлен, но заметен.
const IDLE_SPIN_RATE: f32 = 0.006;

/// Плавный доворот камеры к стране; отменяется перетаскиванием.
#[derive(Debug, Clone, Copy)]
struct FlyAnimation {
    start_yaw: f32,
    target_yaw: f32,
    start_pitch: f32,
    target_pitch: f32,
    elapsed: f32,
    duration: f32,
}

/// Униформ-данные камеры для шейдера.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub light_direction: [f32; 4],
    pub camera_position: [f32; 4],
    /// x = scale_y − 1, y = scale_xz − 1, z = ripple, w = ripple_phase.
    pub motion: [f32; 4],
    /// x = flash, y = style_id, z = beat_phase, w = time.
    pub motion2: [f32; 4],
}

/// Орбитальная камера вокруг глобуса радиусом 1.
#[derive(Debug)]
pub struct GlobeCamera {
    yaw: f32,
    pitch: f32,
    distance: f32,
    yaw_velocity: f32,
    pitch_velocity: f32,
    fly: Option<FlyAnimation>,
    /// Сколько секунд прошло без взаимодействия с глобусом.
    idle: f32,
}

impl Default for GlobeCamera {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
            distance: DEFAULT_DISTANCE,
            yaw_velocity: 0.0,
            pitch_velocity: 0.0,
            fly: None,
            idle: 0.0,
        }
    }
}

impl GlobeCamera {
    /// Плавно переводит камеру к точке `(lat, lon)` за `duration`.
    ///
    /// Доворот идёт по кратчайшей дуге долготы; широта ограничивается
    /// допустимым наклоном камеры. Текущая инерция сбрасывается.
    pub fn fly_to(&mut self, lat: f64, lon: f64, duration: Duration) {
        let target_pitch = (lat as f32).to_radians().clamp(-MAX_PITCH, MAX_PITCH);
        let target_yaw = (lon as f32).to_radians();
        let delta_yaw = shortest_delta(self.yaw, target_yaw);
        self.fly = Some(FlyAnimation {
            start_yaw: self.yaw,
            target_yaw: self.yaw + delta_yaw,
            start_pitch: self.pitch,
            target_pitch,
            elapsed: 0.0,
            duration: duration.as_secs_f32().max(MIN_FLY_DURATION),
        });
        self.yaw_velocity = 0.0;
        self.pitch_velocity = 0.0;
        self.idle = 0.0;
    }

    /// Обновляет камеру по вводу за кадр (в тестах фактический интервал
    /// совпадает со сглаженным).
    #[cfg(test)]
    pub fn update(&mut self, dt: f32, input: &GlobeInput) {
        self.advance(dt, dt, input);
    }

    /// То же, но с реальным временем кадра: после сна приложения `stable_dt`
    /// остаётся маленьким, а простой нужно считать по фактическому интервалу.
    pub fn advance(&mut self, dt: f32, elapsed: f32, input: &GlobeInput) {
        let dt = dt.clamp(0.001, 0.05);
        let elapsed = elapsed.clamp(0.0, 60.0);
        let interacting = input.drag_delta != Vec2::ZERO
            || input.scroll_delta != 0.0
            || input.primary_clicked.is_some();

        if interacting || self.fly.is_some() {
            self.idle = 0.0;
        } else {
            self.idle += elapsed;
        }

        if input.drag_delta != Vec2::ZERO {
            self.fly = None;
        }

        if let Some(fly) = self.fly.as_mut() {
            fly.elapsed += dt;
            let progress = (fly.elapsed / fly.duration).clamp(0.0, 1.0);
            let eased = smoothstep(progress);
            self.yaw = fly.start_yaw + (fly.target_yaw - fly.start_yaw) * eased;
            self.pitch = fly.start_pitch + (fly.target_pitch - fly.start_pitch) * eased;

            if progress >= 1.0 {
                self.yaw = fly.target_yaw;
                self.pitch = fly.target_pitch;
                self.fly = None;
            }
        } else if input.drag_delta != Vec2::ZERO {
            let yaw_delta = -input.drag_delta.x * DRAG_SENSITIVITY;
            let pitch_delta = input.drag_delta.y * DRAG_SENSITIVITY;

            self.yaw += yaw_delta;
            self.pitch += pitch_delta;
            self.yaw_velocity = yaw_delta / dt;
            self.pitch_velocity = pitch_delta / dt;
        } else {
            self.yaw += self.yaw_velocity * dt;
            self.pitch += self.pitch_velocity * dt;

            let damping = (-INERTIA_DAMPING * dt).exp();
            self.yaw_velocity *= damping;
            self.pitch_velocity *= damping;
            if self.yaw_velocity.abs() < INERTIA_STOP {
                self.yaw_velocity = 0.0;
            }
            if self.pitch_velocity.abs() < INERTIA_STOP {
                self.pitch_velocity = 0.0;
            }
        }

        self.yaw = self.yaw.rem_euclid(std::f32::consts::TAU);
        self.pitch = self.pitch.clamp(-MAX_PITCH, MAX_PITCH);
        if (self.pitch == MAX_PITCH && self.pitch_velocity > 0.0)
            || (self.pitch == -MAX_PITCH && self.pitch_velocity < 0.0)
        {
            self.pitch_velocity = 0.0;
        }

        if input.scroll_delta != 0.0 {
            let scale = (-input.scroll_delta * ZOOM_SENSITIVITY).exp();
            self.distance = (self.distance * scale).clamp(MIN_DISTANCE, MAX_DISTANCE);
        }

        if self.fly.is_none() && !interacting && self.idle >= IDLE_SPIN_DELAY {
            let ramp = ((self.idle - IDLE_SPIN_DELAY) / IDLE_SPIN_RAMP).clamp(0.0, 1.0);
            // Земля вращается с запада на восток: камера смещается на запад,
            // континенты уходят вправо — как при реальном вращении.
            self.yaw = (self.yaw - IDLE_SPIN_RATE * ramp * dt).rem_euclid(std::f32::consts::TAU);
        }
    }

    /// Идёт ли движение, требующее перерисовки: доворот, инерция или
    /// авто-вращение в простое.
    pub fn is_animating(&self) -> bool {
        self.fly.is_some()
            || self.yaw_velocity != 0.0
            || self.pitch_velocity != 0.0
            || self.is_idle_spinning()
    }

    /// Сколько секунд осталось до авто-вращения. `None`, если доворот уже
    /// идёт или вращение началось.
    pub fn idle_countdown(&self) -> Option<f32> {
        if self.fly.is_some() || self.is_idle_spinning() {
            None
        } else {
            Some((IDLE_SPIN_DELAY - self.idle).max(0.0))
        }
    }

    /// Идёт ли медленное авто-вращение в простое.
    pub fn is_idle_spinning(&self) -> bool {
        self.fly.is_none() && self.idle >= IDLE_SPIN_DELAY
    }

    /// Позиция камеры в мировых координатах.
    pub fn eye(&self) -> Vec3 {
        let cos_pitch = self.pitch.cos();
        Vec3::new(
            cos_pitch * self.yaw.sin(),
            self.pitch.sin(),
            cos_pitch * self.yaw.cos(),
        ) * self.distance
    }

    /// Матрица вида и проекции для текущего aspect.
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = view::look_at_mat4(self.eye(), Vec3::ZERO, Vec3::Y);
        let projection =
            directx::perspective(FOV_Y_RADIANS, aspect.max(0.01), NEAR_PLANE, FAR_PLANE);
        projection * view
    }

    /// Радиус силуэта единичной сферы на экране в пикселях. Центр силуэта
    /// совпадает с центром viewport'а: камера всегда смотрит в начало координат.
    pub fn projected_radius(&self, viewport: Rect) -> f32 {
        if !viewport.is_finite() || viewport.height() <= 0.0 {
            return 0.0;
        }
        let focal = viewport.height() * 0.5 / (FOV_Y_RADIANS * 0.5).tan();
        let depth = (self.distance * self.distance - 1.0).max(1e-4).sqrt();
        focal / depth
    }

    /// Пересечение луча из экранной точки со сферой радиусом 1.
    ///
    /// То же, но луч переводится в пространство единичной сферы обратным
    /// масштабом `deform` (ADR-0028). Возвращённая точка всегда единичная.
    pub fn screen_to_sphere_deformed(
        &self,
        screen_pos: Pos2,
        viewport: Rect,
        deform: &GlobeDeform,
    ) -> Option<Vec3> {
        if viewport.width() <= 0.0 || viewport.height() <= 0.0 || !viewport.contains(screen_pos) {
            return None;
        }

        let ndc_x = (screen_pos.x - viewport.left()) / viewport.width() * 2.0 - 1.0;
        let ndc_y = 1.0 - (screen_pos.y - viewport.top()) / viewport.height() * 2.0;
        let inverse = self.view_proj(viewport.aspect_ratio()).inverse();

        let near = unproject(inverse, ndc_x, ndc_y, 0.0)?;
        let far = unproject(inverse, ndc_x, ndc_y, 1.0)?;

        let origin = deform.to_unit(self.eye());
        let near = deform.to_unit(near);
        let far = deform.to_unit(far);
        let direction = (far - near).normalize_or_zero();
        if direction == Vec3::ZERO {
            return None;
        }

        let b = origin.dot(direction);
        let c = origin.length_squared() - 1.0;
        let discriminant = b * b - c;
        if discriminant < 0.0 {
            return None;
        }

        let distance = -b - discriminant.sqrt();
        (distance >= 0.0).then(|| (origin + direction * distance).normalize())
    }

    /// Проецирует точку поверхности в экранные координаты. Точка может быть
    /// как единичной, так и уже деформированной (`GlobeDeform::to_world`).
    ///
    /// Возвращает `None` для обратной полусферы, выхода за viewport и
    /// нечисловых координат.
    pub fn project_on_sphere(&self, point: Vec3, viewport: Rect) -> Option<Pos2> {
        if viewport.width() <= 0.0 || viewport.height() <= 0.0 {
            return None;
        }
        if point.dot(self.eye()) <= point.length_squared() {
            return None;
        }

        let clip = self.view_proj(viewport.aspect_ratio()) * point.extend(1.0);
        if clip.w <= f32::EPSILON {
            return None;
        }

        let ndc = clip.truncate() / clip.w;
        let position = Pos2::new(
            viewport.left() + (ndc.x * 0.5 + 0.5) * viewport.width(),
            viewport.top() + (0.5 - ndc.y * 0.5) * viewport.height(),
        );
        (position.x.is_finite() && position.y.is_finite() && viewport.contains(position))
            .then_some(position)
    }

    pub(crate) fn uniform_with_motion(&self, aspect: f32, motion: &GlobeMotion) -> CameraUniform {
        let light = Vec3::new(0.35, 0.28, 0.90).normalize();
        let eye = self.eye();
        CameraUniform {
            view_proj: self.view_proj(aspect).to_cols_array_2d(),
            light_direction: [light.x, light.y, light.z, 0.0],
            camera_position: [eye.x, eye.y, eye.z, 0.0],
            motion: motion.uniform_values(),
            motion2: motion.uniform2_values(),
        }
    }
}

fn unproject(inverse_view_proj: Mat4, x: f32, y: f32, z: f32) -> Option<Vec3> {
    let clip = inverse_view_proj * Vec4::new(x, y, z, 1.0);
    (clip.w.abs() > f32::EPSILON).then(|| clip.truncate() / clip.w)
}

fn shortest_delta(from: f32, to: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    (to - from + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> Rect {
        Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))
    }

    fn sphere_point(lat: f64, lon: f64) -> Vec3 {
        let lat = lat.to_radians() as f32;
        let lon = lon.to_radians() as f32;
        Vec3::new(lat.cos() * lon.sin(), lat.sin(), lat.cos() * lon.cos())
    }

    #[test]
    fn center_ray_hits_visible_hemisphere() {
        let camera = GlobeCamera::default();
        let hit = camera
            .screen_to_sphere_deformed(viewport().center(), viewport(), &GlobeDeform::IDENTITY)
            .expect("центр экрана должен попадать в сферу");

        assert!((hit.length() - 1.0).abs() < 1e-5);
        assert!(hit.z > 0.99, "ожидалась точка перед камерой: {hit:?}");
    }

    #[test]
    fn center_ray_reflects_camera_angles() {
        let camera = GlobeCamera {
            yaw: 13.405_f32.to_radians(),
            pitch: 52.52_f32.to_radians(),
            ..GlobeCamera::default()
        };
        let hit = camera
            .screen_to_sphere_deformed(viewport().center(), viewport(), &GlobeDeform::IDENTITY)
            .expect("центр экрана должен попадать в сферу");

        let lat = hit.y.asin().to_degrees();
        let lon = hit.x.atan2(hit.z).to_degrees();
        assert!((lat - 52.52).abs() < 1e-3, "широта: {lat}");
        assert!((lon - 13.405).abs() < 1e-3, "долгота: {lon}");
    }

    #[test]
    fn projects_visible_point_to_screen_center() {
        let camera = GlobeCamera::default();
        let rect = viewport();
        let position = camera
            .project_on_sphere(sphere_point(0.0, 0.0), rect)
            .expect("точка перед камерой должна проецироваться");

        assert!((position - rect.center()).length() < 1e-3);
    }

    #[test]
    fn projection_respects_latitude_and_longitude() {
        let camera = GlobeCamera::default();
        let rect = viewport();
        let position = camera
            .project_on_sphere(sphere_point(37.97, 23.72), rect)
            .expect("Афины должны быть видны");

        assert!(
            position.x > rect.center().x,
            "восточная долгота правее центра"
        );
        assert!(position.y < rect.center().y, "северная широта выше центра");
    }

    #[test]
    fn hides_back_hemisphere_and_invalid_viewport() {
        let camera = GlobeCamera::default();
        let rect = viewport();

        assert!(
            camera
                .project_on_sphere(sphere_point(0.0, 180.0), rect)
                .is_none()
        );
        assert!(
            camera
                .project_on_sphere(sphere_point(0.0, 0.0), Rect::NOTHING)
                .is_none()
        );
    }

    #[test]
    fn corner_ray_misses_sphere() {
        let camera = GlobeCamera::default();
        let corner = viewport().left_top();

        assert!(
            camera
                .screen_to_sphere_deformed(corner, viewport(), &GlobeDeform::IDENTITY)
                .is_none()
        );
    }

    #[test]
    fn drag_changes_orientation_and_keeps_inertia() {
        let mut camera = GlobeCamera::default();
        let input = GlobeInput {
            drag_delta: Vec2::new(40.0, 0.0),
            ..GlobeInput::default()
        };

        camera.update(0.016, &input);
        assert!(camera.is_animating());

        let yaw_before = camera.yaw;
        camera.update(0.05, &GlobeInput::default());
        assert!(
            camera.yaw < yaw_before,
            "инерция должна продолжать вращение"
        );
    }

    #[test]
    fn zoom_is_clamped() {
        let mut camera = GlobeCamera::default();
        camera.update(
            0.016,
            &GlobeInput {
                scroll_delta: 100_000.0,
                ..GlobeInput::default()
            },
        );
        assert!((camera.distance - MIN_DISTANCE).abs() < 1e-5);

        camera.update(
            0.016,
            &GlobeInput {
                scroll_delta: -100_000.0,
                ..GlobeInput::default()
            },
        );
        assert!((camera.distance - MAX_DISTANCE).abs() < 1e-5);
    }

    #[test]
    fn pitch_is_clamped() {
        let mut camera = GlobeCamera::default();
        camera.update(
            0.016,
            &GlobeInput {
                drag_delta: Vec2::new(0.0, 100_000.0),
                ..GlobeInput::default()
            },
        );

        assert!((camera.pitch - MAX_PITCH).abs() < 1e-5);
        assert!(camera.eye().is_finite());
    }

    #[test]
    fn fly_to_centers_requested_point() {
        let mut camera = GlobeCamera::default();
        camera.fly_to(52.52, 13.405, Duration::from_millis(400));

        while camera.is_animating() {
            camera.update(0.05, &GlobeInput::default());
        }

        let hit = camera
            .screen_to_sphere_deformed(viewport().center(), viewport(), &GlobeDeform::IDENTITY)
            .expect("после доворота центр должен попадать в сферу");
        let lat = hit.y.asin().to_degrees();
        let lon = hit.x.atan2(hit.z).to_degrees();
        assert!((lat - 52.52).abs() < 1e-3, "широта: {lat}");
        assert!((lon - 13.405).abs() < 1e-3, "долгота: {lon}");
    }

    #[test]
    fn fly_uses_shortest_way_across_antimeridian() {
        let mut camera = GlobeCamera {
            yaw: 350_f32.to_radians(),
            ..GlobeCamera::default()
        };
        camera.fly_to(0.0, 10.0, Duration::from_secs(1));
        camera.update(0.016, &GlobeInput::default());

        let yaw_degrees = camera.yaw.to_degrees();
        assert!(
            yaw_degrees > 350.0 && yaw_degrees < 351.0,
            "камера должна идти на восток через 0°, а не на запад: {yaw_degrees}"
        );
    }

    #[test]
    fn drag_cancels_fly() {
        let mut camera = GlobeCamera::default();
        camera.fly_to(-33.87, 151.21, Duration::from_millis(500));
        camera.update(
            0.016,
            &GlobeInput {
                drag_delta: Vec2::new(12.0, 0.0),
                ..GlobeInput::default()
            },
        );

        assert!(camera.fly.is_none());
    }

    #[test]
    fn fly_respects_pitch_limit() {
        let mut camera = GlobeCamera::default();
        camera.fly_to(89.0, 0.0, Duration::from_millis(100));

        while camera.is_animating() {
            camera.update(0.05, &GlobeInput::default());
        }

        assert!((camera.pitch - MAX_PITCH).abs() < 1e-5);
    }

    #[test]
    fn idle_spin_starts_after_the_delay() {
        let mut camera = GlobeCamera::default();
        for _ in 0..79 {
            camera.update(0.05, &GlobeInput::default());
        }
        assert_eq!(camera.yaw, 0.0, "до истечения паузы глобус неподвижен");
        assert!(camera.idle_countdown().is_some());

        for _ in 0..3 {
            camera.update(0.05, &GlobeInput::default());
        }
        let delta = shortest_delta(0.0, camera.yaw);
        assert!(
            delta < 0.0 && delta > -0.01,
            "после паузы камера медленно смещается на запад: {delta}"
        );
        assert!(camera.is_animating());
        assert_eq!(camera.idle_countdown(), None);
    }

    #[test]
    fn idle_spin_stays_very_slow() {
        let mut camera = GlobeCamera::default();
        for _ in 0..100 {
            camera.update(0.05, &GlobeInput::default());
        }
        let start = camera.yaw;
        for _ in 0..200 {
            camera.update(0.05, &GlobeInput::default());
        }
        let delta = shortest_delta(start, camera.yaw).abs();
        assert!(camera.yaw.is_finite());
        assert!(
            (0.02..0.09).contains(&delta),
            "за 10 секунд примерно 0.06 рад, получено {delta}"
        );
    }

    #[test]
    fn interaction_resets_the_idle_timer() {
        let mut camera = GlobeCamera::default();
        for _ in 0..100 {
            camera.update(0.05, &GlobeInput::default());
        }
        assert!(camera.is_idle_spinning());

        camera.update(
            0.05,
            &GlobeInput {
                drag_delta: Vec2::new(6.0, 0.0),
                ..GlobeInput::default()
            },
        );
        assert_eq!(camera.idle, 0.0);
        assert!(!camera.is_idle_spinning());
        assert!(camera.idle_countdown().is_some_and(|left| left > 3.9));
    }

    #[test]
    fn idle_uses_real_elapsed_time() {
        let mut camera = GlobeCamera::default();
        // `stable_dt` после сна маленький, фактический интервал — большой.
        camera.advance(0.016, 4.0, &GlobeInput::default());

        assert!(camera.is_idle_spinning());
        assert!(camera.is_animating());
    }

    #[test]
    fn fly_resets_the_idle_timer() {
        let mut camera = GlobeCamera::default();
        camera.fly_to(0.0, 10.0, Duration::from_millis(200));
        for _ in 0..10 {
            camera.update(0.05, &GlobeInput::default());
        }

        assert!(!camera.is_idle_spinning());
        assert!(camera.idle_countdown().is_some_and(|left| left > 3.5));
    }

    #[test]
    fn projected_radius_fits_default_framing() {
        let camera = GlobeCamera::default();
        let radius = camera.projected_radius(viewport());

        assert!(radius.is_finite() && radius > 0.0);
        assert!(
            radius < viewport().height() * 0.5,
            "дефолтная дистанция должна оставлять место кольцу за планетой"
        );
    }

    #[test]
    fn projected_radius_grows_when_zooming_in() {
        let near = GlobeCamera {
            distance: 2.0,
            ..GlobeCamera::default()
        };
        let far = GlobeCamera {
            distance: 4.0,
            ..GlobeCamera::default()
        };

        assert!(near.projected_radius(viewport()) > far.projected_radius(viewport()));
        assert_eq!(GlobeCamera::default().projected_radius(Rect::NOTHING), 0.0);
    }

    #[test]
    fn uniform_carries_eye_and_light() {
        let camera = GlobeCamera::default();
        let uniform = camera.uniform_with_motion(1.0, &GlobeMotion::default());

        let eye = camera.eye();
        assert_eq!(&uniform.camera_position[..3], &[eye.x, eye.y, eye.z]);

        let light = Vec3::from_slice(&uniform.light_direction[..3]);
        assert!((light.length() - 1.0).abs() < 1e-5);
        assert!(
            uniform
                .view_proj
                .iter()
                .flatten()
                .all(|value| value.is_finite())
        );
        assert_eq!(uniform.motion, [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(uniform.motion2, [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn uniform_keeps_motion_in_pod_layout() {
        assert_eq!(std::mem::size_of::<CameraUniform>(), 128);
        assert_eq!(std::mem::size_of::<CameraUniform>() % 16, 0);
        assert_eq!(std::mem::align_of::<CameraUniform>(), 4);

        let camera = GlobeCamera::default();
        let motion = GlobeMotion {
            deform: GlobeDeform::new(0.98),
            ripple: 0.002,
            ripple_phase: 0.75,
            flash: 0.08,
            beat_phase: 0.3,
            time: 9.5,
            ..GlobeMotion::default()
        };
        let uniform = camera.uniform_with_motion(1.0, &motion);

        assert!((uniform.motion[0] + 0.02).abs() < 1e-6);
        assert_eq!(uniform.motion[2], 0.002);
        assert_eq!(uniform.motion[3], 0.75);
        assert_eq!(uniform.motion2, [0.08, 0.0, 0.3, 9.5]);
    }

    #[test]
    fn deformed_hit_test_matches_projected_surface() {
        let camera = GlobeCamera::default();
        let viewport = viewport();
        let deform = GlobeDeform::new(0.97);
        let unit = Vec3::Z;

        let world = deform.to_world(unit);
        let screen = camera
            .project_on_sphere(world, viewport)
            .expect("деформированная точка перед камерой");
        let hit = camera
            .screen_to_sphere_deformed(screen, viewport, &deform)
            .expect("луч должен пересекать единичную сферу");

        assert!((hit - unit).length() < 1e-3, "hit: {hit:?}");
    }
}
