//! Стеклянные поверхности и pill-контролы: панель, переключатель, чипы, вкладки.

use super::content::contrast_color;
use super::prelude::*;
use super::shared::*;

/// Толщина верхнего блика стекла.
const GLASS_RIM: f32 = 1.5;

/// Стеклянная панель: вертикальный градиент `glass_top → glass_bottom`,
/// верхний блик 1–2 px и мягкая рамка. `egui::Frame` умеет только сплошную
/// заливку, а `Shape::gradient_rect` не скругляет углы, поэтому фон — один
/// меш по контуру скруглённого прямоугольника.
pub(crate) fn glass_panel(
    painter: &egui::Painter,
    rect: egui::Rect,
    palette: &Palette,
    radius: f32,
    alpha: f32,
) {
    if !rect.is_finite() || rect.width() < 2.0 || rect.height() < 2.0 {
        return;
    }
    let rounded = RoundedRect::new(rect, radius);
    painter.add(Shape::mesh(gradient_fill(
        rounded,
        faded(palette.glass_top, alpha),
        faded(palette.glass_bottom, alpha),
    )));
    painter.add(Shape::line(
        top_rim(rounded),
        PathStroke::new(GLASS_RIM, faded(palette.rim_light, alpha)),
    ));
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(1.0, faded(palette.card_stroke, alpha)),
        egui::StrokeKind::Middle,
    );
}

/// Pill-кнопка сегментированного переключателя (Country / Genre).
pub(crate) fn pill(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    selected: bool,
) -> egui::Response {
    let text_color = if selected {
        contrast_color(palette.accent)
    } else {
        palette.text
    };
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(13.0),
        text_color,
    );
    let size = Vec2::new(galley.size().x + 28.0, 26.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let (hover, press) = control_motion(
        ui,
        response.id,
        response.hovered(),
        response.is_pointer_button_down_on(),
    );
    if ui.is_rect_visible(rect) {
        let (fill, stroke) = if selected {
            (
                mix_colors(palette.accent, Color32::WHITE, HOVER_LIFT * hover),
                Stroke::NONE,
            )
        } else {
            (
                mix_colors(
                    Color32::TRANSPARENT,
                    palette.chip_fill,
                    hover.max(0.45 * press),
                ),
                Stroke::new(1.0, faded(palette.chip_stroke, hover)),
            )
        };
        ui.painter()
            .rect(rect, PILL_RADIUS, fill, stroke, egui::StrokeKind::Middle);
        ui.painter()
            .galley(rect.center() - galley.size() * 0.5, galley, text_color);
    }
    focus_ring(ui, &response, rect, PILL_RADIUS);
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, true, selected, label));
    response
}

/// Жанровый чип (All / Rock / Pop / …).
pub(crate) fn chip(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    selected: bool,
) -> egui::Response {
    let text_color = if selected {
        contrast_color(palette.accent)
    } else {
        palette.text
    };
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(11.5),
        text_color,
    );
    let size = Vec2::new(galley.size().x + 18.0, 22.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let (hover, press) = control_motion(
        ui,
        response.id,
        response.hovered(),
        response.is_pointer_button_down_on(),
    );
    if ui.is_rect_visible(rect) {
        let (fill, stroke) = if selected {
            (
                mix_colors(palette.accent, Color32::WHITE, 0.08 * hover),
                Stroke::NONE,
            )
        } else {
            (
                mix_colors(
                    palette.chip_fill,
                    palette.accent,
                    0.14 * hover + 0.06 * press,
                ),
                Stroke::new(
                    1.0,
                    mix_colors(palette.chip_stroke, palette.accent, 0.65 * hover),
                ),
            )
        };
        ui.painter()
            .rect(rect, PILL_RADIUS, fill, stroke, egui::StrokeKind::Middle);
        ui.painter()
            .galley(rect.center() - galley.size() * 0.5, galley, text_color);
    }
    focus_ring(ui, &response, rect, PILL_RADIUS);
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, selected, label));
    response
}

/// Однорядный сегментированный переключатель вкладок. Активный сегмент
/// подсвечен accent-пилюлей, которая плавно переезжает к новому выбору.
///
/// Пилюля живёт только в отрисовке: каждый сегмент занимает фиксированную
/// долю ширины, поэтому хиттест не зависит от анимации (ADR-0032 §7).
/// Возвращает индекс нажатого сегмента.
pub(crate) fn segmented_tabs(
    ui: &mut egui::Ui,
    palette: &Palette,
    id_salt: &str,
    labels: &[&str],
    selected: usize,
    reduce: bool,
) -> Option<usize> {
    if labels.is_empty() {
        return None;
    }
    let selected = selected.min(labels.len() - 1);
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return None;
    }

    let track = rect.shrink(0.5);
    ui.painter()
        .rect_filled(track, PILL_RADIUS, palette.chip_fill);
    ui.painter().rect_stroke(
        track,
        PILL_RADIUS,
        Stroke::new(1.0, palette.chip_stroke),
        egui::StrokeKind::Inside,
    );

    let inner = track.shrink(3.0);
    let segment = inner.width() / labels.len() as f32;
    let position = if reduce {
        selected as f32
    } else {
        ui.ctx().animate_value_with_time(
            ui.scope_id().with((id_salt, "pill")),
            selected as f32,
            animation::TAB,
        )
    };
    let pill = egui::Rect::from_min_size(
        pos2(inner.left() + segment * position, inner.top()),
        Vec2::new(segment, inner.height()),
    );
    ui.painter().rect_filled(pill, PILL_RADIUS, palette.accent);

    let mut clicked = None;
    for (index, label) in labels.iter().enumerate() {
        let cell = egui::Rect::from_min_size(
            pos2(inner.left() + segment * index as f32, inner.top()),
            Vec2::new(segment, inner.height()),
        );
        let response = ui.interact(
            cell,
            ui.scope_id().with((id_salt, index)),
            egui::Sense::click(),
        );
        let active = index == selected;
        let (hover, press) = control_motion(
            ui,
            response.id,
            response.hovered(),
            response.is_pointer_button_down_on(),
        );
        if !active && (hover > 0.0 || press > 0.0) {
            ui.painter().rect_filled(
                cell.shrink(1.0),
                PILL_RADIUS,
                faded(palette.text, 0.05 * hover + 0.04 * press),
            );
        }
        let color = if active {
            contrast_color(palette.accent)
        } else {
            mix_colors(palette.muted, palette.text, hover)
        };
        ui.painter().text(
            cell.center(),
            egui::Align2::CENTER_CENTER,
            *label,
            egui::FontId::proportional(12.5),
            color,
        );
        focus_ring(ui, &response, cell.shrink(1.0), PILL_RADIUS);
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::Tab, true, index == selected, *label)
        });
        if response.clicked() {
            clicked = Some(index);
        }
    }
    clicked
}
