//! Верхний HUD: логотип, режимы, поиск, now-playing, громкость и настройки.
//!
//! Из `FrameMotion` HUD берёт только вход стола (ADR-0032 §4): каркас стекла
//! гасит `app.rs`, здесь гаснет содержимое — текст, иконки и поле поиска.
//! Своих циклов и запросов кадров у HUD нет.

use eframe::egui;

use crate::audio::PlaybackState;
use crate::geo::{Country, GeoService};
use crate::i18n;
use crate::services::Station;
use crate::settings::{Language, ThemeMode};

use super::animation;
use super::icons::{
    close_button, gear_button, help_button, moon_button, panel_button, roulette_button,
};
use super::images::FaviconCache;
use super::mode_switch::genre_label;
use super::palette::Palette;
use super::widgets::{avatar, faded_group, live_badge, logo_mark, search_field};
use super::{
    SearchMode, UiAction, country_label, mode_switch, playback_label, single_line, truncate,
};

const COMPACT_HUD_WIDTH: f32 = 1000.0;
const TIGHT_HUD_WIDTH: f32 = 840.0;
const COUNTRY_SUGGESTIONS: usize = 6;
/// Высота строки бренда в шапке панели. Строку нельзя отдавать на растяжение
/// вертикальному layout: центрирование по перекрёстной оси тогда считает от
/// всей высоты шапки и уводит поиск с режимами за её нижнюю границу.
const PANEL_BRAND_ROW: f32 = 34.0;
/// Позиция HUD в общем stagger входа Focus-стола. Индексы задает `app.rs`, где
/// тем же значением гаснет каркас стеклянной зоны: содержимое обязано входить
/// синхронно со своим стеклом.
const SCENE_INDEX: usize = 0;

pub(crate) struct HudState<'a> {
    pub query: &'a mut String,
    pub country_query: &'a mut String,
    pub genre_query: &'a mut String,
    pub mode: SearchMode,
    pub selected_genre: Option<&'a str>,
    pub context_active: bool,
    pub geo: &'a GeoService,
    pub now_playing: Option<&'a Station>,
    pub favicons: &'a mut FaviconCache,
    pub playback: &'a PlaybackState,
    pub panel_open: bool,
    pub help_open: bool,
    /// Разрешённый язык интерфейса (ADR-0030).
    pub language: Language,
}

pub(crate) fn show(
    ui: &mut egui::Ui,
    palette: &Palette,
    motion: &mut animation::FrameMotion<'_>,
    mut state: HudState<'_>,
) -> Vec<UiAction> {
    let mut actions = Vec::new();
    let width = ui.available_width();
    let compact = width < COMPACT_HUD_WIDTH;
    let tight = width < TIGHT_HUD_WIDTH;
    let shown = content_alpha(motion.scene);
    let text = i18n::catalog(state.language);

    palette.glass().show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        // Гасим всю группу сразу: часть контролов (иконки, поле поиска) берет цвет
        // из `visuals` egui, поэтому погашенной палитры недостаточно. Затрагивается
        // только отрисовка — аллокации и hit-testing остаются на исходных
        // прямоугольниках (ADR-0032 §7).
        faded_group(ui, shown, |ui| {
            ui.horizontal_centered(|ui| {
                if !tight {
                    logo_mark(ui, palette, 34.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(
                            egui::RichText::new("OrbitFM")
                                .size(16.0)
                                .strong()
                                .color(palette.text),
                        );
                        ui.label(
                            egui::RichText::new(text.hud.subtitle)
                                .size(10.5)
                                .color(palette.muted),
                        );
                    });
                    ui.add_space(14.0);
                }

                actions.extend(mode_switch::show(
                    ui,
                    palette,
                    mode_switch::ModeState {
                        mode: state.mode,
                        selected_genre: state.selected_genre,
                        genre_query: state.genre_query,
                        language: state.language,
                    },
                ));
                ui.add_space(8.0);

                let search_width = if compact { 150.0 } else { 240.0 };
                if state.mode == SearchMode::Country {
                    actions.extend(country_search(
                        ui,
                        palette,
                        state.country_query,
                        state.geo,
                        search_width,
                        state.language,
                    ));
                } else {
                    let hint = if state.context_active {
                        text.hud.filter_stations
                    } else {
                        text.hud.search_stations
                    };
                    let output = search_field(
                        ui,
                        palette,
                        egui::Id::unique("orbitfm_hud_search"),
                        hint,
                        state.query,
                        search_width,
                    );
                    if output.enter {
                        actions.push(UiAction::RunSearch);
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let is_playing = playing(&state);
                    actions.extend(right_side(
                        ui, palette, &mut state, is_playing, compact, tight,
                    ));
                });
            });
        });
    });

    actions
}

/// Альфа содержимого HUD по прогрессу входа стола.
///
/// Ровно тот же `stagger`-индекс, что и у каркаса стекла в `app.rs`, поэтому
/// текст и иконки входят синхронно со своей зоной. При reduce motion `scene`
/// приходит конечной единицей, и хелпер ничего не гасит.
fn content_alpha(scene: f32) -> f32 {
    animation::alpha(animation::stagger(SCENE_INDEX, scene))
}

fn playing(state: &HudState<'_>) -> bool {
    matches!(
        state.playback,
        PlaybackState::Playing | PlaybackState::Buffering | PlaybackState::Reconnecting
    )
}

/// Состояние шапки выпадающей панели бар-виджета (ADR-0033).
pub(crate) struct PanelHeaderState<'a> {
    /// Альфа появления панели: шапка входит вместе со своей поверхностью.
    pub entry: f32,
    pub query: &'a mut String,
    pub country_query: &'a mut String,
    pub genre_query: &'a mut String,
    pub mode: SearchMode,
    pub selected_genre: Option<&'a str>,
    pub context_active: bool,
    pub country: Option<&'a Country>,
    pub genre: Option<&'a str>,
    pub geo: &'a GeoService,
    pub roulette_pending: bool,
    /// Разрешённый язык интерфейса (ADR-0030).
    pub language: Language,
}

/// Шапка выпадающей панели: бренд, поиск и режим поиска. В отличие от HUD
/// стола здесь нет now-playing и раскладок: панель живёт под бар-виджетом и
/// открывается ради станций.
pub(crate) fn panel_header(
    ui: &mut egui::Ui,
    palette: &Palette,
    state: PanelHeaderState<'_>,
) -> Vec<UiAction> {
    let mut actions = Vec::new();
    let text = i18n::catalog(state.language);
    let context = match (state.country, state.genre) {
        (Some(country), _) => {
            let name = single_line(&country.name);
            let name = if name.is_empty() {
                country.iso_a2.clone()
            } else {
                name
            };
            i18n::fill(text.panel.country_prefix, &[("name", &name)])
        }
        (None, Some(genre)) => {
            let tag = genre_label(state.language, genre);
            i18n::fill(text.panel.genre_prefix, &[("genre", &tag)])
        }
        (None, None) => text.hud.subtitle.to_owned(),
    };

    palette.glass().show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        faded_group(ui, state.entry, |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), PANEL_BRAND_ROW),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            logo_mark(ui, palette, 28.0);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                ui.label(
                                    egui::RichText::new("OrbitFM")
                                        .size(15.0)
                                        .strong()
                                        .color(palette.text),
                                );
                                ui.label(
                                    egui::RichText::new(truncate(&context, 26))
                                        .size(10.0)
                                        .color(palette.muted),
                                );
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if close_button(ui, palette, text.common.hide_window_hint)
                                        .clicked()
                                    {
                                        actions.push(UiAction::Hide);
                                    }
                                    if gear_button(ui, palette, text.common.settings).clicked() {
                                        actions.push(UiAction::ToggleSettings);
                                    }
                                    if roulette_button(
                                        ui,
                                        palette,
                                        !state.roulette_pending,
                                        text.transport.random_station,
                                    )
                                    .clicked()
                                    {
                                        actions.push(UiAction::Roulette);
                                    }
                                },
                            );
                        },
                    );
                });

                let width = ui.available_width();
                if state.mode == SearchMode::Country {
                    actions.extend(country_search(
                        ui,
                        palette,
                        state.country_query,
                        state.geo,
                        width,
                        state.language,
                    ));
                } else {
                    let hint = if state.context_active {
                        text.hud.filter_stations
                    } else {
                        text.hud.search_stations
                    };
                    let output = search_field(
                        ui,
                        palette,
                        egui::Id::unique("orbitfm_panel_search"),
                        hint,
                        state.query,
                        width,
                    );
                    if output.enter {
                        actions.push(UiAction::RunSearch);
                    }
                }

                actions.extend(super::mode_switch::show(
                    ui,
                    palette,
                    super::mode_switch::ModeState {
                        mode: state.mode,
                        selected_genre: state.selected_genre,
                        genre_query: state.genre_query,
                        language: state.language,
                    },
                ));
            });
        });
    });

    actions
}

/// Поиск страны с подсказками из `GeoService`; Enter выбирает первую страну.
fn country_search(
    ui: &mut egui::Ui,
    palette: &Palette,
    country_query: &mut String,
    geo: &GeoService,
    width: f32,
    language: Language,
) -> Vec<UiAction> {
    let mut actions = Vec::new();
    let text = i18n::catalog(language);
    let output = search_field(
        ui,
        palette,
        egui::Id::unique("orbitfm_hud_country"),
        text.hud.search_country,
        country_query,
        width,
    );
    let query = country_query.clone();
    let suggestions = geo.search_countries(&query, COUNTRY_SUGGESTIONS);
    // Стабильный id попапа: у якоря поля id меняется между «в фокусе» и «не в
    // фокусе», и попап терял собственную память на кадре отпускания кнопки.
    let popup_id = egui::Id::unique("orbitfm_hud_country_popup");
    // TextEdit отдаёт фокус на клике, то есть уже на кадре отпускания, — без
    // этой проверки попап исчезал раньше, чем кнопка подсказки получала клик.
    let pointer_over_popup = ui
        .ctx()
        .pointer_hover_pos()
        .zip(ui.ctx().read_response(popup_id))
        .is_some_and(|(pointer, popup)| popup.rect.contains(pointer));
    let escape = ui.ctx().input(|input| input.key_pressed(egui::Key::Escape));
    let show_popup = (output.response.has_focus() || output.enter || pointer_over_popup)
        && !query.trim().is_empty()
        && !escape;

    if show_popup && !suggestions.is_empty() {
        egui::Popup::from_response(&output.response)
            .id(popup_id)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(output.rect.width());
                    for country in &suggestions {
                        if ui.button(country_label(country)).clicked() {
                            actions.push(UiAction::SelectCountry((*country).clone()));
                            ui.close();
                        }
                    }
                });
            });
    } else if show_popup {
        ui.label(
            egui::RichText::new(text.common.no_match)
                .size(10.5)
                .color(palette.muted),
        );
    }

    if output.enter
        && actions.is_empty()
        && let Some(country) = suggestions.first()
    {
        actions.push(UiAction::SelectCountry((*country).clone()));
    }
    actions
}

fn right_side(
    ui: &mut egui::Ui,
    palette: &Palette,
    state: &mut HudState<'_>,
    is_playing: bool,
    compact: bool,
    tight: bool,
) -> Vec<UiAction> {
    let mut actions = Vec::new();
    let text = i18n::catalog(state.language);

    let hint = if state.panel_open {
        text.common.hide_station_list
    } else {
        text.common.show_station_list
    };
    if panel_button(ui, palette, hint).clicked() {
        actions.push(UiAction::TogglePanel);
    }

    if help_button(ui, palette, text.common.mouse_controls, state.help_open).clicked() {
        actions.push(UiAction::ToggleHelp);
    }

    // Шестерёка доступна и в узком окне: drawer — единственный вход в настройки.
    if gear_button(ui, palette, text.common.settings).clicked() {
        actions.push(UiAction::ToggleSettings);
    }

    if !tight {
        let dark = matches!(ui.ctx().theme(), egui::Theme::Dark);
        let hint = if dark {
            text.common.light_theme
        } else {
            text.common.dark_theme
        };
        if moon_button(ui, palette, hint).clicked() {
            actions.push(UiAction::SetTheme(if dark {
                ThemeMode::Light
            } else {
                ThemeMode::Dark
            }));
        }
    }

    if !compact {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            let name = match state.now_playing {
                Some(station) => {
                    let name = single_line(&station.name);
                    if name.is_empty() {
                        text.common.untitled.to_owned()
                    } else {
                        truncate(&name, if compact { 18 } else { 26 })
                    }
                }
                None => playback_label(state.language, state.playback),
            };
            ui.label(
                egui::RichText::new(name)
                    .size(13.0)
                    .strong()
                    .color(if is_playing {
                        palette.accent
                    } else {
                        palette.text
                    }),
            );
            let details = match state.now_playing {
                Some(station) => {
                    let place = single_line(
                        &[station.state.as_str(), station.country.as_str()]
                            .iter()
                            .filter(|part| !part.trim().is_empty())
                            .copied()
                            .collect::<Vec<_>>()
                            .join(", "),
                    );
                    if place.is_empty() {
                        truncate(&playback_label(state.language, state.playback), 24)
                    } else {
                        truncate(&place, if compact { 20 } else { 30 })
                    }
                }
                None => text.hud.idle.to_owned(),
            };
            ui.label(egui::RichText::new(details).size(10.5).color(palette.muted));
        });
    }

    if !tight {
        live_badge(ui, palette, is_playing, state.language);
        let texture = state
            .now_playing
            .and_then(|station| station.favicon.as_deref())
            .and_then(|url| state.favicons.texture(url));
        let name = state
            .now_playing
            .map_or("", |station| station.name.as_str());
        avatar(ui, palette, texture, name, 38.0);
        ui.add_space(4.0);
    }

    if close_button(ui, palette, text.common.hide_window_hint).clicked() {
        actions.push(UiAction::Hide);
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hud_texts_match_the_previous_english_wording() {
        let text = i18n::catalog(Language::English);
        assert_eq!(text.hud.subtitle, "Global Internet Radio");
        assert_eq!(text.hud.search_stations, "Search stations…");
        assert_eq!(text.hud.filter_stations, "Filter stations…");
        assert_eq!(text.hud.search_country, "Search country…");
        assert_eq!(text.hud.idle, "Idle");
        assert_eq!(text.common.no_match, "No match");
        assert_eq!(text.common.untitled, "Untitled");
        assert_eq!(text.common.mouse_controls, "Mouse controls");
        assert_eq!(text.common.settings, "Settings");
        assert_eq!(
            text.common.hide_window_hint,
            "Hide window (audio keeps playing)"
        );
        assert_eq!(text.common.show_station_list, "Show station list");
        assert_eq!(text.common.hide_station_list, "Hide station list");
        assert_eq!(text.common.light_theme, "Light theme");
        assert_eq!(text.common.dark_theme, "Dark theme");
    }

    #[test]
    fn language_choice_keeps_every_hud_text() {
        // Русского каталога пока нет: проверяем, что любой разрешённый язык
        // отдаёт непустую строку, а не голый литерал или пустоту.
        for language in [Language::English, Language::Russian] {
            let text = i18n::catalog(language);
            for value in [
                text.hud.subtitle,
                text.hud.search_stations,
                text.hud.filter_stations,
                text.hud.search_country,
                text.hud.idle,
                text.common.no_match,
                text.common.untitled,
                text.common.hide_station_list,
                text.common.show_station_list,
                text.common.mouse_controls,
                text.common.settings,
                text.common.light_theme,
                text.common.dark_theme,
                text.common.hide_window_hint,
            ] {
                assert!(!value.trim().is_empty(), "пустая строка на {language:?}");
            }
        }
    }

    #[test]
    fn content_fade_matches_its_zone_frame() {
        // HUD — нулевой индекс общего stagger, как в `app.rs`.
        for scene in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert_eq!(
                content_alpha(scene),
                animation::alpha(animation::stagger(0, scene)),
                "расхождение со стеклянным каркасом на {scene}"
            );
        }
    }

    #[test]
    fn content_fade_spans_the_whole_entrance() {
        assert_eq!(content_alpha(0.0), 0.0);
        assert_eq!(content_alpha(1.0), 1.0);
        assert!(
            content_alpha(0.1) > 0.0,
            "HUD не должен стартовать с задержкой"
        );

        let mut previous = -1.0;
        for step in 0..=100u8 {
            let value = content_alpha(f32::from(step) / 100.0);
            assert!(value >= previous, "прозрачность идёт назад на шаге {step}");
            previous = value;
        }
    }

    #[test]
    fn garbage_progress_never_leaks_nan_alpha() {
        // Ядро считает неконечный прогресс нулём (fail-closed): NaN не должен
        // попадать ни в альфу, ни в геометрию отрисовки.
        assert_eq!(content_alpha(f32::NAN), 0.0);
        assert_eq!(content_alpha(f32::INFINITY), 0.0);
        assert_eq!(content_alpha(f32::NEG_INFINITY), 0.0);
    }
}
