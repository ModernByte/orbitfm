//! Интерактивный 3D-глобус: камера, wgpu-рендер и интеграция с egui.

mod camera;
mod globe;
mod spectrum;

use eframe::egui::{Pos2, Vec2};

pub use camera::GlobeCamera;
pub use globe::{GlobeRenderer, paint_globe};
pub use spectrum::{SpectrumOverlay, SpectrumTarget};

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
