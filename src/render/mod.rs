//! Интерактивный 3D-глобус: камера, wgpu-рендер и интеграция с egui.

mod camera;
mod countries;
mod globe;
mod markers;
mod spectrum;
mod visualizer;

use eframe::egui::{Pos2, Vec2};

pub use camera::GlobeCamera;
pub use countries::{CountryStyle, paint_country};
pub use globe::{GlobeRenderer, paint_globe};
pub use markers::{MarkerLabel, MarkerVisual, paint_markers, sphere_point};
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
}
