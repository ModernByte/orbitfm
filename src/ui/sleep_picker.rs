//! Быстрый выбор таймера сна (ADR-0046): часы в шапке открывают список
//! длительностей. Модуль ничего не сохраняет — он возвращает [`UiAction`],
//! а дедлайн живёт в `Session`.

use eframe::egui;

use crate::i18n;
use crate::settings::{Language, SLEEP_CHOICES};

use super::UiAction;
use super::icons::sleep_button;
use super::palette::Palette;

const MENU_WIDTH: f32 = 168.0;

/// Кнопка часов с меню длительностей. `active` — выбранные минуты, `None` —
/// таймер выключен. Действие возвращается только при выборе пункта.
pub(crate) fn show(
    ui: &mut egui::Ui,
    palette: &Palette,
    active: Option<u16>,
    language: Language,
) -> Vec<UiAction> {
    let mut actions = Vec::new();
    let text = i18n::catalog(language);
    let hint = active.map_or_else(
        || text.settings.sleep_timer.to_owned(),
        |minutes| {
            format!(
                "{} · {}",
                text.settings.sleep_timer,
                i18n::sleep_option_label(language, minutes)
            )
        },
    );
    let response = sleep_button(ui, palette, &hint, active.is_some());
    let popup_id = egui::Popup::default_response_id(&response);
    let frame = egui::Frame::popup(ui.style())
        .fill(palette.card_fill)
        .stroke(egui::Stroke::new(1.0, palette.card_stroke))
        .inner_margin(egui::Margin::same(10));

    egui::Popup::menu(&response)
        .id(popup_id)
        .width(MENU_WIDTH)
        .frame(frame)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(MENU_WIDTH - 20.0);
            ui.visuals_mut().selection.bg_fill = palette.accent;
            ui.label(
                egui::RichText::new(text.settings.sleep_timer)
                    .size(11.0)
                    .strong()
                    .color(palette.muted),
            );
            ui.add_space(4.0);
            for minutes in SLEEP_CHOICES {
                let selected = active.map_or(minutes == 0, |active| minutes == active);
                let label = i18n::sleep_option_label(language, minutes);
                if ui.selectable_label(selected, label).clicked() {
                    actions.push(UiAction::SetSleepTimer((minutes > 0).then_some(minutes)));
                    egui::Popup::close_id(ui.ctx(), popup_id);
                }
            }
        });

    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_choice_has_a_label() {
        for language in Language::ALL {
            for minutes in SLEEP_CHOICES {
                assert!(
                    !i18n::sleep_option_label(language, minutes)
                        .trim()
                        .is_empty(),
                    "{language:?}: пустой вариант {minutes}"
                );
            }
        }
    }
}
