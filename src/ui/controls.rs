//! Примитивы строк drawer'а настроек: dropdown, чекбокс и строка-ряд (ADR-0031).
//!
//! Контролы рисуют себя сами и берут цвета из палитры, которую вызывающий код уже
//! погасил stagger'ом входа. Внутри контролов запрещены `multiply_opacity` и
//! условная вложенность `Ui`: по ADR-0032 §7 анимация не имеет права двигать
//! аллокации, идентификаторы виджетов и hit-зоны — поэтому геометрия строки не
//! зависит от `checked`, а переходы остаются только в отрисовке.

use eframe::egui;

use super::palette::{Palette, mix_colors};
use super::widgets::{contrast_color, control_motion, faded, focus_ring};

/// Высота строки с контролом: чип dropdown выше подписи, поэтому задаёт ритм.
const ROW_HEIGHT: f32 = 26.0;
/// Квадрат чекбокса и его скругление.
const CHECK_SIZE: f32 = 18.0;
const CHECK_RADIUS: f32 = 5.0;
/// Подпись строки — 12 pt, приглушённая подсказка под ней — 10 pt.
const LABEL_SIZE: f32 = 12.0;
const HINT_SIZE: f32 = 10.0;
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
    // Стиль чипа правим на текущем Ui (вложенный сдвину бы auto-иденты), а потом
    // возвращаем прежний: попап строится из `ctx.style()` и гаснуть не должен —
    // он открывается кликом, когда вход drawer'а уже завершён.
    let base = ui.style().clone();
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

/// Чекбокс строки настройки: квадрат с галочкой из двух штрихов и hit-зоной
/// вместе с подписью. `enabled` гасит цвета и снимает `Sense::click`, не трогая
/// размеры строки.
pub(crate) fn checkbox(
    ui: &mut egui::Ui,
    palette: &Palette,
    checked: bool,
    enabled: bool,
    label: &str,
) -> egui::Response {
    let gap = ui.spacing().item_spacing.x;
    let text = if enabled {
        palette.text
    } else {
        faded(palette.text, DISABLED)
    };
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(LABEL_SIZE),
        text,
    );
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(
        egui::Vec2::new(
            CHECK_SIZE + gap + galley.size().x,
            ROW_HEIGHT.max(galley.size().y),
        ),
        sense,
    );
    let box_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + CHECK_SIZE * 0.5, rect.center().y),
        egui::Vec2::splat(CHECK_SIZE),
    );
    if ui.is_rect_visible(rect) {
        let (hover, press) = control_motion(
            ui,
            response.id,
            enabled && response.hovered(),
            response.is_pointer_button_down_on(),
        );
        let edge = if enabled {
            mix_colors(palette.chip_stroke, palette.accent, hover.max(press))
        } else {
            faded(palette.chip_stroke, DISABLED)
        };
        let fill = if checked && enabled {
            palette.accent
        } else {
            palette.chip_fill
        };
        ui.painter().rect_filled(box_rect, CHECK_RADIUS, fill);
        ui.painter().rect_stroke(
            box_rect,
            CHECK_RADIUS,
            egui::Stroke::new(1.0, edge),
            egui::StrokeKind::Inside,
        );
        if checked {
            // Галочка двумя штрихами: SVG-ассета под неё нет, и ADR-0034 не
            // трогается — цвет контрастный к accent, как текст на выбранном чипе.
            let check = egui::Stroke::new(2.0, contrast_color(palette.accent));
            let left = box_rect.left_top() + box_rect.size() * egui::Vec2::new(0.24, 0.52);
            let bottom = box_rect.left_top() + box_rect.size() * egui::Vec2::new(0.44, 0.72);
            let right = box_rect.left_top() + box_rect.size() * egui::Vec2::new(0.78, 0.30);
            ui.painter()
                .add(egui::Shape::line(vec![left, bottom], check));
            ui.painter()
                .add(egui::Shape::line(vec![bottom, right], check));
        }
        ui.painter().galley(
            egui::pos2(
                box_rect.right() + gap,
                rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            text,
        );
    }
    focus_ring(ui, &response, box_rect, CHECK_RADIUS);
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, enabled, checked, label));
    response
}

/// Строка настройки: подпись слева (перенос по ширине слота), контрол в
/// фиксированном слоте справа, приглушённая подсказка под подписью.
///
/// `slot` — (ширина, высота) места под контрол: 168×26 для dropdown, 150×18 для
/// слайдера, 0 для контрола, который приносит подпись сам (чекбокс). Возвращает
/// то, что вернул контрол.
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
        widget.corner_radius = egui::CornerRadius::same(8);
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

    /// Кадр drawer'а: 352 px — содержимое drawer'а 380 px за вычетом полей.
    fn palette() -> Palette {
        Palette::for_ui(egui::Theme::Dark, AccentColor::Blue)
    }

    fn input(time: f64) -> egui::RawInput {
        egui::RawInput {
            time: Some(time),
            predicted_dt: 1.0 / 60.0,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(352.0, 400.0),
            )),
            ..Default::default()
        }
    }

    #[test]
    fn checked_state_does_not_move_the_checkbox() {
        // ADR-0032 §7: stagger и состояние гасят только отрисовку, hit-зона
        // остаётся на месте.
        let mut boxes = Vec::new();
        for (time, checked) in [(0.1, false), (0.2, true)] {
            let ctx = egui::Context::default();
            let mut rect = egui::Rect::NOTHING;
            let output = ctx.run_ui(input(time), |ui| {
                rect = checkbox(ui, &palette(), checked, true, "Визуализатор").rect;
            });
            output.drop_without_applying_deltas();
            boxes.push(rect);
        }
        assert_eq!(boxes[0], boxes[1], "галочка не двигает квадрат и подпись");
        assert!(boxes[0].width() > 18.0, "подпись входит в hit-зону");
    }

    #[test]
    fn long_label_wraps_instead_of_pushing_the_control() {
        let slot = (168.0, 26.0);
        let mut seen = Vec::new();
        for (time, label) in [(0.1, "Тема"), (0.2, "Darstellung der Kamerabewegung")] {
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
