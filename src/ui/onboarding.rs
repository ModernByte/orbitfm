//! Мастер первого запуска (ADR-0041): язык и тема, окружение, интеграции.
//!
//! Модалка рисуется поверх стола и не входит в [`crate::ui::layout`]: пока она
//! открыта, сцена не принимает ввод. Текст берётся из каталога разрешённого
//! языка; при арабском компоновка шага разворачивается справа налево.

use eframe::egui;

use crate::i18n;
use crate::integration::wm::{self, Desktop, RuntimeRules};
use crate::settings::{Language, Settings};

use super::UiAction;
use super::controls::{dropdown, row};
use super::layout::MARGIN;
use super::palette::{Palette, mix_colors};
use super::widgets::{action_pill, control_motion, faded, focus_ring, glass_panel};

const MODAL: (f32, f32) = (560.0, 468.0);
const RADIUS: f32 = 20.0;
const SCRIM_ALPHA: u8 = 100;
const DROPDOWN_SLOT: (f32, f32) = (220.0, 30.0);
const CODE_HEIGHT: f32 = 116.0;
const STEPS: usize = 3;
/// Высота заголовка мастера: имя, подзаголовок и отступ до первого шага.
const HEADER_HEIGHT: f32 = 52.0;

/// Результат кнопки «Настроить окно» на текущем открытии мастера.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum RuleState {
    #[default]
    Idle,
    Applied,
    Failed,
}

/// Представленческое состояние мастера: шаг и отклик на действия.
#[derive(Debug, Default)]
pub(crate) struct View {
    pub(crate) step: usize,
    pub(crate) rule: RuleState,
    pub(crate) copied: bool,
}

pub(crate) struct State<'a> {
    pub view: &'a mut View,
    pub language: Language,
    pub settings: Settings,
    pub desktop: Desktop,
}

/// Скрим и модальное окно мастера поверх стола.
pub(crate) fn show(
    ui: &mut egui::Ui,
    viewport: egui::Rect,
    palette: &Palette,
    mut state: State<'_>,
) -> Vec<UiAction> {
    let text = i18n::catalog(state.language);
    let welcome = &text.welcome;
    let mut actions = Vec::new();

    egui::Area::new(egui::Id::unique("orbitfm_onboarding_scrim"))
        .fixed_pos(viewport.min)
        .default_size(viewport.size())
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            let (rect, response) = ui.allocate_exact_size(viewport.size(), egui::Sense::click());
            let dim = if matches!(ui.ctx().theme(), egui::Theme::Dark) {
                egui::Color32::from_black_alpha(SCRIM_ALPHA)
            } else {
                egui::Color32::from_white_alpha(SCRIM_ALPHA)
            };
            ui.painter().rect_filled(rect, 0.0, dim);
            let _ = response;
        });

    let size = egui::vec2(
        MODAL.0.min((viewport.width() - 2.0 * MARGIN).max(1.0)),
        MODAL.1.min((viewport.height() - 2.0 * MARGIN).max(1.0)),
    );
    let rect = egui::Rect::from_center_size(viewport.center(), size);
    let rtl = state.language == Language::Arabic;
    let step = state.view.step.min(STEPS - 1);

    egui::Area::new(egui::Id::unique("orbitfm_onboarding_modal"))
        .fixed_pos(rect.min)
        .default_size(rect.size())
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            for (spread, alpha) in [(20.0, 0.10), (12.0, 0.12), (5.0, 0.16)] {
                ui.painter().rect_filled(
                    rect.expand(spread),
                    RADIUS + spread,
                    faded(egui::Color32::BLACK, alpha),
                );
            }
            glass_panel(ui.painter(), rect, palette, RADIUS, 1.0);
            ui.set_min_size(rect.size());
            ui.set_max_size(rect.size());
            ui.interact(
                rect,
                ui.scope_id().with("orbitfm_onboarding_body"),
                egui::Sense::click(),
            );

            let inner = rect.shrink2(egui::vec2(22.0, 18.0));
            let footer = egui::Rect::from_min_max(
                egui::pos2(inner.left(), inner.bottom() - 34.0),
                inner.right_bottom(),
            );
            let header = inner.with_max_y(inner.top() + HEADER_HEIGHT);
            let content = egui::Rect::from_min_max(
                egui::pos2(inner.left(), header.bottom() + 8.0),
                egui::pos2(inner.right(), footer.top() - 10.0),
            );

            ui.scope_builder(
                egui::UiBuilder::new().max_rect(header).layout(stack(rtl)),
                |ui| {
                    ui.label(
                        egui::RichText::new(welcome.title)
                            .size(19.0)
                            .strong()
                            .color(palette.text),
                    );
                    ui.add_space(3.0);
                    ui.label(
                        egui::RichText::new(welcome.subtitle)
                            .size(11.5)
                            .color(palette.muted),
                    );
                },
            );
            step_dots(
                ui,
                egui::pos2(inner.right() - 44.0, inner.top() + 12.0),
                palette,
                step,
            );

            ui.scope_builder(
                egui::UiBuilder::new().max_rect(content).layout(stack(rtl)),
                |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| match step {
                            0 => language_step(ui, palette, &state, &mut actions),
                            1 => environment_step(ui, palette, &mut state, &mut actions),
                            _ => integrations_step(ui, palette, welcome, &mut actions),
                        });
                },
            );

            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(footer)
                    .layout(direction(rtl)),
                |ui| {
                    footer_bar(ui, palette, welcome, state.view, step, rtl, &mut actions);
                },
            );
        });

    actions
}

/// Шаг 1: язык и тема применяются сразу, без перезапуска (ADR-0030).
fn language_step(
    ui: &mut egui::Ui,
    palette: &Palette,
    state: &State<'_>,
    actions: &mut Vec<UiAction>,
) {
    let text = i18n::catalog(state.language);
    let welcome = &text.welcome;
    let strings = &text.settings;
    ui.label(
        egui::RichText::new(welcome.language_theme)
            .size(13.0)
            .strong()
            .color(palette.text),
    );
    ui.add_space(8.0);
    if let Some(index) = row(ui, palette, strings.language, None, DROPDOWN_SLOT, |ui| {
        dropdown(
            ui,
            palette,
            "orbitfm_onboarding_language",
            ui.available_width(),
            &Language::ALL,
            state.settings.language,
            |index| i18n::language_label(state.language, Language::ALL[index]).to_owned(),
        )
    }) {
        actions.push(UiAction::SetLanguage(Language::ALL[index]));
    }
    ui.add_space(6.0);
    if let Some(index) = row(ui, palette, strings.theme, None, DROPDOWN_SLOT, |ui| {
        dropdown(
            ui,
            palette,
            "orbitfm_onboarding_theme",
            ui.available_width(),
            &crate::settings::ThemeMode::ALL,
            state.settings.theme,
            |index| {
                i18n::theme_label(state.language, crate::settings::ThemeMode::ALL[index]).to_owned()
            },
        )
    }) {
        actions.push(UiAction::SetTheme(crate::settings::ThemeMode::ALL[index]));
    }
}

/// Шаг 2: обнаружение окружения и правила окна без правки чужих конфигов.
fn environment_step(
    ui: &mut egui::Ui,
    palette: &Palette,
    state: &mut State<'_>,
    actions: &mut Vec<UiAction>,
) {
    let text = i18n::catalog(state.language);
    let welcome = &text.welcome;
    let desktop = state.desktop;
    ui.label(
        egui::RichText::new(welcome.environment)
            .size(13.0)
            .strong()
            .color(palette.text),
    );
    ui.add_space(2.0);
    ui.label(
        egui::RichText::new(i18n::fill(welcome.detected, &[("name", desktop.name())]))
            .size(14.0)
            .strong()
            .color(palette.accent),
    );
    ui.add_space(5.0);
    ui.add(
        egui::Label::new(
            egui::RichText::new(welcome.environment_hint)
                .size(11.5)
                .color(palette.muted),
        )
        .wrap(),
    );
    ui.add_space(10.0);

    match desktop.runtime_rules() {
        RuntimeRules::Hyprland => {
            if action_pill(ui, palette, welcome.configure).clicked() {
                actions.push(UiAction::ApplyWindowRules);
            }
            ui.add_space(8.0);
            match state.view.rule {
                RuleState::Idle => {}
                RuleState::Applied => {
                    ui.label(
                        egui::RichText::new(welcome.applied)
                            .size(11.5)
                            .color(palette.live),
                    );
                }
                RuleState::Failed => {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(welcome.failed)
                                .size(11.5)
                                .color(palette.error),
                        )
                        .wrap(),
                    );
                    ui.add_space(6.0);
                    if snippet(ui, palette, wm::HYPRLAND_SNIPPET, welcome.copy) {
                        ui.ctx().copy_text(wm::HYPRLAND_SNIPPET.to_owned());
                        state.view.copied = true;
                    }
                }
            }
        }
        RuntimeRules::Snippet(snippet_text) => {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(welcome.niri_hint)
                        .size(11.5)
                        .color(palette.muted),
                )
                .wrap(),
            );
            ui.add_space(6.0);
            if snippet(ui, palette, snippet_text, welcome.copy) {
                ui.ctx().copy_text(snippet_text.to_owned());
                state.view.copied = true;
            }
        }
        RuntimeRules::Unsupported => {}
    }
    if state.view.copied {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(welcome.copied)
                .size(11.0)
                .color(palette.live),
        );
    }
}

/// Шаг 3: путь к интеграциям; отказ не блокирует выход из мастера.
fn integrations_step(
    ui: &mut egui::Ui,
    palette: &Palette,
    welcome: &i18n::WelcomeText,
    actions: &mut Vec<UiAction>,
) {
    ui.label(
        egui::RichText::new(welcome.integrations_hint)
            .size(12.0)
            .color(palette.muted),
    );
    ui.add_space(12.0);
    if action_pill(ui, palette, welcome.open_integrations).clicked() {
        actions.push(UiAction::OpenIntegrations);
    }
}

/// Прокручиваемый статический текст правила и кнопка «Копировать».
fn snippet(ui: &mut egui::Ui, palette: &Palette, text: &'static str, copy: &str) -> bool {
    egui::Frame::new()
        .fill(faded(palette.chip_fill, 0.7))
        .stroke(egui::Stroke::new(1.0, palette.chip_stroke))
        .corner_radius(10)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(CODE_HEIGHT)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(text)
                            .monospace()
                            .size(9.5)
                            .color(palette.text),
                    );
                });
        });
    ui.add_space(6.0);
    action_pill(ui, palette, copy).clicked()
}

fn footer_bar(
    ui: &mut egui::Ui,
    palette: &Palette,
    welcome: &i18n::WelcomeText,
    state: &mut View,
    step: usize,
    rtl: bool,
    actions: &mut Vec<UiAction>,
) {
    if step > 0 && ghost_button(ui, palette, welcome.back).clicked() {
        state.step = step - 1;
        state.copied = false;
    }
    if ghost_button(ui, palette, welcome.skip).clicked() {
        actions.push(UiAction::FinishOnboarding);
    }
    let primary_layout = if rtl {
        egui::Layout::left_to_right(egui::Align::Center)
    } else {
        egui::Layout::right_to_left(egui::Align::Center)
    };
    ui.with_layout(primary_layout, |ui| {
        let label = if step + 1 >= STEPS {
            welcome.finish
        } else {
            welcome.next
        };
        if action_pill(ui, palette, label).clicked() {
            if step + 1 >= STEPS {
                actions.push(UiAction::FinishOnboarding);
            } else {
                state.step = step + 1;
                state.copied = false;
            }
        }
    });
}

/// Точки шагов: заполнены до текущего включительно.
fn step_dots(ui: &mut egui::Ui, center: egui::Pos2, palette: &Palette, step: usize) {
    let spacing = 12.0;
    let start = center.x - spacing * (STEPS - 1) as f32 * 0.5;
    for index in 0..STEPS {
        let filled = index <= step;
        let point = egui::pos2(start + spacing * index as f32, center.y);
        ui.painter().circle_filled(
            point,
            3.0,
            if filled {
                palette.accent
            } else {
                mix_colors(palette.muted, palette.chip_fill, 0.6)
            },
        );
    }
}

/// Малошумная кнопка фиксированной ширины: подложка только на hover.
fn ghost_button(ui: &mut egui::Ui, palette: &Palette, label: &str) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(12.0),
        palette.muted,
    );
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(galley.size().x + 22.0, 30.0),
        egui::Sense::click(),
    );
    let (hover, _) = control_motion(ui, response.id, response.hovered(), false);
    if ui.is_rect_visible(rect) && hover > 0.0 {
        ui.painter()
            .rect_filled(rect, 8.0, faded(palette.chip_fill, hover));
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(12.0),
        mix_colors(palette.muted, palette.text, hover),
    );
    focus_ring(ui, &response, rect, 8.0);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    response
}

/// Горизонтальный ряд (футер): кнопки идут от края внутрь.
fn direction(rtl: bool) -> egui::Layout {
    if rtl {
        egui::Layout::right_to_left(egui::Align::Min)
    } else {
        egui::Layout::left_to_right(egui::Align::Min)
    }
}

/// Вертикальный поток шага; при арабском прижимается к правому краю.
fn stack(rtl: bool) -> egui::Layout {
    if rtl {
        egui::Layout::top_down(egui::Align::Max)
    } else {
        egui::Layout::top_down(egui::Align::Min)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_index_never_leaves_the_wizard() {
        let view = View {
            step: 99,
            ..Default::default()
        };
        assert_eq!(view.step.min(STEPS - 1), STEPS - 1);
    }

    #[test]
    fn arabic_switches_the_composition_direction() {
        assert_eq!(
            direction(true).main_dir,
            egui::Direction::RightToLeft,
            "арабский разворачивает шаг справа налево"
        );
        assert_eq!(direction(false).main_dir, egui::Direction::LeftToRight);
    }

    #[test]
    fn wizard_text_stacks_vertically_in_both_directions() {
        assert_eq!(stack(false).main_dir, egui::Direction::TopDown);
        assert_eq!(stack(false).cross_align, egui::Align::Min);
        assert_eq!(stack(true).main_dir, egui::Direction::TopDown);
        assert_eq!(stack(true).cross_align, egui::Align::Max);
    }
}
