//! Drawer настроек: правая модальная панель со всеми секциями (ADR-0031).
//!
//! Модуль ничего не сохраняет сам — он возвращает [`UiAction`], а побочные
//! эффекты выполняет presentation-слой. Подписи берутся из каталога уже
//! разрешённого языка, поэтому смена языка видна в том же кадре (ADR-0030).
//!
//! Приход и уход каркаса, смещение справа и скрим считает `app.rs`. Здесь
//! гаснут только секции: stagger по прогрессу входа drawer'а действует на
//! прозрачность и цвета, аллокации, идентификаторы виджетов и hit-зоны остаются
//! неподвижными (ADR-0032 §7).

use eframe::egui;

use crate::i18n;
use crate::session::WindowMode;
use crate::settings::{
    AccentColor, Language, MotionReactivity, MotionStyle, Settings, ThemeMode, VIZ_BAR_CHOICES,
};

use super::UiAction;
use super::animation;
use super::icons::close_button;
use super::palette::{Palette, accent_color};
use super::widgets::{faded, faded_group, volume_slider_width};

/// Трек громкости: почти вся ширина drawer'а за вычетом полей.
const VOLUME_WIDTH: f32 = 230.0;
/// Размер кружка акцента в палитре.
const SWATCH_SIZE: egui::Vec2 = egui::Vec2::new(30.0, 22.0);
/// Порядок stagger: заголовок, затем шесть блоков секций (ADR-0032 §4).
const BLOCK_HEADER: usize = 0;
const BLOCK_APPEARANCE: usize = 1;
const BLOCK_LANGUAGE: usize = 2;
const BLOCK_AUDIO: usize = 3;
const BLOCK_MOTION: usize = 4;
const BLOCK_WINDOW: usize = 5;
const BLOCK_ABOUT: usize = 6;

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
    motion: &mut animation::FrameMotion<'_>,
    state: SettingsState<'_>,
) -> Vec<UiAction> {
    let text = i18n::catalog(state.language);
    let strings = &text.settings;
    let labels = &text.labels;
    let mut actions = Vec::new();

    // Значения считает ядро: на reduce motion прогресс уже конечный, а
    // промежуточных состояний не бывает.
    let drawer = animation::alpha(motion.drawer);
    let no_motion = motion.reduce;
    let shown = |block: usize| section_alpha(no_motion, drawer, block);

    egui::Frame::default()
        .fill(egui::Color32::TRANSPARENT)
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let header = shown(BLOCK_HEADER);
            dimmed(ui, header, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(strings.title)
                            .size(15.0)
                            .strong()
                            .color(faded(palette.text, header)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if close_button(ui, text.common.close).clicked() {
                            actions.push(UiAction::CloseSettings);
                        }
                    });
                });
                ui.separator();
            });

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let appearance = shown(BLOCK_APPEARANCE);
                    dimmed(ui, appearance, |ui| {
                        section(ui, palette, strings.appearance, appearance);
                        choice_row(
                            ui,
                            palette,
                            strings.theme,
                            appearance,
                            &mut actions,
                            |ui, actions| {
                                for theme in ThemeMode::ALL {
                                    let selected = state.settings.theme == theme;
                                    let label = i18n::theme_label(state.language, theme);
                                    if selectable(ui, selected, label).clicked() && !selected {
                                        actions.push(UiAction::SetTheme(theme));
                                    }
                                }
                            },
                        );
                        choice_row(
                            ui,
                            palette,
                            strings.accent,
                            appearance,
                            &mut actions,
                            |ui, actions| {
                                for accent in AccentColor::ALL {
                                    let selected = state.settings.accent == accent;
                                    let stroke = if selected {
                                        egui::Stroke::new(2.0, faded(palette.text, appearance))
                                    } else {
                                        egui::Stroke::new(
                                            1.0,
                                            faded(palette.chip_stroke, appearance),
                                        )
                                    };
                                    let swatch = ui
                                        .add(
                                            egui::Button::new("")
                                                .fill(faded(accent_color(accent), appearance))
                                                .stroke(stroke)
                                                .min_size(SWATCH_SIZE)
                                                .corner_radius(6.0),
                                        )
                                        .on_hover_text(i18n::accent_label(state.language, accent));
                                    if swatch.clicked() && !selected {
                                        actions.push(UiAction::SetAccent(accent));
                                    }
                                }
                            },
                        );
                        let visualizer = state.settings.visualizer_enabled;
                        if ui
                            .checkbox(&mut { visualizer }, strings.visualizer)
                            .clicked()
                        {
                            actions.push(UiAction::ToggleSpectrum(!visualizer));
                        }
                        choice_row(
                            ui,
                            palette,
                            strings.viz_bars,
                            appearance,
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
                    });

                    let language = shown(BLOCK_LANGUAGE);
                    dimmed(ui, language, |ui| {
                        section(ui, palette, strings.language, language);
                        ui.horizontal_wrapped(|ui| {
                            for variant in Language::ALL {
                                let selected = state.settings.language == variant;
                                let caption = i18n::language_label(state.language, variant);
                                if selectable(ui, selected, caption).clicked() && !selected {
                                    actions.push(UiAction::SetLanguage(variant));
                                }
                            }
                        });
                        hint(ui, palette, strings.language_hint, language);
                    });

                    let audio = shown(BLOCK_AUDIO);
                    dimmed(ui, audio, |ui| {
                        section(ui, palette, strings.audio, audio);
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(strings.volume)
                                    .size(12.0)
                                    .color(faded(palette.text, audio)),
                            );
                            // Трек громкости рисуется цветами палитры мимо
                            // `Visuals`, поэтому гасится множителем прозрачности.
                            let slider = faded_group(ui, audio, |ui| {
                                volume_slider_width(
                                    ui,
                                    palette,
                                    state.volume,
                                    VOLUME_WIDTH,
                                    state.language,
                                )
                            });
                            if slider.changed && !slider.committed {
                                actions.push(UiAction::PreviewVolume(*state.volume));
                            }
                            if slider.committed {
                                actions.push(UiAction::CommitVolume(*state.volume));
                            }
                        });
                        hint(ui, palette, strings.audio_hint, audio);
                    });

                    let accessibility = shown(BLOCK_MOTION);
                    dimmed(ui, accessibility, |ui| {
                        section(ui, palette, strings.motion, accessibility);
                        choice_row(
                            ui,
                            palette,
                            strings.motion_reactivity,
                            accessibility,
                            &mut actions,
                            |ui, actions| {
                                for reactivity in MotionReactivity::ALL {
                                    let selected = state.settings.motion == reactivity;
                                    let label = i18n::motion_label(state.language, reactivity);
                                    if selectable(ui, selected, label).clicked() && !selected {
                                        actions.push(UiAction::SetMotion(reactivity));
                                    }
                                }
                            },
                        );
                        choice_row(
                            ui,
                            palette,
                            strings.motion_style,
                            accessibility,
                            &mut actions,
                            |ui, actions| {
                                for style in MotionStyle::ALL {
                                    let selected = state.settings.motion_style == style;
                                    let label = i18n::style_label(state.language, style);
                                    if selectable(ui, selected, label).clicked() && !selected {
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
                        hint(ui, palette, strings.reduce_motion_hint, accessibility);
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
                            accessibility,
                        );
                    });

                    let window = shown(BLOCK_WINDOW);
                    dimmed(ui, window, |ui| {
                        section(ui, palette, strings.window, window);
                        choice_row(
                            ui,
                            palette,
                            strings.window_mode,
                            window,
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
                        hint(ui, palette, strings.window_hint, window);
                        let help = state.show_help;
                        if ui.checkbox(&mut { help }, strings.show_help).clicked() {
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
                    });

                    let about = shown(BLOCK_ABOUT);
                    dimmed(ui, about, |ui| {
                        section(ui, palette, strings.about, about);
                        about_row(ui, palette, strings.version, crate::APP_VERSION, about);
                        about_row(ui, palette, strings.license, crate::APP_LICENSE, about);
                        about_row(ui, palette, strings.app_id, crate::APP_ID, about);
                        hint(ui, palette, strings.about_hint, about);
                        hint(ui, palette, strings.saved_hint, about);
                    });
                });
        });

    actions
}

/// Прозрачность блока секций: общий stagger по входу каркаса drawer'а.
/// При reduce motion переходов нет — значение сразу конечное, без задержек.
fn section_alpha(no_motion: bool, drawer: f32, block: usize) -> f32 {
    let progress = animation::alpha(drawer);
    if no_motion {
        return progress;
    }
    animation::alpha(animation::stagger(block, progress))
}

/// Заголовок секции: акцентная подпись поверх разделителя.
fn section(ui: &mut egui::Ui, palette: &Palette, title: &str, shown: f32) {
    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(title)
            .size(11.0)
            .strong()
            .color(faded(palette.accent, shown)),
    );
}

/// Поясняющая строка мелким приглушённым шрифтом.
fn hint(ui: &mut egui::Ui, palette: &Palette, text: &str, shown: f32) {
    ui.label(
        egui::RichText::new(text)
            .size(10.0)
            .color(faded(palette.muted, shown))
            .weak(),
    );
}

/// Подпись варианта + значения в строке с переносом: так длинные немецкие и
/// французские подписи не выезжают за край drawer'а.
fn choice_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    title: &str,
    shown: f32,
    actions: &mut Vec<UiAction>,
    body: impl FnOnce(&mut egui::Ui, &mut Vec<UiAction>),
) {
    ui.label(
        egui::RichText::new(title)
            .size(12.0)
            .color(faded(palette.text, shown)),
    );
    ui.horizontal_wrapped(|ui| body(ui, actions));
}

fn selectable(ui: &mut egui::Ui, selected: bool, caption: &str) -> egui::Response {
    ui.selectable_label(selected, caption)
}

fn about_row(ui: &mut egui::Ui, palette: &Palette, key: &str, value: &str, shown: f32) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(key)
                .size(11.0)
                .color(faded(palette.muted, shown)),
        );
        ui.label(
            egui::RichText::new(value)
                .size(11.0)
                .color(faded(palette.text, shown)),
        );
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

/// Временно гасит стандартные контролы egui: правятся только цвета `Visuals`,
/// поэтому раскладка, идентификаторы виджетов и hit-зоны не двигаются.
/// Вложенный `Ui` не создаётся — иначе поехали бы auto-идентификаторы контролов.
fn dimmed<R>(ui: &mut egui::Ui, shown: f32, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    if shown >= 1.0 {
        return body(ui);
    }
    let style = ui.style().clone();
    fade_visuals(ui.visuals_mut(), shown);
    let outcome = body(ui);
    ui.set_style(style);
    outcome
}

/// Приглушение красок контролов: текст, фон, обводки и выделение.
fn fade_visuals(visuals: &mut egui::Visuals, shown: f32) {
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.bg_fill = faded(widget.bg_fill, shown);
        widget.weak_bg_fill = faded(widget.weak_bg_fill, shown);
        widget.fg_stroke.color = faded(widget.fg_stroke.color, shown);
        widget.bg_stroke.color = faded(widget.bg_stroke.color, shown);
    }
    visuals.selection.bg_fill = faded(visuals.selection.bg_fill, shown);
    visuals.selection.stroke.color = faded(visuals.selection.stroke.color, shown);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Все блоки drawer'а: от заголовка до самой дальной секции.
    const BLOCKS: [usize; 7] = [
        BLOCK_HEADER,
        BLOCK_APPEARANCE,
        BLOCK_LANGUAGE,
        BLOCK_AUDIO,
        BLOCK_MOTION,
        BLOCK_WINDOW,
        BLOCK_ABOUT,
    ];

    #[test]
    fn reduce_motion_gives_the_final_alpha_without_a_transition() {
        for block in BLOCKS {
            assert_eq!(section_alpha(true, 1.0, block), 1.0, "блок {block}");
            assert_eq!(section_alpha(true, 0.0, block), 0.0, "блок {block}");
        }
    }

    #[test]
    fn settled_drawer_shows_every_block_at_full_strength() {
        for block in BLOCKS {
            assert_eq!(section_alpha(false, 1.0, block), 1.0, "блок {block}");
        }
    }

    #[test]
    fn blocks_enter_one_after_another() {
        // В самом начале виден только заголовок, хвост ещё не пришёл.
        assert!(section_alpha(false, 0.05, BLOCK_HEADER) > 0.0);
        assert_eq!(section_alpha(false, 0.05, BLOCK_ABOUT), 0.0);
        for pair in BLOCKS.windows(2) {
            assert!(
                section_alpha(false, 0.5, pair[0]) >= section_alpha(false, 0.5, pair[1]),
                "блок {} не должен опережать блок {}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn closing_runs_the_stagger_backwards() {
        // Прогресс каркаса убывает: хвост гаснет раньше заголовка.
        assert!(section_alpha(false, 0.4, BLOCK_ABOUT) < section_alpha(false, 0.4, BLOCK_HEADER));
    }

    #[test]
    fn alpha_never_leaves_the_visible_range() {
        for progress in [0.0, 0.01, 0.33, 0.5, 0.99, 1.0, f32::NAN, -2.0, 7.0] {
            for block in BLOCKS {
                let value = section_alpha(false, progress, block);
                assert!(
                    (0.0..=1.0).contains(&value),
                    "блок {block} при прогрессе {progress} дал {value}"
                );
            }
        }
    }
}
