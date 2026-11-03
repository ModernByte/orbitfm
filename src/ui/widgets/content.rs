//! Содержимое списков и карточек: заглушки, ошибки, скелетон, аватар, знак.

use super::buttons::action_pill;
use super::prelude::*;
use super::shared::*;

/// Кадры пульсации костяка списка.
const SKELETON_REPAINT: std::time::Duration = std::time::Duration::from_millis(110);

/// Знак центрированной заглушки списка.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EmptyMark {
    Globe,
    Search,
    Star,
    Clock,
    Filter,
}

#[derive(Clone, Copy)]
pub(crate) struct EmptyState<'a> {
    pub mark: EmptyMark,
    pub title: &'a str,
    pub hint: Option<&'a str>,
    pub action: Option<&'a str>,
}

/// Центрированная заглушка списка: знак в круге, заголовок, подсказка и
/// необязательное действие. `height` — высота вьюпорта списка: заглушка
/// центрируется по ней, а не по растущему содержимому `ScrollArea`.
/// Возвращает true, если действие нажато.
pub(crate) fn empty_state(
    ui: &mut egui::Ui,
    palette: &Palette,
    state: EmptyState<'_>,
    height: f32,
) -> bool {
    let EmptyState {
        mark,
        title,
        hint,
        action,
    } = state;
    let height = height.max(120.0);
    let width = ui.available_width();
    let block = 96.0 + hint.map_or(0.0, |_| 34.0) + action.map_or(0.0, |_| 42.0);
    let top = ((height - block) * 0.34).clamp(6.0, 180.0);
    let mut clicked = false;
    ui.allocate_ui_with_layout(
        Vec2::new(width, height),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            ui.add_space(top);
            ui.set_max_width((width - 36.0).max(120.0));
            ui.vertical_centered(|ui| {
                let (mark_rect, _) =
                    ui.allocate_exact_size(Vec2::splat(52.0), egui::Sense::hover());
                if ui.is_rect_visible(mark_rect) {
                    let center = mark_rect.center();
                    ui.painter().circle_filled(center, 25.0, palette.chip_fill);
                    ui.painter()
                        .circle_stroke(center, 25.0, Stroke::new(1.0, palette.chip_stroke));
                    let (icon, size) = match mark {
                        EmptyMark::Globe => (icons::Icon::Globe, 22.0),
                        EmptyMark::Search => (icons::Icon::Search, 21.0),
                        EmptyMark::Star => (icons::Icon::Star, 22.0),
                        EmptyMark::Clock => (icons::Icon::Clock, 22.0),
                        EmptyMark::Filter => (icons::Icon::Funnel, 21.0),
                    };
                    icons::paint_icon(ui, icon, center, size, palette.accent);
                }
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new(title)
                        .size(14.0)
                        .strong()
                        .color(palette.text),
                );
                if let Some(hint) = hint {
                    ui.add_space(3.0);
                    ui.add(
                        egui::Label::new(egui::RichText::new(hint).size(11.5).color(palette.muted))
                            .wrap()
                            .halign(egui::Align::Center),
                    );
                }
                if let Some(action) = action {
                    ui.add_space(12.0);
                    clicked = action_pill(ui, palette, action).clicked();
                }
            });
        },
    );
    clicked
}

/// Карточка ошибки списка с кнопкой повтора. Возвращает клик по Retry.
pub(crate) fn error_card(
    ui: &mut egui::Ui,
    palette: &Palette,
    title: &str,
    detail: &str,
    retry: &str,
) -> bool {
    let mut clicked = false;
    egui::Frame::new()
        .fill(faded(palette.error, 0.08))
        .stroke(Stroke::new(1.0, faded(palette.error, 0.35)))
        .corner_radius(12)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (icon_rect, _) =
                    ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
                icons::paint_icon(
                    ui,
                    icons::Icon::Alert,
                    icon_rect.center(),
                    16.0,
                    palette.error,
                );
                ui.label(
                    egui::RichText::new(title)
                        .size(13.0)
                        .strong()
                        .color(palette.error),
                );
            });
            if !detail.is_empty() {
                ui.add_space(3.0);
                ui.add(
                    egui::Label::new(egui::RichText::new(detail).size(11.0).color(palette.muted))
                        .wrap(),
                );
            }
            ui.add_space(10.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                clicked = action_pill(ui, palette, retry).clicked();
            });
        });
    clicked
}

/// Костяк списка на время поиска: аватары и строки пульсируют одной фазой.
/// `height` — высота вьюпорта; `label` уходит в accessibility вместо текста.
pub(crate) fn skeleton_rows(
    ui: &mut egui::Ui,
    palette: &Palette,
    compact: bool,
    height: f32,
    label: &str,
    reduce: bool,
) {
    let row_height = if compact { 42.0 } else { 58.0 };
    let count = ((height / row_height).floor() as usize).clamp(2, 8);
    let time = ui.input(|input| input.time) as f32;
    for index in 0..count {
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), row_height),
            egui::Sense::hover(),
        );
        if index == 0 {
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, false, label));
        }
        if !ui.is_rect_visible(rect) {
            continue;
        }
        let phase = if reduce {
            0.0
        } else {
            (time * 2.4 + index as f32 * 0.45).sin().abs()
        };
        let bone = faded(palette.text, 0.05 + 0.05 * phase);
        let avatar_radius = if compact { 13.0 } else { 18.0 };
        let avatar_center = pos2(rect.left() + 6.0 + avatar_radius, rect.center().y);
        ui.painter()
            .circle_filled(avatar_center, avatar_radius, bone);
        let text_left = avatar_center.x + avatar_radius + 10.0;
        let name_y = rect.center().y - if compact { 4.0 } else { 7.0 };
        let width = (rect.right() - text_left - 42.0).max(40.0);
        ui.painter().rect_filled(
            egui::Rect::from_min_size(pos2(text_left, name_y - 4.0), Vec2::new(width * 0.55, 8.0)),
            4.0,
            bone,
        );
        ui.painter().rect_filled(
            egui::Rect::from_min_size(pos2(text_left, name_y + 7.0), Vec2::new(width * 0.32, 6.0)),
            3.0,
            bone,
        );
        ui.painter().rect_filled(
            egui::Rect::from_center_size(
                pos2(rect.right() - 16.0, rect.center().y),
                Vec2::splat(9.0),
            ),
            3.0,
            bone,
        );
    }
    if !reduce {
        ui.ctx().request_repaint_after(SKELETON_REPAINT);
    }
}

/// Круглый аватар: favicon или инициал станции.
pub(crate) fn avatar(
    ui: &mut egui::Ui,
    palette: &Palette,
    texture: Option<TextureHandle>,
    name: &str,
    size: f32,
) -> egui::Rect {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return rect;
    }
    let center = rect.center();
    let radius = size * 0.5;
    if let Some(texture) = texture {
        const SEGMENTS: usize = 32;
        let mut mesh = egui::Mesh::with_texture(texture.id());
        mesh.vertices.push(egui::epaint::Vertex {
            pos: center,
            uv: pos2(0.5, 0.5),
            color: Color32::WHITE,
        });
        for segment in 0..=SEGMENTS {
            let angle = segment as f32 * std::f32::consts::TAU / SEGMENTS as f32;
            let point = center + Vec2::new(angle.cos(), angle.sin()) * radius;
            let uv = (point - rect.min) / size;
            mesh.vertices.push(egui::epaint::Vertex {
                pos: point,
                uv: pos2(uv.x, uv.y),
                color: Color32::WHITE,
            });
        }
        for segment in 1..=SEGMENTS as u32 {
            mesh.add_triangle(0, segment, segment + 1);
        }
        ui.painter().add(Shape::mesh(mesh));
    } else {
        ui.painter()
            .circle_filled(center, radius, palette.chip_fill);
        let initial = name
            .trim()
            .chars()
            .next()
            .map_or_else(|| "?".to_owned(), |c| c.to_uppercase().to_string());
        ui.painter().text(
            center,
            egui::Align2::CENTER_CENTER,
            initial,
            egui::FontId::proportional(size * 0.45),
            palette.text,
        );
    }
    ui.painter()
        .circle_stroke(center, radius, Stroke::new(1.0, palette.card_stroke));
    rect
}

/// Флаг-эмодзи по ISO alpha-2; пустая строка при невалидном коде.
pub(crate) fn flag_emoji(iso_a2: &str) -> String {
    let code: Vec<char> = iso_a2
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .collect();
    if code.len() != 2 {
        return String::new();
    }
    code.iter()
        .filter_map(|c| {
            char::from_u32(0x0001_F1E6 + (c.to_ascii_uppercase() as u32 - 'A' as u32))
                .filter(|c| !c.is_ascii())
        })
        .collect()
}

pub(crate) fn contrast_color(color: Color32) -> Color32 {
    let luma =
        0.299 * f32::from(color.r()) + 0.587 * f32::from(color.g()) + 0.114 * f32::from(color.b());
    if luma > 150.0 {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

/// Знак OrbitFM: планета с орбитальным кольцом и точкой-станцией.
pub(crate) fn logo_mark(ui: &mut egui::Ui, palette: &Palette, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    let center = rect.center();
    let radius = size * 0.32;
    painter.circle_stroke(center, radius, Stroke::new(1.6, palette.accent));
    let ring = Vec2::new(radius * 1.25, radius * 0.5);
    let tilt: f32 = -0.5;
    let ring_points: Vec<Pos2> = (0..=48)
        .map(|step| {
            let angle = step as f32 * std::f32::consts::TAU / 48.0;
            let (s, c) = (angle.sin(), angle.cos());
            pos2(
                center.x + ring.x * c * tilt.cos() - ring.y * s * tilt.sin(),
                center.y + ring.x * c * tilt.sin() + ring.y * s * tilt.cos(),
            )
        })
        .collect();
    painter.add(Shape::closed_line(
        ring_points,
        Stroke::new(1.2, palette.chip_stroke),
    ));
    painter.circle_filled(
        center + Vec2::new(ring.x * 0.7, -radius * 0.55),
        2.2,
        palette.live,
    );
}
