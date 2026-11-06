//! Окно предупреждения таймера сна (ADR-0058).
//!
//! Показывается за минуту до действия: обратный отсчёт, сегментное кольцо,
//! кнопка «Отменить» и строка действия. Нет кнопки «выключить сейчас».
//! Одна функция `show` на оба носителя: immediate-потомок стола/настроек
//! и отдельный прогон припаркованного процесса.

use eframe::egui;

use crate::i18n;
use crate::settings::{Language, SleepAction};
use crate::ui::palette::{Palette, mix_colors};
use crate::ui::widgets;

/// Действие пользователя в окне предупреждения.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WarningAction {
    /// «Отменить» / Esc / close_requested: действие отменяется, таймер
    /// досчитывает, громкость восстанавливается через CancelFade.
    Cancel,
}

/// Входные данные для одного кадра окна предупреждения.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WarningInputs<'a> {
    pub(crate) language: Language,
    pub(crate) palette: &'a Palette,
    pub(crate) remaining_secs: u64,
    pub(crate) action: SleepAction,
    pub(crate) close_requested: bool,
}

/// Пресет окна предупреждения: 420×320.
pub(crate) const WARNING_SIZE: [f32; 2] = [420.0, 320.0];
/// Нижний лимит размера окна предупреждения.
pub(crate) const WARNING_MIN_SIZE: [f32; 2] = [360.0, 280.0];

/// Число сегментов кольца обратного отсчёта.
const SEGMENTS: usize = 96;
/// Радиус кольца: ≈48 px по центру окна.
const RING_RADIUS: f32 = 48.0;
/// Толщина линии кольца.
const RING_STROKE: f32 = 3.0;

/// Цвет обратного отсчёта по нормализованному прогрессу `p` (1.0 = начало,
/// 0.0 = конец): p ≥ 0.5 → жёлтый→зелёный, иначе красный→жёлтый.
pub(crate) fn warning_color(p: f32) -> egui::Color32 {
    let p = p.clamp(0.0, 1.0);
    if p >= 0.5 {
        // Жёлтый → зелёный: осталось много времени.
        mix_colors(
            egui::Color32::from_rgb(0xFB, 0xBF, 0x24),
            egui::Color32::from_rgb(0x34, 0xD3, 0x99),
            (p - 0.5) * 2.0,
        )
    } else {
        // Красный → жёлтый: осталось мало времени.
        mix_colors(
            egui::Color32::from_rgb(0xEF, 0x44, 0x44),
            egui::Color32::from_rgb(0xFB, 0xBF, 0x24),
            p * 2.0,
        )
    }
}

/// Окно предупреждения: immediate-потомок стола или окна настроек, либо
/// отдельный прогон припаркованного процесса.
///
/// Возвращает действия пользователя: `Cancel` — отменить действие таймера,
/// досчитать отсчёт, вернуть громкость.
#[allow(deprecated)] // ui.id() — стабильный способ задать Id виджета в egui 0.37
pub(crate) fn show(
    ui: &mut egui::Ui,
    class: egui::ViewportClass,
    inputs: WarningInputs<'_>,
) -> Vec<WarningAction> {
    let mut actions = Vec::new();
    let palette = inputs.palette;
    let rect = ui.available_rect_before_wrap();

    // Glass-поверхность по всему rect + мягкая тень (цикл теней).
    widgets::alert_surface(ui.painter(), rect, 16.0, 1.0);
    widgets::glass_panel(ui.painter(), rect, palette, 16.0, 1.0);

    ui.set_min_size(rect.size());
    ui.set_max_size(rect.size());

    let center = rect.center();
    let remaining = remaining_secs_clamped(inputs.remaining_secs);
    let p = remaining as f32 / 60.0;

    // Кольцо — сегментная полилиния (arc-API в egui 0.37 нет).
    let ring_color = warning_color(p);
    let sweep = p * std::f32::consts::TAU;
    let mut points = Vec::with_capacity(SEGMENTS + 1);
    for index in 0..=SEGMENTS {
        let angle = index as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        if angle > sweep {
            break;
        }
        points.push(egui::pos2(
            center.x + angle.cos() * RING_RADIUS,
            center.y + angle.sin() * RING_RADIUS,
        ));
    }
    if points.len() >= 2 {
        ui.painter().add(egui::Shape::line(
            points,
            eframe::egui::Stroke::new(RING_STROKE, ring_color),
        ));
    }

    // Заголовок.
    let catalog = i18n::catalog(inputs.language);
    let title = catalog.settings.sleep_warning_title;
    let title_galley = ui.painter().layout_no_wrap(
        title.to_owned(),
        egui::FontId::proportional(16.0),
        palette.text,
    );
    ui.painter().galley(
        egui::pos2(center.x - title_galley.size().x / 2.0, rect.top() + 24.0),
        title_galley,
        palette.text,
    );

    // Оставшееся время.
    let time_text = i18n::sleep_remaining_label(inputs.language, remaining);
    let time_galley =
        ui.painter()
            .layout_no_wrap(time_text, egui::FontId::monospace(32.0), ring_color);
    ui.painter().galley(
        egui::pos2(center.x - time_galley.size().x / 2.0, center.y - 16.0),
        time_galley,
        ring_color,
    );

    // Строка действия.
    let action_text = match inputs.action {
        SleepAction::Nothing => catalog.settings.sleep_warning_stop,
        SleepAction::Suspend => catalog.settings.sleep_warning_suspend,
        SleepAction::PowerOff => catalog.settings.sleep_warning_power_off,
    };
    let action_galley = ui.painter().layout_no_wrap(
        action_text.to_owned(),
        egui::FontId::proportional(13.0),
        palette.muted,
    );
    ui.painter().galley(
        egui::pos2(center.x - action_galley.size().x / 2.0, center.y + 34.0),
        action_galley,
        palette.muted,
    );

    // Кнопка «Отменить».
    let cancel_label = catalog.settings.sleep_warning_cancel;
    let cancel_galley = ui.painter().layout_no_wrap(
        cancel_label.to_owned(),
        egui::FontId::proportional(14.0),
        palette.text,
    );
    let cancel_size = egui::Vec2::new(cancel_galley.size().x + 24.0, 32.0);
    let cancel_rect = egui::Rect::from_min_size(
        egui::pos2(center.x - cancel_size.x / 2.0, rect.bottom() - 46.0),
        cancel_size,
    );
    let cancel_response = ui.interact(
        cancel_rect,
        ui.id().with("sleep_warning_cancel"),
        egui::Sense::click(),
    );
    if cancel_response.clicked() {
        actions.push(WarningAction::Cancel);
    }
    if ui.is_rect_visible(cancel_rect) {
        ui.painter()
            .rect_filled(cancel_rect, 8.0, palette.chip_fill);
        ui.painter().rect_stroke(
            cancel_rect,
            8.0,
            eframe::egui::Stroke::new(1.0, palette.chip_stroke),
            eframe::egui::StrokeKind::Middle,
        );
        ui.painter().galley(
            egui::pos2(
                cancel_rect.center().x - cancel_galley.size().x / 2.0,
                cancel_rect.center().y - cancel_galley.size().y / 2.0,
            ),
            cancel_galley,
            palette.text,
        );
    }

    // close_requested (Esc / WM close) трактуется как Cancel.
    if inputs.close_requested {
        actions.push(WarningAction::Cancel);
    }

    // Подсказка под кнопкой.
    let hint = catalog.settings.sleep_warning_cancel_hint;
    let hint_galley = ui.painter().layout_no_wrap(
        hint.to_owned(),
        egui::FontId::proportional(11.0),
        palette.muted,
    );
    ui.painter().galley(
        egui::pos2(center.x - hint_galley.size().x / 2.0, rect.bottom() - 18.0),
        hint_galley,
        palette.muted,
    );

    // Fallback для бэкендов без мультивьюпортности: рисуем embedded.
    if class == egui::ViewportClass::EmbeddedWindow {
        tracing::warn!(
            "бэкенд не поддерживает несколько вьюпортов, предупреждение рисуется embedded"
        );
    }

    actions
}

/// Оставшееся время, clamped к 60 с (предупреждение показывается только
/// в последнюю минуту).
fn remaining_secs_clamped(secs: u64) -> u64 {
    secs.min(60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warning_color_ramp_yellow_to_green_after_half() {
        assert_eq!(
            warning_color(1.0),
            egui::Color32::from_rgb(0x34, 0xD3, 0x99),
        );
        assert_eq!(
            warning_color(0.5),
            egui::Color32::from_rgb(0xFB, 0xBF, 0x24),
        );
        let mid = warning_color(0.75);
        // Смесь жёлтого и зелёного.
        assert!(mid.g() > 0xBF);
        assert!(mid.r() < 0xFB);
    }

    #[test]
    fn warning_color_ramp_red_to_yellow_before_half() {
        assert_eq!(
            warning_color(0.0),
            egui::Color32::from_rgb(0xEF, 0x44, 0x44),
        );
        assert_eq!(
            warning_color(0.5),
            egui::Color32::from_rgb(0xFB, 0xBF, 0x24),
        );
        let mid = warning_color(0.25);
        // Смесь красного и жёлтого.
        assert!(mid.r() > 0xEF);
        assert!(mid.g() > 0x44);
    }

    #[test]
    fn warning_color_clamps() {
        assert_eq!(warning_color(-1.0), warning_color(0.0));
        assert_eq!(warning_color(2.0), warning_color(1.0));
        assert_eq!(warning_color(f32::NAN), warning_color(0.0));
    }

    #[test]
    fn remaining_secs_clamped_to_sixty() {
        assert_eq!(remaining_secs_clamped(30), 30);
        assert_eq!(remaining_secs_clamped(60), 60);
        assert_eq!(remaining_secs_clamped(120), 60);
        assert_eq!(remaining_secs_clamped(0), 0);
    }
}
