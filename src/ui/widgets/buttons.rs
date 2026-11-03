//! Кнопки: круглые диски, капсулы и pill-действия.

use super::content::contrast_color;
use super::prelude::*;
use super::shared::*;

/// Большая круглая play/pause с accent-кольцом.
pub(crate) fn circle_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    playing: bool,
    enabled: bool,
    hint: &str,
) -> egui::Response {
    circle_button_sized(ui, palette, 40.0, playing, enabled, hint)
}

/// Та же кнопка заданного диаметра: капсулы используют меньший размер.
pub(crate) fn circle_button_sized(
    ui: &mut egui::Ui,
    palette: &Palette,
    size: f32,
    playing: bool,
    enabled: bool,
    hint: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::splat(size),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    if ui.is_rect_visible(rect) {
        let center = rect.center();
        // ТЗ §3.4: primary play — сплошной accent, чуть светлее при наведении.
        let hoverable = response.hovered() && enabled;
        let fill = if !enabled {
            palette.chip_fill
        } else if hoverable {
            lift(palette.accent, HOVER_LIFT)
        } else {
            palette.accent
        };
        let ring = if enabled {
            lift(palette.accent, RING_LIFT)
        } else {
            palette.muted
        };
        let glyph = if enabled {
            contrast_color(fill)
        } else {
            palette.muted
        };
        let radius = draw_disc(ui, rect, 1.0, fill, ring, glyph, playing);
        if radius > 0.0 {
            circle_focus_ring(ui, &response, center, radius + 2.0);
        }
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, hint));
    if enabled {
        response.on_hover_text(hint)
    } else {
        response
    }
}

/// Капсульная кнопка play/stop на один кадр: всё, что нужно для её отрисовки.
///
/// Поля посчитаны вызывающим — переход цвета между состояниями воспроизведения
/// и общий фэйд входа принадлежат капсуле, а не виджету.
pub(crate) struct CapsuleDisc<'a> {
    /// Палитра уже погашена фэйдом входа: из неё берутся лишь приглушённые
    /// цвета неактивной кнопки.
    pub palette: &'a Palette,
    /// Диаметр аллокации; шкала живёт только в отрисовке.
    pub size: f32,
    /// Шкала появления из [`pop_scale`].
    pub pop: f32,
    /// Цвет диска: смешение «пауза → эфир» с альфой входа.
    pub fill: Color32,
    pub playing: bool,
    pub enabled: bool,
    pub hint: &'a str,
}

/// Капсульная кнопка play/stop: появление с overshoot (`pop`), press-сжатие по
/// [`animation::PRESS`] и цвет диска из [`CapsuleDisc`].
///
/// Аллокация всегда [`Vec2::splat(disc.size)`]: шкала живёт только в отрисовке,
/// поэтому hit-testing не смещается (ADR-0032 §7).
pub(crate) fn capsule_button(
    ui: &mut egui::Ui,
    motion: &mut animation::FrameMotion<'_>,
    press_key: egui::Id,
    disc: &CapsuleDisc<'_>,
) -> egui::Response {
    let CapsuleDisc {
        palette,
        size,
        pop,
        fill,
        playing,
        enabled,
        hint,
    } = *disc;
    let (rect, response) = ui.allocate_exact_size(
        Vec2::splat(size),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    if ui.is_rect_visible(rect) {
        let center = rect.center();
        let mut scale = pop.max(0.0);
        // Reduce motion глушит шкалу: переход не опрашивается вовсе, чтобы не
        // занимать слот в PresenceStore окна.
        if !motion.reduce {
            let pressed = response.is_pointer_button_down_on();
            let progress = motion.keyed.value(
                ui.ctx(),
                press_key,
                pressed,
                animation::PRESS,
                motion.reduce,
            );
            scale *= pressed_scale(progress);
        }
        let hoverable = response.hovered() && enabled;
        let fill = if !enabled {
            palette.chip_fill
        } else if hoverable {
            lift(fill, HOVER_LIFT)
        } else {
            fill
        };
        let ring = if enabled {
            lift(fill, RING_LIFT)
        } else {
            palette.muted
        };
        let glyph = if enabled {
            // Глиф наследует альфу диска: погасшая при входе капсула не должна
            // светиться иконкой. Выбор контраста берём из непрозрачного оттенка —
            // у premultiplied-каналов погасшего диска luminance ниже, и глиф мог
            // бы перевернуться посреди входа.
            let [r, g, b, _] = fill.to_srgba_unmultiplied();
            faded(
                contrast_color(Color32::from_rgb(r, g, b)),
                f32::from(fill.a()) / 255.0,
            )
        } else {
            palette.muted
        };
        let radius = draw_disc(ui, rect, scale, fill, ring, glyph, playing);
        if radius > 0.0 {
            circle_focus_ring(ui, &response, center, radius + 2.0);
        }
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, hint));
    if enabled {
        response.on_hover_text(hint)
    } else {
        response
    }
}

/// Диск с глифом play/pause, нарисованный от центра `rect` с множителем `scale`.
/// Возвращает радиус отрисованного диска (0.0 — рисовать нечего).
fn draw_disc(
    ui: &egui::Ui,
    rect: egui::Rect,
    scale: f32,
    fill: Color32,
    ring: Color32,
    glyph: Color32,
    playing: bool,
) -> f32 {
    let center = rect.center();
    let radius = (rect.width() * 0.5 - 1.0) * scale;
    if radius <= 0.0 {
        return 0.0;
    }
    ui.painter().circle_filled(center, radius, fill);
    ui.painter()
        .circle_stroke(center, radius, Stroke::new(1.6, ring));
    if playing {
        for dx in [-4.0, 4.0] {
            ui.painter().rect_filled(
                egui::Rect::from_center_size(
                    center + Vec2::new(dx * scale, 0.0),
                    Vec2::new(3.2, 12.0) * scale,
                ),
                1.5 * scale,
                glyph,
            );
        }
    } else {
        let point = |dx: f32, dy: f32| center + Vec2::new(dx, dy) * scale;
        ui.painter().add(Shape::convex_polygon(
            vec![point(-4.5, -7.0), point(-4.5, 7.0), point(7.0, 0.0)],
            glyph,
            Stroke::NONE,
        ));
    }
    radius
}

/// Press-сжатие: 1.0 в покое, [`animation::PRESS_SCALE`] при удержании.
pub(super) fn pressed_scale(progress: f32) -> f32 {
    1.0 - (1.0 - animation::PRESS_SCALE) * animation::alpha(progress)
}

/// Осветление к белому без изменения альфы: hover-подсветка не должна
/// «пробивать» фэйд входа капсулы. Каналы берём в unmultiplied-виде, потому что
/// `Color32` хранит их уже умноженными на альфу.
pub(super) fn lift(color: Color32, amount: f32) -> Color32 {
    let [r, g, b, _] = mix_colors(color, Color32::WHITE, amount).to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, color.a())
}

/// Accent-кнопка действия: pill-форма, светлеет при наведении.
pub(crate) fn action_pill(ui: &mut egui::Ui, palette: &Palette, label: &str) -> egui::Response {
    let font = egui::FontId::proportional(12.5);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), palette.text);
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(galley.size().x + 30.0, 30.0),
        egui::Sense::click(),
    );
    if ui.is_rect_visible(rect) {
        let (hover, _) = control_motion(
            ui,
            response.id,
            response.hovered(),
            response.is_pointer_button_down_on(),
        );
        let fill = lift(palette.accent, HOVER_LIFT * hover);
        ui.painter().rect_filled(rect, PILL_RADIUS, fill);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            font,
            contrast_color(fill),
        );
    }
    focus_ring(ui, &response, rect, PILL_RADIUS);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    response
}

/// Малошумная кнопка на всю ширину: подложка появляется только на hover.
pub(crate) fn quiet_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    color: Color32,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let (hover, _) = control_motion(ui, response.id, response.hovered(), false);
        if hover > 0.0 {
            ui.painter()
                .rect_filled(rect, 8.0, faded(palette.chip_fill, hover));
        }
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(12.0),
            color,
        );
    }
    focus_ring(ui, &response, rect, 8.0);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    response
}
