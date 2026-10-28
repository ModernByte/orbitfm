//! Примитивы строк модального окна настроек: dropdown, тумблер, свотч и
//! строка-ряд (ADR-0031).
//!
//! Контролы рисуют себя сами и берут цвета из палитры, которую вызывающий код уже
//! погасил stagger'ом входа. Внутри контролов запрещены `multiply_opacity` и
//! условная вложенность `Ui`: по ADR-0032 §7 анимация не имеет права двигать
//! аллокации, идентификаторы виджетов и hit-зоны — поэтому геометрия строки не
//! зависит от `on`, а переходы остаются только в отрисовке.

use eframe::egui;

use super::palette::{Palette, mix_colors};
use super::widgets::{contrast_color, control_motion, faded, focus_ring};

/// Высота строки с контролом: тумблер выше подписи, поэтому задаёт ритм.
const ROW_HEIGHT: f32 = 30.0;
/// Подпись строки — 12.5 pt, приглушённая подсказка под ней — 10.5 pt.
const LABEL_SIZE: f32 = 12.5;
const HINT_SIZE: f32 = 10.5;
/// Тумблер: трек 40×22, ручка 16 px, отступ ручки от края трека.
const TRACK_SIZE: egui::Vec2 = egui::Vec2::new(40.0, 22.0);
const KNOB_RADIUS: f32 = 8.0;
const KNOB_INSET: f32 = 3.0;
/// Свотч акцента: кружок 17 px в слоте 26 px и кольцо выбора вокруг него.
const SWATCH_SIZE: f32 = 26.0;
const SWATCH_RADIUS: f32 = 8.5;
const SWATCH_RING: f32 = 12.0;
/// Насколько гаснут цвета выключенного контрола: своего `disabled` у нашей
/// палитры нет, поэтому приглушаем явно.
const DISABLED: f32 = 0.45;

/// Выпадающий список вместо набора кнопок: косозначенный чип с шевроном.
///
/// `label_of` даёт подпись варианта по индексу `options`; возвращается индекс
/// выбора, если он отличается от `selected`. Явный `id_salt` держит идентификатор
/// списка неподвижным независимо от того, сколько контролов в строке.
pub(crate) fn dropdown<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    palette: &Palette,
    id_salt: &str,
    width: f32,
    options: &[T],
    selected: T,
    label_of: impl Fn(usize) -> String,
) -> Option<usize> {
    let current = options
        .iter()
        .position(|option| *option == selected)
        .unwrap_or_default();
    let mut picked = current;
    // Стиль чипа правим на текущем Ui (вложенный сдвинул бы auto-иденты), а
    // потом возвращаем прежний: попап строится из `ctx.style()` и гаснуть не
    // должен — он открывается кликом, когда вход модалки уже завершён.
    let base = ui.style().clone();
    ui.spacing_mut().button_padding = egui::vec2(10.0, 6.0);
    chip_visuals(ui.visuals_mut(), palette);
    egui::ComboBox::from_id_salt(id_salt)
        .width(width)
        .truncate()
        .selected_text(
            egui::RichText::new(label_of(current))
                .size(LABEL_SIZE)
                .color(palette.text),
        )
        .icon({
            let edge = palette.chip_stroke;
            let accent = palette.accent;
            move |ui: &egui::Ui, rect: egui::Rect, _visuals, is_open: bool| {
                // Шеврон из двух лучей: разворачивается, когда список открыт.
                let stroke = egui::Stroke::new(1.4, if is_open { accent } else { edge });
                let center = rect.center();
                let (half, rise) = (3.4f32, 2.2f32);
                let (outer, inner) = if is_open {
                    (center.y + rise, center.y - rise)
                } else {
                    (center.y - rise, center.y + rise)
                };
                let painter = ui.painter();
                painter.add(egui::Shape::line(
                    vec![
                        egui::pos2(center.x - half, outer),
                        egui::pos2(center.x, inner),
                    ],
                    stroke,
                ));
                painter.add(egui::Shape::line(
                    vec![
                        egui::pos2(center.x, inner),
                        egui::pos2(center.x + half, outer),
                    ],
                    stroke,
                ));
            }
        })
        .show_ui(ui, |ui| {
            for index in 0..options.len() {
                ui.selectable_value(&mut picked, index, label_of(index));
            }
        });
    ui.set_style(base);
    (picked != current).then_some(picked)
}

/// Тумблер современного вида: подпись слева на всю ширину строки, трек справа.
/// Клик по любому месту строки переключает значение, `enabled` гасит цвета и
/// снимает `Sense::click`, не трогая размеры (ADR-0032 §7).
///
/// Возвращает `Response` строки: вызывающий смотрит `clicked()`, а `hint`
/// остаётся внутри hit-зоны, поэтому отдельный `on_hover_text` не нужен.
pub(crate) fn switch_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    hint: Option<&str>,
    on: bool,
    enabled: bool,
) -> egui::Response {
    let gap = ui.spacing().item_spacing.x;
    let width = ui.available_width();
    let text_width = (width - TRACK_SIZE.x - gap).max(48.0);
    let text = if enabled {
        palette.text
    } else {
        faded(palette.text, DISABLED)
    };
    let hint_color = if enabled {
        palette.muted
    } else {
        faded(palette.muted, DISABLED)
    };
    let label_galley = ui.painter().layout(
        label.to_owned(),
        egui::FontId::proportional(LABEL_SIZE),
        text,
        text_width,
    );
    let hint_galley = hint.map(|hint| {
        ui.painter().layout(
            hint.to_owned(),
            egui::FontId::proportional(HINT_SIZE),
            hint_color,
            text_width,
        )
    });
    let label_height = label_galley.size().y;
    let text_height = label_height
        + hint_galley
            .as_ref()
            .map_or(0.0, |galley| galley.size().y + 2.0);
    let height = ROW_HEIGHT.max(text_height);
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::Vec2::new(width, height), sense);
    let track = switch_track(rect);
    if ui.is_rect_visible(rect) {
        let (hover, press) = control_motion(
            ui,
            response.id,
            enabled && response.hovered(),
            response.is_pointer_button_down_on(),
        );
        if hover > 0.0 || press > 0.0 {
            ui.painter().rect_filled(
                rect,
                8.0,
                faded(palette.chip_fill, 0.7 * hover.max(0.6 * press)),
            );
        }
        let text_top = rect.center().y - text_height * 0.5;
        ui.painter()
            .galley(egui::pos2(rect.left(), text_top), label_galley, text);
        if let Some(galley) = hint_galley {
            ui.painter().galley(
                egui::pos2(rect.left(), text_top + label_height + 2.0),
                galley,
                hint_color,
            );
        }
        // Ручка едет по треку; при reduce motion egui сам отдаёт конечное
        // значение, потому что `animation_time` равен нулю (ADR-0032 §3).
        let travel = ui.ctx().animate_bool_with_time(
            response.id.with("orbitfm-switch"),
            on,
            ui.style().animation_time,
        );
        let knob_x = switch_knob_x(track, travel);
        let track_fill = if on {
            if enabled {
                mix_colors(palette.accent, egui::Color32::WHITE, 0.12 * hover)
            } else {
                faded(palette.accent, DISABLED)
            }
        } else {
            mix_colors(palette.chip_fill, palette.accent, 0.10 * hover)
        };
        let track_stroke = if on {
            egui::Stroke::NONE
        } else if enabled {
            egui::Stroke::new(
                1.0,
                mix_colors(palette.chip_stroke, palette.accent, 0.7 * hover.max(press)),
            )
        } else {
            egui::Stroke::new(1.0, faded(palette.chip_stroke, DISABLED))
        };
        ui.painter().rect(
            track,
            TRACK_SIZE.y * 0.5,
            track_fill,
            track_stroke,
            egui::StrokeKind::Inside,
        );
        let knob = if on {
            contrast_color(palette.accent)
        } else if enabled {
            palette.text
        } else {
            faded(palette.text, DISABLED)
        };
        ui.painter()
            .circle_filled(egui::pos2(knob_x, track.center().y), KNOB_RADIUS, knob);
    }
    focus_ring(ui, &response, track, TRACK_SIZE.y * 0.5);
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, enabled, on, label));
    response
}

/// Круглый свотч палитры акцентов: выбранный обведён кольцом и галочкой.
/// В отличие от строки, подпись приходит снаружи — свотчи стоят в ряд.
pub(crate) fn accent_swatch(
    ui: &mut egui::Ui,
    palette: &Palette,
    color: egui::Color32,
    selected: bool,
    hint: &str,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::Vec2::splat(SWATCH_SIZE), egui::Sense::click());
    let center = rect.center();
    if ui.is_rect_visible(rect) {
        let (hover, press) = control_motion(
            ui,
            response.id,
            response.hovered(),
            response.is_pointer_button_down_on(),
        );
        if selected || hover > 0.0 || press > 0.0 {
            ui.painter().circle_stroke(
                center,
                SWATCH_RING,
                egui::Stroke::new(
                    if selected { 1.8 } else { 1.2 },
                    if selected {
                        palette.text
                    } else {
                        mix_colors(palette.chip_stroke, palette.text, hover)
                    },
                ),
            );
        }
        ui.painter().circle_filled(center, SWATCH_RADIUS, color);
        if selected {
            // Галочка двумя штрихами: цвет контрастный к кружку, как у текста
            // на выбранном чипе. SVG-ассета под неё нет и ADR-0034 не трогается.
            let check = egui::Stroke::new(1.8, contrast_color(color));
            let point = |dx: f32, dy: f32| center + egui::vec2(dx, dy);
            ui.painter().add(egui::Shape::line(
                vec![point(-3.4, 0.2), point(-0.8, 2.8)],
                check,
            ));
            ui.painter().add(egui::Shape::line(
                vec![point(-0.8, 2.8), point(3.8, -2.6)],
                check,
            ));
        }
    }
    focus_ring(ui, &response, rect, SWATCH_SIZE * 0.5);
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, true, selected, hint));
    response.on_hover_text(hint)
}

/// Строка настройки: подпись слева (перенос по ширине слота), контрол в
/// фиксированном слоте справа, приглушённая подсказка под подписью.
///
/// `slot` — (ширина, высота) места под контрол: 220×30 для dropdown, 0 для
/// контрола, который приносит подпись сам (тумблер). Возвращает то, что вернул
/// контрол.
pub(crate) fn row<R>(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    hint: Option<&str>,
    slot: (f32, f32),
    control: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let gap = ui.spacing().item_spacing.x;
    let width = ui.available_width();
    let label_width = (width - slot.0 - gap).max(48.0);
    let galley = ui.painter().layout(
        label.to_owned(),
        egui::FontId::proportional(LABEL_SIZE),
        palette.text,
        label_width,
    );
    let height = ROW_HEIGHT.max(galley.size().y);
    let rect = egui::Rect::from_min_size(
        ui.available_rect_before_wrap().min,
        egui::Vec2::new(width, height),
    );
    ui.allocate_rect(rect, egui::Sense::hover());
    ui.painter().galley(
        egui::pos2(rect.left(), rect.center().y - galley.size().y * 0.5),
        galley,
        palette.text,
    );
    let outcome = ui
        .scope_builder(
            // Вложенный Ui здесь безусловен: идентификаторы контролов не съезжают,
            // а `centered_and_justified` ставит контрол в середину слота, чтобы
            // низкий слайдер не прилипал к верхнему краю строки.
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_center_size(
                    egui::pos2(rect.right() - slot.0 * 0.5, rect.center().y),
                    egui::Vec2::from(slot),
                ))
                .layout(egui::Layout::centered_and_justified(
                    egui::Direction::TopDown,
                )),
            |ui| control(ui),
        )
        .inner;
    if let Some(hint) = hint {
        ui.label(
            egui::RichText::new(hint)
                .size(HINT_SIZE)
                .color(palette.muted)
                .weak(),
        );
    }
    outcome
}

/// Место трека тумблера в строке: всегда у правого края, независимо от
/// состояния и подсказки.
fn switch_track(row: egui::Rect) -> egui::Rect {
    egui::Rect::from_center_size(
        egui::pos2(row.right() - TRACK_SIZE.x * 0.5, row.center().y),
        TRACK_SIZE,
    )
}

/// Позиция центра ручки по прогрессу `0..1`: крайние точки внутри трека,
/// поэтому ручка не вылезает за скругление.
fn switch_knob_x(track: egui::Rect, progress: f32) -> f32 {
    let progress = if progress.is_finite() {
        progress.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let left = track.left() + KNOB_INSET + KNOB_RADIUS;
    let right = track.right() - KNOB_INSET - KNOB_RADIUS;
    left + (right - left) * progress
}

/// Краска чипа для нативного `ComboBox`: фон и обводка нашей палитры, accent на
/// hover и на открытом списке.
fn chip_visuals(visuals: &mut egui::Visuals, palette: &Palette) {
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.bg_fill = palette.chip_fill;
        widget.weak_bg_fill = palette.chip_fill;
        widget.fg_stroke.color = palette.text;
        widget.bg_stroke = egui::Stroke::new(1.0, palette.chip_stroke);
        widget.corner_radius = egui::CornerRadius::same(9);
    }
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, palette.accent);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, palette.accent);
    visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0, palette.accent);
    // Выделение варианта в попапе остаётся акцентным — его рисует `selection`.
    visuals.widgets.noninteractive.fg_stroke.color = palette.text;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::AccentColor;

    /// Кадр содержимого модалки: 512 px — правая колонка 760 минус навигация.
    fn palette() -> Palette {
        Palette::for_ui(egui::Theme::Dark, AccentColor::Blue)
    }

    fn input(time: f64) -> egui::RawInput {
        egui::RawInput {
            time: Some(time),
            predicted_dt: 1.0 / 60.0,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(512.0, 420.0),
            )),
            ..Default::default()
        }
    }

    #[test]
    fn switch_state_does_not_move_the_row_or_the_track() {
        // ADR-0032 §7: состояние гасит только отрисовку, hit-зона остаётся.
        let mut seen = Vec::new();
        for (time, on) in [(0.1, false), (0.2, true)] {
            let ctx = egui::Context::default();
            let mut frame = (egui::Rect::NOTHING, egui::Rect::NOTHING);
            let output = ctx.run_ui(input(time), |ui| {
                let response = switch_row(ui, &palette(), "Визуализатор", None, on, true);
                frame = (response.rect, switch_track(response.rect));
            });
            output.drop_without_applying_deltas();
            seen.push(frame);
        }
        assert_eq!(seen[0].0, seen[1].0, "тумблер не двигает строку");
        assert_eq!(seen[0].1, seen[1].1, "трек стоит на месте");
        assert!(seen[0].1.right() <= seen[0].0.right() + 0.01, "трек у края");
        assert!(seen[0].0.width() > 40.0, "подпись входит в hit-зону");
    }

    #[test]
    fn disabled_switch_keeps_its_geometry() {
        let mut seen = Vec::new();
        for (time, enabled) in [(0.1, true), (0.2, false)] {
            let ctx = egui::Context::default();
            let mut rect = egui::Rect::NOTHING;
            let output = ctx.run_ui(input(time), |ui| {
                rect = switch_row(
                    ui,
                    &palette(),
                    "Перевороты",
                    Some("Подсказка"),
                    true,
                    enabled,
                )
                .rect;
            });
            output.drop_without_applying_deltas();
            seen.push(rect);
        }
        assert_eq!(seen[0], seen[1], "disabled не меняет размеры и позицию");
    }

    #[test]
    fn knob_travels_between_the_track_edges() {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 30.0));
        let track = switch_track(rect);
        let off = switch_knob_x(track, 0.0);
        let on = switch_knob_x(track, 1.0);
        assert!(off < on, "ручка едет слева направо");
        assert!(
            (off - track.left() - KNOB_INSET - KNOB_RADIUS).abs() < 0.01,
            "в покое ручка у левого края"
        );
        assert!(
            (on - track.right() + KNOB_INSET + KNOB_RADIUS).abs() < 0.01,
            "включённая ручка у правого края"
        );
        assert_eq!(switch_knob_x(track, f32::NAN), off);
        assert_eq!(switch_knob_x(track, 7.0), on);
    }

    #[test]
    fn long_label_wraps_instead_of_pushing_the_control() {
        let slot = (220.0, 30.0);
        let mut seen = Vec::new();
        for (time, label) in [
            (0.1, "Тема"),
            (
                0.2,
                "Darstellung der Kamerabewegung bei weichen Übergängen im Fokus, in der Übersicht und in der Kapsel",
            ),
        ] {
            let ctx = egui::Context::default();
            let mut control = egui::Rect::NOTHING;
            let mut after = 0.0f32;
            let output = ctx.run_ui(input(time), |ui| {
                control = row(ui, &palette(), label, None, slot, |ui| {
                    ui.allocate_exact_size(egui::Vec2::from(slot), egui::Sense::hover())
                        .0
                });
                after = ui.cursor().min.y;
            });
            output.drop_without_applying_deltas();
            seen.push((control, after));
        }
        let (short, long) = (seen[0], seen[1]);
        assert_eq!(short.0.width(), slot.0, "слот контрола фиксирован");
        assert!(
            (short.0.right() - long.0.right()).abs() < 0.01,
            "длинная подпись переносится, а не сдвигает контрол: {:?} vs {:?}",
            short.0,
            long.0
        );
        assert!(long.1 > short.1, "перенос сделал строку выше");
    }
}
