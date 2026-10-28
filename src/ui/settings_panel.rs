//! Модальное окно настроек (ADR-0031): слева категории, справа карточки.
//!
//! Модуль ничего не сохраняет сам — он возвращает [`UiAction`], а побочные
//! эффекты выполняет presentation-слой. Подписи берутся из каталога уже
//! разрешённого языка, поэтому смена языка видна в следующем кадре (ADR-0030).
//!
//! Приход и уход каркаса, тень и скрим считает `app.rs`. Здесь гаснут только
//! блоки: stagger по прогрессу входа действует на прозрачность и цвета,
//! аллокации, идентификаторы виджетов и hit-зоны остаются неподвижными
//! (ADR-0032 §7).

use eframe::egui;

use crate::i18n;
use crate::session::WindowMode;
use crate::settings::{
    AccentColor, Language, MotionReactivity, MotionStyle, Settings, ThemeMode, VIZ_BAR_CHOICES,
};

use super::UiAction;
use super::animation;
use super::controls::{accent_swatch, dropdown, row, switch_row};
use super::icons::{self, Icon, close_button};
use super::palette::{Palette, accent_color, mix_colors};
use super::widgets::{
    control_motion, faded, faded_palette, focus_ring, quiet_button, volume_slider_width,
};

/// Шапка модалки: иконка, заголовок, Esc и крестик.
const HEADER_HEIGHT: f32 = 58.0;
const HEADER_MARGIN: f32 = 20.0;
/// Поля тела: навигация отделена от краёв, контент — от шапки.
const BODY_MARGIN: f32 = 16.0;
/// Колонка категорий: полная подпись на широком окне, рейка иконок — на узком.
const NAV_WIDTH: f32 = 188.0;
const NAV_RAIL: f32 = 64.0;
const NAV_BREAKPOINT: f32 = 640.0;
/// Слот под dropdown в строке настройки и под палитру акцентов.
const DROPDOWN_SLOT: (f32, f32) = (220.0, 30.0);
const SWATCH_SLOT: (f32, f32) = (184.0, 26.0);
const SWATCH_GAP: f32 = 4.0;
/// Насколько гаснет контрол, недоступный из-за другой настройки.
const DISABLED: f32 = 0.45;
/// Порядок stagger: шапка, навигация, заголовок категории, затем карточки.
const BLOCK_HEADER: usize = 0;
const BLOCK_NAV: usize = 1;
const BLOCK_TITLE: usize = 2;
const BLOCK_CARD: usize = 3;

/// Категории модального окна: порядок навигации и иконки.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SettingsCategory {
    #[default]
    Appearance,
    Language,
    Audio,
    Motion,
    Window,
    About,
}

impl SettingsCategory {
    pub(crate) const ALL: [Self; 6] = [
        Self::Appearance,
        Self::Language,
        Self::Audio,
        Self::Motion,
        Self::Window,
        Self::About,
    ];

    fn icon(self) -> Icon {
        match self {
            Self::Appearance => Icon::Moon,
            Self::Language => Icon::Globe,
            Self::Audio => Icon::Volume,
            Self::Motion => Icon::Sparkles,
            Self::Window => Icon::AppWindow,
            Self::About => Icon::Help,
        }
    }
}

pub(crate) struct SettingsState<'a> {
    pub settings: Settings,
    /// Разрешённый язык интерфейса: `System` здесь уже стал конкретным.
    pub language: Language,
    pub window_mode: WindowMode,
    /// Копия громкости, которую двигает слайдер (commit-on-release, ADR-0031).
    pub volume: &'a mut f64,
    pub show_help: bool,
    /// Открытая категория: состояние окна, а не персистентная настройка.
    pub category: &'a mut SettingsCategory,
}

pub(crate) fn show(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    palette: &Palette,
    motion: &animation::FrameMotion<'_>,
    mut state: SettingsState<'_>,
) -> Vec<UiAction> {
    let text = i18n::catalog(state.language);
    let strings = &text.settings;
    let mut actions = Vec::new();

    // Значения считает ядро: на reduce motion прогресс уже конечный, а
    // промежуточных состояний не бывает.
    let settings_in = animation::alpha(motion.settings);
    let no_motion = motion.reduce;
    let shown = |block: usize| section_alpha(no_motion, settings_in, block);

    // Клик по пустому стеклу принадлежит модалке, а не скриму под ней.
    ui.interact(
        rect,
        ui.scope_id().with("orbitfm_settings_body"),
        egui::Sense::click(),
    );

    let header = egui::Rect::from_min_max(
        egui::pos2(rect.left() + HEADER_MARGIN, rect.top()),
        egui::pos2(rect.right() - HEADER_MARGIN, rect.top() + HEADER_HEIGHT),
    );
    header_bar(ui, header, palette, text, shown(BLOCK_HEADER), &mut actions);
    ui.painter().hline(
        header.x_range(),
        header.bottom() - 4.0,
        egui::Stroke::new(1.0, faded(palette.chip_stroke, shown(BLOCK_HEADER))),
    );

    let body = egui::Rect::from_min_max(
        egui::pos2(rect.left() + BODY_MARGIN, header.bottom() + 6.0),
        egui::pos2(rect.right() - BODY_MARGIN, rect.bottom() - BODY_MARGIN),
    );
    let footer_height = 32.0;
    let footer = egui::Rect::from_min_max(
        egui::pos2(body.left(), body.bottom() - footer_height),
        body.right_bottom(),
    );
    let body = body.with_max_y(footer.top() - 8.0);
    ui.painter().hline(
        footer.x_range(),
        footer.top(),
        egui::Stroke::new(1.0, faded(palette.chip_stroke, shown(BLOCK_TITLE))),
    );
    ui.painter().text(
        footer.center(),
        egui::Align2::CENTER_CENTER,
        strings.saved_hint,
        egui::FontId::proportional(10.5),
        faded(palette.muted, shown(BLOCK_TITLE)),
    );
    let nav_column = nav_width(rect.width());
    let rail = nav_column == NAV_RAIL;
    let nav_panel = egui::Rect::from_min_size(body.min, egui::vec2(nav_column, body.height()));
    let nav_alpha = shown(BLOCK_NAV);
    ui.painter()
        .rect_filled(nav_panel, 14.0, faded(palette.card_fill, 0.30 * nav_alpha));

    let nav_palette = faded_palette(palette, nav_alpha);
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(nav_panel.shrink2(egui::vec2(8.0, 10.0)))
            .layout(egui::Layout::top_down(egui::Align::Min)),
        |ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            for category in SettingsCategory::ALL {
                let label = category_label(strings, category);
                let selected = category == *state.category;
                if nav_item(ui, &nav_palette, category.icon(), label, selected, rail).clicked() {
                    *state.category = category;
                }
            }
        },
    );

    let content = egui::Rect::from_min_max(
        egui::pos2(nav_panel.right() + 16.0, body.top()),
        body.right_bottom(),
    );
    let title_palette = faded_palette(palette, shown(BLOCK_TITLE));
    let cards = [shown(BLOCK_CARD), shown(BLOCK_CARD + 1)];
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(content)
            .layout(egui::Layout::top_down(egui::Align::Min)),
        |ui| {
            ui.label(
                egui::RichText::new(category_label(strings, *state.category))
                    .size(18.0)
                    .strong()
                    .color(title_palette.text),
            );
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(category_hint(strings, *state.category))
                    .size(11.5)
                    .color(title_palette.muted),
            );
            ui.add_space(10.0);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    draw_category(ui, palette, text, &mut state, cards, &mut actions);
                    ui.add_space(4.0);
                });
        },
    );

    actions
}

/// Шапка: знак настроек, заголовок, подсказка Esc и крестик.
fn header_bar(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    palette: &Palette,
    text: &i18n::Catalog,
    alpha: f32,
    actions: &mut Vec<UiAction>,
) {
    let strings = &text.settings;
    let dim = faded_palette(palette, alpha);
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            let (mark, _) = ui.allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(mark, 10.0, faded(palette.accent, 0.16 * alpha));
            ui.painter().rect_stroke(
                mark,
                10.0,
                egui::Stroke::new(1.0, faded(palette.accent, 0.28 * alpha)),
                egui::StrokeKind::Inside,
            );
            icons::paint_icon(ui, Icon::Settings, mark.center(), 18.0, dim.accent);
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(strings.title)
                    .size(16.5)
                    .strong()
                    .color(dim.text),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if close_button(ui, &dim, text.common.close).clicked() {
                    actions.push(UiAction::CloseSettings);
                }
                ui.add_space(4.0);
                let (esc, _) = ui.allocate_exact_size(egui::vec2(34.0, 20.0), egui::Sense::hover());
                ui.painter().rect(
                    esc,
                    6.0,
                    faded(palette.chip_fill, alpha),
                    egui::Stroke::new(1.0, faded(palette.chip_stroke, alpha)),
                    egui::StrokeKind::Inside,
                );
                ui.painter().text(
                    esc.center(),
                    egui::Align2::CENTER_CENTER,
                    "Esc",
                    egui::FontId::monospace(10.0),
                    dim.muted,
                );
            });
        },
    );
}

/// Одна категория навигации: иконка и подпись или только иконка в рейке.
fn nav_item(
    ui: &mut egui::Ui,
    palette: &Palette,
    icon: Icon,
    label: &str,
    selected: bool,
    rail: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::Vec2::new(ui.available_width(), 36.0),
        egui::Sense::click(),
    );
    if ui.is_rect_visible(rect) {
        let (hover, press) = control_motion(
            ui,
            response.id,
            response.hovered(),
            response.is_pointer_button_down_on(),
        );
        if selected {
            ui.painter()
                .rect_filled(rect, 10.0, faded(palette.accent, 0.16 + 0.05 * hover));
        } else if hover > 0.0 || press > 0.0 {
            ui.painter()
                .rect_filled(rect, 10.0, faded(palette.text, 0.05 * hover + 0.04 * press));
        }
        let icon_center = if rail {
            rect.center()
        } else {
            egui::pos2(rect.left() + 20.0, rect.center().y)
        };
        let tint = if selected {
            palette.accent
        } else {
            mix_colors(palette.muted, palette.text, hover)
        };
        icons::paint_icon(ui, icon, icon_center, 17.0, tint);
        if !rail {
            ui.painter().text(
                egui::pos2(rect.left() + 38.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(12.5),
                if selected { palette.text } else { tint },
            );
        }
    }
    focus_ring(ui, &response, rect, 10.0);
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, selected, label));
    if rail {
        response.on_hover_text(label)
    } else {
        response
    }
}

/// Ширина колонки категорий: подпись или рейка иконок на узком окне.
fn nav_width(modal_width: f32) -> f32 {
    if modal_width < NAV_BREAKPOINT {
        NAV_RAIL
    } else {
        NAV_WIDTH
    }
}

/// Содержимое открытой категории: одна или две карточки строк.
fn draw_category(
    ui: &mut egui::Ui,
    palette: &Palette,
    text: &i18n::Catalog,
    state: &mut SettingsState<'_>,
    cards: [f32; 2],
    actions: &mut Vec<UiAction>,
) {
    match *state.category {
        SettingsCategory::Appearance => {
            appearance(ui, palette, &text.settings, state, cards, actions);
        }
        SettingsCategory::Language => {
            language(ui, palette, &text.settings, state, cards[0], actions);
        }
        SettingsCategory::Audio => {
            audio(ui, palette, &text.settings, state, cards[0], actions);
        }
        SettingsCategory::Motion => {
            motion(ui, palette, &text.settings, state, cards, actions);
        }
        SettingsCategory::Window => {
            window(
                ui,
                palette,
                &text.settings,
                &text.labels,
                state,
                cards,
                actions,
            );
        }
        SettingsCategory::About => {
            about(ui, palette, text, cards[0], actions);
        }
    }
}

fn appearance(
    ui: &mut egui::Ui,
    palette: &Palette,
    strings: &i18n::SettingsText,
    state: &mut SettingsState<'_>,
    cards: [f32; 2],
    actions: &mut Vec<UiAction>,
) {
    let alpha = cards[0];
    card(ui, palette, alpha, |ui| {
        let dim = faded_palette(palette, alpha);
        if let Some(index) = row(ui, &dim, strings.theme, None, DROPDOWN_SLOT, |ui| {
            dropdown(
                ui,
                &dim,
                "orbitfm_set_theme",
                ui.available_width(),
                &ThemeMode::ALL,
                state.settings.theme,
                |index| i18n::theme_label(state.language, ThemeMode::ALL[index]).to_owned(),
            )
        }) {
            actions.push(UiAction::SetTheme(ThemeMode::ALL[index]));
        }
        divider(ui, palette, alpha);
        row(ui, &dim, strings.accent, None, SWATCH_SLOT, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = SWATCH_GAP;
                for accent in AccentColor::ALL {
                    let selected = state.settings.accent == accent;
                    let swatch = accent_swatch(
                        ui,
                        &dim,
                        faded(accent_color(accent), alpha),
                        selected,
                        i18n::accent_label(state.language, accent),
                    );
                    if swatch.clicked() && !selected {
                        actions.push(UiAction::SetAccent(accent));
                    }
                }
            });
        });
    });

    let alpha = cards[1];
    ui.add_space(10.0);
    card(ui, palette, alpha, |ui| {
        let dim = faded_palette(palette, alpha);
        let visualizer = state.settings.visualizer_enabled;
        if switch_row(ui, &dim, strings.visualizer, None, visualizer, true).clicked() {
            actions.push(UiAction::ToggleSpectrum(!visualizer));
        }
        divider(ui, palette, alpha);
        // Число полос живёт только вместе с визуализатором.
        let bars_palette = if visualizer {
            dim
        } else {
            faded_palette(&dim, DISABLED)
        };
        let bars = ui.add_enabled_ui(visualizer, |ui| {
            row(
                ui,
                &bars_palette,
                strings.viz_bars,
                None,
                DROPDOWN_SLOT,
                |ui| {
                    dropdown(
                        ui,
                        &bars_palette,
                        "orbitfm_set_viz_bars",
                        ui.available_width(),
                        &VIZ_BAR_CHOICES,
                        state.settings.viz_bars,
                        |index| {
                            i18n::fill(
                                strings.bars,
                                &[("count", &VIZ_BAR_CHOICES[index].to_string())],
                            )
                        },
                    )
                },
            )
        });
        if let Some(index) = bars.inner {
            actions.push(UiAction::SetVizBars(VIZ_BAR_CHOICES[index]));
        }
    });
}

fn language(
    ui: &mut egui::Ui,
    palette: &Palette,
    strings: &i18n::SettingsText,
    state: &mut SettingsState<'_>,
    alpha: f32,
    actions: &mut Vec<UiAction>,
) {
    card(ui, palette, alpha, |ui| {
        let dim = faded_palette(palette, alpha);
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
                    ui.available_width(),
                    &Language::ALL,
                    state.settings.language,
                    |index| i18n::language_label(state.language, Language::ALL[index]).to_owned(),
                )
            },
        ) {
            actions.push(UiAction::SetLanguage(Language::ALL[index]));
        }
    });
}

fn audio(
    ui: &mut egui::Ui,
    palette: &Palette,
    strings: &i18n::SettingsText,
    state: &mut SettingsState<'_>,
    alpha: f32,
    actions: &mut Vec<UiAction>,
) {
    card(ui, palette, alpha, |ui| {
        let dim = faded_palette(palette, alpha);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(strings.volume)
                    .size(12.5)
                    .color(dim.text),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{:.0}%", *state.volume * 100.0))
                        .size(12.0)
                        .strong()
                        .color(dim.accent),
                );
            });
        });
        ui.add_space(2.0);
        // Слайдер рисует палитрой, поэтому получает уже погашенную палитру —
        // второго затемнения здесь нет.
        let width = ui.available_width();
        let slider = volume_slider_width(ui, &dim, state.volume, width, state.language);
        if slider.changed && !slider.committed {
            actions.push(UiAction::PreviewVolume(*state.volume));
        }
        if slider.committed {
            actions.push(UiAction::CommitVolume(*state.volume));
        }
        ui.add_space(3.0);
        hint(ui, palette, strings.audio_hint, alpha);
    });
}

fn motion(
    ui: &mut egui::Ui,
    palette: &Palette,
    strings: &i18n::SettingsText,
    state: &mut SettingsState<'_>,
    cards: [f32; 2],
    actions: &mut Vec<UiAction>,
) {
    let alpha = cards[0];
    card(ui, palette, alpha, |ui| {
        let dim = faded_palette(palette, alpha);
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
                    ui.available_width(),
                    &MotionReactivity::ALL,
                    state.settings.motion,
                    |index| {
                        i18n::motion_label(state.language, MotionReactivity::ALL[index]).to_owned()
                    },
                )
            },
        ) {
            actions.push(UiAction::SetMotion(MotionReactivity::ALL[index]));
        }
        divider(ui, palette, alpha);
        if let Some(index) = row(ui, &dim, strings.motion_style, None, DROPDOWN_SLOT, |ui| {
            dropdown(
                ui,
                &dim,
                "orbitfm_set_motion_style",
                ui.available_width(),
                &MotionStyle::ALL,
                state.settings.motion_style,
                |index| i18n::style_label(state.language, MotionStyle::ALL[index]).to_owned(),
            )
        }) {
            actions.push(UiAction::SetMotionStyle(MotionStyle::ALL[index]));
        }
    });

    let alpha = cards[1];
    ui.add_space(10.0);
    card(ui, palette, alpha, |ui| {
        let dim = faded_palette(palette, alpha);
        let reduce = state.settings.reduce_motion;
        if switch_row(
            ui,
            &dim,
            strings.reduce_motion,
            Some(strings.reduce_motion_hint),
            reduce,
            true,
        )
        .clicked()
        {
            actions.push(UiAction::SetReduceMotion(!reduce));
        }
        divider(ui, palette, alpha);
        // Перевороты имеют смысл только при живом движении (ADR-0028).
        let flips_available = state.settings.motion != MotionReactivity::Off && !reduce;
        let flips = state.settings.allow_flips;
        if switch_row(
            ui,
            &dim,
            strings.allow_flips,
            Some(if flips_available {
                strings.allow_flips_hint
            } else {
                strings.flips_unavailable
            }),
            flips,
            flips_available,
        )
        .clicked()
        {
            actions.push(UiAction::SetAllowFlips(!flips));
        }
    });
}

fn window(
    ui: &mut egui::Ui,
    palette: &Palette,
    strings: &i18n::SettingsText,
    labels: &i18n::LabelText,
    state: &mut SettingsState<'_>,
    cards: [f32; 2],
    actions: &mut Vec<UiAction>,
) {
    let alpha = cards[0];
    card(ui, palette, alpha, |ui| {
        let dim = faded_palette(palette, alpha);
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
                    ui.available_width(),
                    &WindowMode::ALL,
                    state.window_mode,
                    |index| mode_label(labels, WindowMode::ALL[index]).to_owned(),
                )
            },
        ) {
            actions.push(UiAction::SetWindowMode(WindowMode::ALL[index]));
        }
    });

    let alpha = cards[1];
    ui.add_space(10.0);
    card(ui, palette, alpha, |ui| {
        let dim = faded_palette(palette, alpha);
        let help = state.show_help;
        if switch_row(ui, &dim, strings.show_help, None, help, true).clicked() {
            actions.push(UiAction::ToggleHelp);
        }
    });
}

fn about(
    ui: &mut egui::Ui,
    palette: &Palette,
    text: &i18n::Catalog,
    alpha: f32,
    actions: &mut Vec<UiAction>,
) {
    let strings = &text.settings;
    card(ui, palette, alpha, |ui| {
        about_row(ui, palette, strings.version, crate::APP_VERSION, alpha);
        divider(ui, palette, alpha);
        about_row(ui, palette, strings.license, crate::APP_LICENSE, alpha);
        divider(ui, palette, alpha);
        about_row(ui, palette, strings.app_id, crate::APP_ID, alpha);
    });
    ui.add_space(8.0);
    // Единственная кнопка окна: в безрамочных режимах это честный выход, а не
    // скрытие в трей, которое уже делает крестик шапки.
    let dim = faded_palette(palette, alpha);
    if quiet_button(ui, &dim, text.common.quit, faded(palette.error, alpha)).clicked() {
        actions.push(UiAction::Quit);
    }
    ui.add_space(4.0);
    hint(ui, palette, strings.about_hint, alpha);
}

/// Карточка строк: фон и обводка палитры, поля задают общий ритм.
fn card(ui: &mut egui::Ui, palette: &Palette, alpha: f32, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(faded(palette.card_fill, 0.55 * alpha))
        .stroke(egui::Stroke::new(
            1.0,
            faded(palette.chip_stroke, 0.9 * alpha),
        ))
        .corner_radius(14)
        .inner_margin(egui::Margin::symmetric(14, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            body(ui);
        });
}

/// Тонкая линия между строками внутри карточки; всегда занимает одну и ту же
/// высоту, поэтому stagger не двигает раскладку.
fn divider(ui: &mut egui::Ui, palette: &Palette, alpha: f32) {
    ui.add_space(2.0);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(1.0, faded(palette.chip_stroke, 0.7 * alpha)),
    );
    ui.add_space(2.0);
}

/// Поясняющая строка мелким приглушённым шрифтом.
fn hint(ui: &mut egui::Ui, palette: &Palette, text: &str, alpha: f32) {
    ui.label(
        egui::RichText::new(text)
            .size(10.5)
            .color(faded(palette.muted, alpha))
            .weak(),
    );
}

/// Строка «ключ — значение» для категории «О программе».
fn about_row(ui: &mut egui::Ui, palette: &Palette, key: &str, value: &str, alpha: f32) {
    let width = ui.available_width();
    let rect = egui::Rect::from_min_size(
        ui.available_rect_before_wrap().min,
        egui::Vec2::new(width, 28.0),
    );
    ui.allocate_rect(rect, egui::Sense::hover());
    ui.painter().text(
        egui::pos2(rect.left(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        key,
        egui::FontId::proportional(12.0),
        faded(palette.muted, alpha),
    );
    ui.painter().text(
        egui::pos2(rect.right(), rect.center().y),
        egui::Align2::RIGHT_CENTER,
        value,
        egui::FontId::proportional(12.0),
        faded(palette.text, alpha),
    );
}

fn category_label(strings: &i18n::SettingsText, category: SettingsCategory) -> &'static str {
    match category {
        SettingsCategory::Appearance => strings.appearance,
        SettingsCategory::Language => strings.language,
        SettingsCategory::Audio => strings.audio,
        SettingsCategory::Motion => strings.motion,
        SettingsCategory::Window => strings.window,
        SettingsCategory::About => strings.about,
    }
}

/// Подзаголовок категории: одна строка под названием страницы.
fn category_hint(strings: &i18n::SettingsText, category: SettingsCategory) -> &'static str {
    match category {
        SettingsCategory::Appearance => strings.appearance_page_hint,
        SettingsCategory::Language => strings.language_page_hint,
        SettingsCategory::Audio => strings.audio_page_hint,
        SettingsCategory::Motion => strings.motion_page_hint,
        SettingsCategory::Window => strings.window_page_hint,
        SettingsCategory::About => strings.about_page_hint,
    }
}

/// Подпись режима окна из каталога: `WindowMode` — тип session, а i18n от него
/// не зависит, поэтому соответствие развёрнуто здесь (ADR-0030).
fn mode_label(labels: &i18n::LabelText, mode: WindowMode) -> &'static str {
    match mode {
        // Panel не входит в `WindowMode::ALL`: переключатель раскладок его
        // не предлагает, и эта ветка нужна только для полноты match.
        WindowMode::Focus | WindowMode::Panel => labels.mode_focus,
        WindowMode::Overlay => labels.mode_overlay,
        WindowMode::Listening => labels.mode_listening,
    }
}

/// Прозрачность блока: общий stagger по входу модалки.
/// При reduce motion переходов нет — значение сразу конечное, без задержек.
fn section_alpha(no_motion: bool, settings_in: f32, block: usize) -> f32 {
    let progress = animation::alpha(settings_in);
    if no_motion {
        return progress;
    }
    animation::alpha(animation::stagger(block, progress))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Все блоки модалки: шапка, навигация, заголовок и две карточки.
    const BLOCKS: [usize; 5] = [
        BLOCK_HEADER,
        BLOCK_NAV,
        BLOCK_TITLE,
        BLOCK_CARD,
        BLOCK_CARD + 1,
    ];

    #[test]
    fn reduce_motion_gives_the_final_alpha_without_a_transition() {
        for block in BLOCKS {
            assert_eq!(section_alpha(true, 1.0, block), 1.0, "блок {block}");
            assert_eq!(section_alpha(true, 0.0, block), 0.0, "блок {block}");
        }
    }

    #[test]
    fn settled_modal_shows_every_block_at_full_strength() {
        for block in BLOCKS {
            assert_eq!(section_alpha(false, 1.0, block), 1.0, "блок {block}");
        }
    }

    #[test]
    fn blocks_enter_one_after_another() {
        // В самом начале видна только шапка, хвост ещё не пришёл.
        assert!(section_alpha(false, 0.05, BLOCK_HEADER) > 0.0);
        assert_eq!(section_alpha(false, 0.05, BLOCK_CARD + 1), 0.0);
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
        // Прогресс каркаса убывает: хвост гаснет раньше шапки.
        assert!(
            section_alpha(false, 0.4, BLOCK_CARD + 1) < section_alpha(false, 0.4, BLOCK_HEADER)
        );
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

    #[test]
    fn narrow_modal_falls_back_to_the_icon_rail() {
        assert_eq!(nav_width(NAV_BREAKPOINT - 1.0), NAV_RAIL);
        assert_eq!(nav_width(NAV_BREAKPOINT), NAV_WIDTH);
        assert_eq!(nav_width(760.0), NAV_WIDTH);
    }

    #[test]
    fn every_category_has_a_label_and_a_hint() {
        let text = i18n::catalog(Language::Russian);
        for category in SettingsCategory::ALL {
            assert!(!category_label(&text.settings, category).is_empty());
            assert!(!category_hint(&text.settings, category).is_empty());
        }
    }
}
