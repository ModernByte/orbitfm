//! Общие импорты подмодулей виджетов.

pub(super) use eframe::egui::{
    self, Color32, Pos2, Shape, Stroke, TextureHandle, Vec2,
    epaint::{PathStroke, RoundedRect, tessellator},
    pos2,
};

pub(super) use crate::i18n;
pub(super) use crate::render::bar_color;
pub(super) use crate::settings::{Language, sanitized_volume};

pub(super) use crate::ui::animation;
pub(super) use crate::ui::icons;
pub(super) use crate::ui::palette::{Palette, mix_colors};
