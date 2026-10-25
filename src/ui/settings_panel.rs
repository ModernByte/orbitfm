//! Drawer настроек: правая модальная панель со всеми секциями (ADR-0031).
//!
//! Модуль ничего не сохраняет сам — он возвращает [`UiAction`], а побочные
//! эффекты выполняет presentation-слой. Подписи берутся из каталога уже
//! разрешённого языка, поэтому смена языка видна в том же кадре (ADR-0030).

use eframe::egui;

use crate::i18n;
use crate::session::WindowMode;
use crate::settings::{
    AccentColor, Language, MotionReactivity, MotionStyle, Settings, ThemeMode, VIZ_BAR_CHOICES,
};

use super::UiAction;
use super::icons::close_button;
use super::palette::{Palette, accent_color};
use super::widgets::volume_slider_width;

/// Трек громкости: почти вся ширина drawer'а за вычетом полей.
const VOLUME_WIDTH: f32 = 230.0;
/// Размер кружка акцента в палитре.
const SWATCH_SIZE: egui::Vec2 = egui::Vec2::new(30.0, 22.0);

pub(crate) struct SettingsState<'a> {
    pub settings: Settings,
    /// Разрешённый язык интерфейса: `System` здесь уже стал конкретным.
    pub language: Language,
    pub window_mode: WindowMode,
    /// Копия громкости, которую двигает слайдер (commit-on-release, ADR-0031).
    pub volume: &'a mut f64,
    pub show_help: bool,
}

pub(crate) fn show(
    ui: &mut egui::Ui,
    palette: &Palette,
    state: SettingsState<'_>,
) -> Vec<UiAction> {
    let text = i18n::catalog(state.language);
    let strings = &text.settings;
    let labels = &text.labels;
    let mut actions = Vec::new();

    egui::Frame::default()
        .fill(egui::Color32::TRANSPARENT)
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(strings.title)
                        .size(15.0)
                        .strong()
                        .color(palette.text),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if close_button(ui, text.common.close).clicked() {
                        actions.push(UiAction::CloseSettings);
                    }
                });
            });
            ui.separator();

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    section(ui, palette, strings.appearance);
                    choice_row(ui, palette, strings.theme, &mut actions, |ui, actions| {
                        for theme in ThemeMode::ALL {
                            let selected = state.settings.theme == theme;
                            if selectable(ui, selected, i18n::theme_label(state.language, theme))
                                .clicked()
                                && !selected
                            {
                                actions.push(UiAction::SetTheme(theme));
                            }
                        }
                    });
                    choice_row(ui, palette, strings.accent, &mut actions, |ui, actions| {
                        for accent in AccentColor::ALL {
                            let selected = state.settings.accent == accent;
                            let stroke = if selected {
                                egui::Stroke::new(2.0, palette.text)
                            } else {
                                egui::Stroke::new(1.0, palette.chip_stroke)
                            };
                            let swatch = ui
                                .add(
                                    egui::Button::new("")
                                        .fill(accent_color(accent))
                                        .stroke(stroke)
                                        .min_size(SWATCH_SIZE)
                                        .corner_radius(6.0),
                                )
                                .on_hover_text(i18n::accent_label(state.language, accent));
                            if swatch.clicked() && !selected {
                                actions.push(UiAction::SetAccent(accent));
                            }
                        }
                    });
                    let visualizer = state.settings.visualizer_enabled;
                    if ui
                        .checkbox(&mut { visualizer }, text.transport.visualizer)
                        .clicked()
                    {
                        actions.push(UiAction::ToggleSpectrum(!visualizer));
                    }
                    choice_row(
                        ui,
                        palette,
                        strings.viz_bars,
                        &mut actions,
                        |ui, actions| {
                            for bars in VIZ_BAR_CHOICES {
                                let selected = state.settings.viz_bars == bars;
                                let caption =
                                    i18n::fill(strings.bars, &[("count", &bars.to_string())]);
                                if selectable(ui, selected, &caption).clicked() && !selected {
                                    actions.push(UiAction::SetVizBars(bars));
                                }
                            }
                        },
                    );

                    section(ui, palette, strings.language);
                    ui.horizontal_wrapped(|ui| {
                        for language in Language::ALL {
                            let selected = state.settings.language == language;
                            let caption = i18n::language_label(state.language, language);
                            if selectable(ui, selected, caption).clicked() && !selected {
                                actions.push(UiAction::SetLanguage(language));
                            }
                        }
                    });
                    hint(ui, palette, strings.language_hint);

                    section(ui, palette, strings.audio);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(strings.volume)
                                .size(12.0)
                                .color(palette.text),
                        );
                        let slider = volume_slider_width(ui, palette, state.volume, VOLUME_WIDTH);
                        if slider.changed && !slider.committed {
                            actions.push(UiAction::PreviewVolume(*state.volume));
                        }
                        if slider.committed {
                            actions.push(UiAction::CommitVolume(*state.volume));
                        }
                    });
                    hint(ui, palette, strings.audio_hint);

                    section(ui, palette, strings.motion);
                    choice_row(
                        ui,
                        palette,
                        strings.motion_reactivity,
                        &mut actions,
                        |ui, actions| {
                            for motion in MotionReactivity::ALL {
                                let selected = state.settings.motion == motion;
                                if selectable(
                                    ui,
                                    selected,
                                    i18n::motion_label(state.language, motion),
                                )
                                .clicked()
                                    && !selected
                                {
                                    actions.push(UiAction::SetMotion(motion));
                                }
                            }
                        },
                    );
                    choice_row(
                        ui,
                        palette,
                        strings.motion_style,
                        &mut actions,
                        |ui, actions| {
                            for style in MotionStyle::ALL {
                                let selected = state.settings.motion_style == style;
                                if selectable(
                                    ui,
                                    selected,
                                    i18n::style_label(state.language, style),
                                )
                                .clicked()
                                    && !selected
                                {
                                    actions.push(UiAction::SetMotionStyle(style));
                                }
                            }
                        },
                    );
                    let reduce = state.settings.reduce_motion;
                    if ui
                        .checkbox(&mut { reduce }, strings.reduce_motion)
                        .on_hover_text(strings.reduce_motion_hint)
                        .clicked()
                    {
                        actions.push(UiAction::SetReduceMotion(!reduce));
                    }
                    hint(ui, palette, strings.reduce_motion_hint);
                    // Перевороты имеют смысл только при живом движении (ADR-0028).
                    let flips_available = state.settings.motion != MotionReactivity::Off
                        && !state.settings.reduce_motion;
                    let flips = state.settings.allow_flips;
                    ui.add_enabled_ui(flips_available, |ui| {
                        if ui
                            .checkbox(&mut { flips }, strings.allow_flips)
                            .on_hover_text(strings.allow_flips_hint)
                            .clicked()
                        {
                            actions.push(UiAction::SetAllowFlips(!flips));
                        }
                    });
                    hint(
                        ui,
                        palette,
                        if flips_available {
                            strings.allow_flips_hint
                        } else {
                            strings.flips_unavailable
                        },
                    );

                    section(ui, palette, strings.window);
                    choice_row(
                        ui,
                        palette,
                        strings.window_mode,
                        &mut actions,
                        |ui, actions| {
                            for mode in WindowMode::ALL {
                                let selected = state.window_mode == mode;
                                if selectable(ui, selected, mode_label(labels, mode)).clicked()
                                    && !selected
                                {
                                    actions.push(UiAction::SetWindowMode(mode));
                                }
                            }
                        },
                    );
                    hint(ui, palette, strings.window_hint);
                    let help = state.show_help;
                    if ui
                        .checkbox(&mut { help }, text.common.mouse_controls)
                        .clicked()
                    {
                        actions.push(UiAction::ToggleHelp);
                    }
                    ui.horizontal(|ui| {
                        if ui.button(text.common.hide_window).clicked() {
                            actions.push(UiAction::Hide);
                        }
                        if ui.button(text.common.quit).clicked() {
                            actions.push(UiAction::Quit);
                        }
                    });

                    section(ui, palette, strings.about);
                    about_row(ui, palette, strings.version, crate::APP_VERSION);
                    about_row(ui, palette, strings.license, crate::APP_LICENSE);
                    about_row(ui, palette, strings.app_id, crate::APP_ID);
                    hint(ui, palette, strings.about_hint);
                    hint(ui, palette, strings.saved_hint);
                });
        });

    actions
}

/// Заголовок секции: акцентная подпись поверх разделителя.
fn section(ui: &mut egui::Ui, palette: &Palette, title: &str) {
    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(title)
            .size(11.0)
            .strong()
            .color(palette.accent),
    );
}

/// Поясняющая строка мелким приглушённым шрифтом.
fn hint(ui: &mut egui::Ui, palette: &Palette, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(10.0)
            .color(palette.muted)
            .weak(),
    );
}

/// Подпись варианта + значения в строке с переносом: так длинные немецкие и
/// французские подписи не выезжают за край drawer'а.
fn choice_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    title: &str,
    actions: &mut Vec<UiAction>,
    body: impl FnOnce(&mut egui::Ui, &mut Vec<UiAction>),
) {
    ui.label(egui::RichText::new(title).size(12.0).color(palette.text));
    ui.horizontal_wrapped(|ui| body(ui, actions));
}

fn selectable(ui: &mut egui::Ui, selected: bool, caption: &str) -> egui::Response {
    ui.selectable_label(selected, caption)
}

fn about_row(ui: &mut egui::Ui, palette: &Palette, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(key).size(11.0).color(palette.muted));
        ui.label(egui::RichText::new(value).size(11.0).color(palette.text));
    });
}

/// Подпись режима окна из каталога: `WindowMode` — тип session, а i18n от него
/// не зависит, поэтому соответствие развёрнуто здесь (ADR-0030).
fn mode_label(labels: &crate::i18n::LabelText, mode: WindowMode) -> &'static str {
    match mode {
        WindowMode::Focus => labels.mode_focus,
        WindowMode::Overlay => labels.mode_overlay,
        WindowMode::Listening => labels.mode_listening,
    }
}
