//! Интерактивный 3D-глобус: камера, wgpu-рендер и интеграция с egui.

mod camera;
mod countries;
mod globe;
mod gpu;
mod heatmap;
mod markers;
mod motion;
mod spectrum;
mod visualizer;

use std::fmt;

use eframe::egui::{Pos2, Vec2};
use egui_wgpu::wgpu;

pub use camera::GlobeCamera;
pub use countries::{CountryStyle, paint_country};
pub use globe::{GlobeRenderer, paint_globe};
pub use gpu::wgpu_options;
pub(crate) use heatmap::{CountryColor, HEAT_SIZE, HeatFrame, build as build_heat_map};
pub use markers::{MarkerLabel, MarkerVisual, paint_markers, sphere_point};
pub use motion::{GlobeDeform, GlobeMotion, MotionMixer};
pub use spectrum::{BandTargets, SpectrumOverlay, SpectrumTarget};
pub(crate) use spectrum::{MIN_RMS_DB, RMS_RANGE_DB};
pub use visualizer::{VisualizerModel, bar_color};

/// Ввод пользователя для глобуса.
#[derive(Debug, Clone, Default)]
pub struct GlobeInput {
    /// Смещение при перетаскивании правой кнопкой мыши.
    pub drag_delta: Vec2,
    /// Прокрутка колеса для зума.
    pub scroll_delta: f32,
    /// Клик левой кнопкой в экранных координатах.
    pub primary_clicked: Option<Pos2>,
    /// Курсор над глобусом: авто-движение плавно замирает.
    pub hovered: bool,
    /// Высота области глобуса в пунктах: из неё считается фокусное расстояние
    /// для перетаскивания 1:1 (ADR-0060 § 3). Ноль — раскладки нет, камера
    /// берёт номинальную высоту.
    pub viewport_height: f32,
}

/// Что приложение видит про устройство вывода при старте (ADR-0052 § 3).
///
/// Это лог, а не публичный интерфейс: `status --json` отчётом не пользуется.
/// Поля — только строки и скаляры, чтобы формат строки закреплялся unit-тестом
/// на любой машине, а не зависел от того, чей GPU попался в CI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterReport {
    /// Backend, которым реально рендерит wgpu: `Metal`, `Vulkan`, `Gl`, `Cpu`.
    pub backend: String,
    /// Имя активного устройства.
    pub device: String,
    /// Тип активного устройства: `IntegratedGpu`, `DiscreteGpu`, `Cpu`.
    pub device_type: String,
    /// Имя драйвера из `AdapterInfo`.
    pub driver: String,
    /// Версия/сборка драйвера из `AdapterInfo`.
    pub driver_info: String,
    /// Формат поверхности, в который рисуется UI.
    pub target_format: String,
    /// Эффективный лимит устройства: единственное измеримое требование ADR-0052 § 2.
    pub max_texture_dimension_2d: u32,
    /// Эффективный лимит размера буфера, байт.
    pub max_buffer_size: u64,
    /// Поддерживает ли устройство анизотропную фильтрацию. Глобус просит clamp 8
    /// (`globe.rs:27`), и без этого флага wgpu срезает её до 1 — на Intel это
    /// единственный downlevel-флаг, от которого зависит картинка.
    pub anisotropic_filtering: bool,
    /// Имена всех доступных adapter'ов: на машине с дискретной GPU по списку видно,
    /// какое именно устройство досталось окну.
    pub devices: Vec<String>,
}

impl AdapterReport {
    /// Собирает отчёт из состояния рендера, которое eframe передал окну.
    ///
    /// Ограничения берём у `device`, а не у `adapter`: устройство — то, с чем
    /// приложение реально работает, включая урезки и «buckets», которые применил wgpu.
    pub fn from_render_state(state: &egui_wgpu::RenderState) -> Self {
        let info = state.adapter.get_info();
        let limits = state.device.limits();
        let downlevel = state.adapter.get_downlevel_capabilities();
        Self {
            backend: format!("{:?}", info.backend),
            device: info.name,
            device_type: format!("{:?}", info.device_type),
            driver: info.driver,
            driver_info: info.driver_info,
            target_format: format!("{:?}", state.target_format),
            max_texture_dimension_2d: limits.max_texture_dimension_2d,
            max_buffer_size: limits.max_buffer_size,
            anisotropic_filtering: downlevel
                .flags
                .contains(wgpu::DownlevelFlags::ANISOTROPIC_FILTERING),
            devices: state
                .available_adapters
                .iter()
                .map(|adapter| adapter.get_info().name)
                .collect(),
        }
    }
}

impl fmt::Display for AdapterReport {
    /// Одна строка с префиксом `orbitfm render:`: идентификаторы без кавычек,
    /// свободный текст — через `{:?}`, чтобы пробелы не ломали разбор.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "orbitfm render: backend={} device={:?} device_type={} driver={:?} driver_info={:?} \
             target_format={} max_texture_dimension_2d={} max_buffer_size={} \
             anisotropic_filtering={} devices=[",
            self.backend,
            self.device,
            self.device_type,
            self.driver,
            self.driver_info,
            self.target_format,
            self.max_texture_dimension_2d,
            self.max_buffer_size,
            if self.anisotropic_filtering {
                "yes"
            } else {
                "no"
            },
        )?;
        for (index, name) in self.devices.iter().enumerate() {
            if index > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{name:?}")?;
        }
        write!(f, "]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(devices: &[&str]) -> AdapterReport {
        AdapterReport {
            backend: "Metal".to_string(),
            device: "Apple M1".to_string(),
            device_type: "IntegratedGpu".to_string(),
            driver: "Metal".to_string(),
            driver_info: "gpu-assembly-187.9".to_string(),
            target_format: "Bgra8UnormSrgb".to_string(),
            max_texture_dimension_2d: 8192,
            max_buffer_size: 268_435_456,
            anisotropic_filtering: true,
            devices: devices.iter().map(|name| (*name).to_string()).collect(),
        }
    }

    #[test]
    fn render_report_formats_single_device() {
        let line = report(&["Apple M1"]).to_string();
        assert_eq!(
            line,
            "orbitfm render: backend=Metal device=\"Apple M1\" device_type=IntegratedGpu \
             driver=\"Metal\" driver_info=\"gpu-assembly-187.9\" target_format=Bgra8UnormSrgb \
             max_texture_dimension_2d=8192 max_buffer_size=268435456 anisotropic_filtering=yes \
             devices=[\"Apple M1\"]"
        );
    }

    /// Список устройств печатается даже когда их несколько: именно так в логе
    /// живого Mac видно, какое устройство выбрано из двух.
    #[test]
    fn render_report_lists_every_device() {
        let line = report(&["Intel UHD Graphics 630", "Radeon Pro 555X"]).to_string();
        assert!(
            line.ends_with("devices=[\"Intel UHD Graphics 630\", \"Radeon Pro 555X\"]"),
            "неожиданный хвост строки: {line}"
        );
    }

    #[test]
    fn render_report_marks_missing_anisotropy() {
        let mut without_aniso = report(&["Apple M1"]);
        without_aniso.anisotropic_filtering = false;
        assert!(
            without_aniso
                .to_string()
                .contains("anisotropic_filtering=no")
        );
    }
}
