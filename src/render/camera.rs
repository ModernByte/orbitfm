//! Камера глобуса: орбитальное вращение, зум и преобразование экран → сфера.

use std::time::Duration;

use eframe::egui::{Pos2, Rect, Vec2};
use glam::camera::rh::{proj::directx, view};
use glam::{Mat4, Vec3, Vec4};

use super::GlobeInput;

const FOV_Y_RADIANS: f32 = std::f32::consts::FRAC_PI_4;
const NEAR_PLANE: f32 = 0.1;
const FAR_PLANE: f32 = 50.0;

const DEFAULT_DISTANCE: f32 = 2.8;
const MIN_DISTANCE: f32 = 1.4;
const MAX_DISTANCE: f32 = 6.0;
const ZOOM_SENSITIVITY: f32 = 0.002;

const DRAG_SENSITIVITY: f32 = 0.006;
const INERTIA_DAMPING: f32 = 6.0;
const INERTIA_STOP: f32 = 0.01;
const MAX_PITCH: f32 = 85.0 * std::f32::consts::PI / 180.0;
const MIN_FLY_DURATION: f32 = 0.05;

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
    /// `x` — сигнал эфира 0..1 для Fresnel-обода.
    pub pulse: [f32; 4],
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
    }

    /// Обновляет камеру по вводу за кадр.
    pub fn update(&mut self, dt: f32, input: &GlobeInput) {
        let dt = dt.clamp(0.001, 0.05);

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
    }

    /// Идёт ли инерционное движение или доворот, требующие перерисовки.
    pub fn is_animating(&self) -> bool {
        self.fly.is_some() || self.yaw_velocity != 0.0 || self.pitch_velocity != 0.0
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

    /// Пересечение луча из экранной точки со сферой радиусом 1.
    pub fn screen_to_sphere(&self, screen_pos: Pos2, viewport: Rect) -> Option<Vec3> {
        if viewport.width() <= 0.0 || viewport.height() <= 0.0 || !viewport.contains(screen_pos) {
            return None;
        }

        let ndc_x = (screen_pos.x - viewport.left()) / viewport.width() * 2.0 - 1.0;
        let ndc_y = 1.0 - (screen_pos.y - viewport.top()) / viewport.height() * 2.0;
        let inverse = self.view_proj(viewport.aspect_ratio()).inverse();

        let near = unproject(inverse, ndc_x, ndc_y, 0.0)?;
        let far = unproject(inverse, ndc_x, ndc_y, 1.0)?;

        let origin = self.eye();
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

    /// Проецирует точку единичной сферы в экранные координаты.
    ///
    /// Возвращает `None` для обратной полусферы, выхода за viewport и
    /// нечисловых координат.
    pub fn project_on_sphere(&self, point: Vec3, viewport: Rect) -> Option<Pos2> {
        if viewport.width() <= 0.0 || viewport.height() <= 0.0 {
            return None;
        }
        if point.dot(self.eye()) <= 1.0 {
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

    pub(crate) fn uniform(&self, aspect: f32, pulse: f32) -> CameraUniform {
        let light = Vec3::new(0.5, 0.35, 0.8).normalize();
        let eye = self.eye();
        let pulse = if pulse.is_finite() {
            pulse.clamp(0.0, 1.0)
        } else {
            0.0
        };
        CameraUniform {
            view_proj: self.view_proj(aspect).to_cols_array_2d(),
            light_direction: [light.x, light.y, light.z, 0.0],
            camera_position: [eye.x, eye.y, eye.z, 0.0],
            pulse: [pulse, 0.0, 0.0, 0.0],
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
            .screen_to_sphere(viewport().center(), viewport())
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
            .screen_to_sphere(viewport().center(), viewport())
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
            .project_on_sphere(sphere_point(52.52, 13.405), rect)
            .expect("Берлин должен быть виден");

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

        assert!(camera.screen_to_sphere(corner, viewport()).is_none());
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
            .screen_to_sphere(viewport().center(), viewport())
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
    fn uniform_carries_eye_and_clamps_pulse() {
        let camera = GlobeCamera::default();
        let uniform = camera.uniform(1.0, 2.5);

        assert_eq!(uniform.pulse[0], 1.0);
        let eye = camera.eye();
        assert_eq!(&uniform.camera_position[..3], &[eye.x, eye.y, eye.z]);

        assert_eq!(camera.uniform(1.0, f32::NAN).pulse[0], 0.0);
        assert_eq!(camera.uniform(1.0, -1.0).pulse[0], 0.0);
    }
}
