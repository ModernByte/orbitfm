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
use super::controls::{checkbox, dropdown, row};
use super::icons::close_button;
use super::palette::{Palette, accent_color};
use super::widgets::{faded, faded_palette, quiet_button, volume_slider_width};

/// Слот под dropdown в строке настройки: 168×26 при ширине drawer'а 380 px.
const DROPDOWN_SLOT: (f32, f32) = (168.0, 26.0);
/// Слот громкости: подпись «Громкость» и трек в одной строке.
const VOLUME_SLOT: (f32, f32) = (150.0, 18.0);
/// Слот палитры акцентов: шесть кружков в правой части строки. 220 px — 6×28
/// плюс пять промежутков по 8 px, чтобы полоса не переносилась.
const SWATCH_SLOT: (f32, f32) = (220.0, 22.0);
/// Размер кружка акцента в палитре.
const SWATCH_SIZE: egui::Vec2 = egui::Vec2::new(28.0, 22.0);
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
                        if close_button(ui, palette, text.common.close).clicked() {
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
                    let dim = faded_palette(palette, appearance);
                    dimmed(ui, appearance, |ui| {
                        section(ui, palette, strings.appearance, appearance);
                        if let Some(index) =
                            row(ui, &dim, strings.theme, None, DROPDOWN_SLOT, |ui| {
                                dropdown(
                                    ui,
                                    &dim,
                                    "orbitfm_set_theme",
                                    DROPDOWN_SLOT.0,
                                    &ThemeMode::ALL,
                                    state.settings.theme,
                                    |index| {
                                        i18n::theme_label(state.language, ThemeMode::ALL[index])
                                            .to_owned()
                                    },
                                )
                            })
                        {
                            actions.push(UiAction::SetTheme(ThemeMode::ALL[index]));
                        }
                        row(ui, &dim, strings.accent, None, SWATCH_SLOT, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                for accent in AccentColor::ALL {
                                    let selected = state.settings.accent == accent;
                                    let stroke = if selected {
                                        egui::Stroke::new(2.0, dim.text)
                                    } else {
                                        egui::Stroke::new(1.0, dim.chip_stroke)
                                    };
                                    // Кружок — вне палитры: цвет берётся из
                                    // варианта акцента, поэтому гасится множителем.
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
                            });
                        });
                        let visualizer = state.settings.visualizer_enabled;
                        if checkbox(ui, &dim, visualizer, true, strings.visualizer).clicked() {
                            actions.push(UiAction::ToggleSpectrum(!visualizer));
                        }
                        // Число полос живёт только вместе с визуализатором.
                        let bars = ui.add_enabled_ui(visualizer, |ui| {
                            row(ui, &dim, strings.viz_bars, None, DROPDOWN_SLOT, |ui| {
                                dropdown(
                                    ui,
                                    &dim,
                                    "orbitfm_set_viz_bars",
                                    DROPDOWN_SLOT.0,
                                    &VIZ_BAR_CHOICES,
                                    state.settings.viz_bars,
                                    |index| {
                                        i18n::fill(
                                            strings.bars,
                                            &[("count", &VIZ_BAR_CHOICES[index].to_string())],
                                        )
                                    },
                                )
                            })
                        });
                        if let Some(index) = bars.inner {
                            actions.push(UiAction::SetVizBars(VIZ_BAR_CHOICES[index]));
                        }
                    });

                    let language = shown(BLOCK_LANGUAGE);
                    let dim = faded_palette(palette, language);
                    dimmed(ui, language, |ui| {
                        section(ui, palette, strings.language, language);
                        if let Some(index) = row(
                            ui,
                            &dim,
                            strings.language,
                            Some(strings.language_hint),
                            DROPDOWN_SLOT,
                            |ui| {
                                dropdown(
                                    ui,
                                    &dim,
                                    "orbitfm_set_language",
                                    DROPDOWN_SLOT.0,
                                    &Language::ALL,
                                    state.settings.language,
                                    |index| {
                                        i18n::language_label(state.language, Language::ALL[index])
                                            .to_owned()
                                    },
                                )
                            },
                        ) {
                            actions.push(UiAction::SetLanguage(Language::ALL[index]));
                        }
                    });

                    let audio = shown(BLOCK_AUDIO);
                    let dim = faded_palette(palette, audio);
                    dimmed(ui, audio, |ui| {
                        section(ui, palette, strings.audio, audio);
                        // Слайдер рисует палитрой, поэтому получает уже погашенную
                        // палитру — второго затемнения (`faded_group`) здесь нет.
                        let slider = row(
                            ui,
                            &dim,
                            strings.volume,
                            Some(strings.audio_hint),
                            VOLUME_SLOT,
                            |ui| {
                                volume_slider_width(
                                    ui,
                                    &dim,
                                    state.volume,
                                    VOLUME_SLOT.0,
                                    state.language,
                                )
                            },
                        );
                        if slider.changed && !slider.committed {
                            actions.push(UiAction::PreviewVolume(*state.volume));
                        }
                        if slider.committed {
                            actions.push(UiAction::CommitVolume(*state.volume));
                        }
                    });

                    let accessibility = shown(BLOCK_MOTION);
                    let dim = faded_palette(palette, accessibility);
                    dimmed(ui, accessibility, |ui| {
                        section(ui, palette, strings.motion, accessibility);
                        if let Some(index) = row(
                            ui,
                            &dim,
                            strings.motion_reactivity,
                            Some(strings.motion_spin_hint),
                            DROPDOWN_SLOT,
                            |ui| {
                                dropdown(
                                    ui,
                                    &dim,
                                    "orbitfm_set_motion",
                                    DROPDOWN_SLOT.0,
                                    &MotionReactivity::ALL,
                                    state.settings.motion,
                                    |index| {
                                        i18n::motion_label(
                                            state.language,
                                            MotionReactivity::ALL[index],
                                        )
                                        .to_owned()
                                    },
                                )
                            },
                        ) {
                            actions.push(UiAction::SetMotion(MotionReactivity::ALL[index]));
                        }
                        if let Some(index) =
                            row(ui, &dim, strings.motion_style, None, DROPDOWN_SLOT, |ui| {
                                dropdown(
                                    ui,
                                    &dim,
                                    "orbitfm_set_motion_style",
                                    DROPDOWN_SLOT.0,
                                    &MotionStyle::ALL,
                                    state.settings.motion_style,
                                    |index| {
                                        i18n::style_label(state.language, MotionStyle::ALL[index])
                                            .to_owned()
                                    },
                                )
                            })
                        {
                            actions.push(UiAction::SetMotionStyle(MotionStyle::ALL[index]));
                        }
                        let reduce = state.settings.reduce_motion;
                        if checkbox(ui, &dim, reduce, true, strings.reduce_motion)
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
                        if checkbox(ui, &dim, flips, flips_available, strings.allow_flips)
                            .on_hover_text(strings.allow_flips_hint)
                            .clicked()
                        {
                            actions.push(UiAction::SetAllowFlips(!flips));
                        }
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
                    let dim = faded_palette(palette, window);
                    dimmed(ui, window, |ui| {
                        section(ui, palette, strings.window, window);
                        if let Some(index) = row(
                            ui,
                            &dim,
                            strings.window_mode,
                            Some(strings.window_hint),
                            DROPDOWN_SLOT,
                            |ui| {
                                dropdown(
                                    ui,
                                    &dim,
                                    "orbitfm_set_window_mode",
                                    DROPDOWN_SLOT.0,
                                    &WindowMode::ALL,
                                    state.window_mode,
                                    |index| mode_label(labels, WindowMode::ALL[index]).to_owned(),
                                )
                            },
                        ) {
                            actions.push(UiAction::SetWindowMode(WindowMode::ALL[index]));
                        }
                        let help = state.show_help;
                        if checkbox(ui, &dim, help, true, strings.show_help).clicked() {
                            actions.push(UiAction::ToggleHelp);
                        }
                        // В безрамочных режимах это единственный выход: крестик
                        // окна там не чем нажать.
                        ui.add_space(6.0);
                        if quiet_button(ui, &dim, text.common.hide_window, dim.text).clicked() {
                            actions.push(UiAction::Hide);
                        }
                        if quiet_button(ui, &dim, text.common.quit, dim.error).clicked() {
                            actions.push(UiAction::Quit);
                        }
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
        // Panel не входит в `WindowMode::ALL`: переключатель раскладок его
        // не предлагает, и эта ветка нужна только для полноты match.
        WindowMode::Panel => labels.mode_focus,
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
