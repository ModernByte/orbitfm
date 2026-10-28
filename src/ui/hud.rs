//! Верхний HUD: логотип, режимы поиска, поле поиска и кнопки окна. Станция и
//! её обложка в HUD не рисуются — для них есть transport и панель списка, а
//! в шапке остаётся только индикатор эфира «● LIVE».
//!
//! Из `FrameMotion` HUD берёт только вход стола (ADR-0032 §4): каркас стекла
//! гасит `app.rs`, здесь гаснет содержимое — текст, иконки и поле поиска.
//! Своих циклов и запросов кадров у HUD нет.

use eframe::egui;

use crate::audio::PlaybackState;
use crate::geo::{Country, GeoService};
use crate::i18n;
use crate::render::VisualizerModel;
use crate::session::WindowMode;
use crate::settings::{Language, ThemeMode};

use super::animation;
use super::icons::{
    close_button, expand_button, gear_button, help_button, moon_button, panel_button,
    roulette_button,
};
use super::mode_switch::genre_label;
use super::palette::Palette;
use super::widgets::{
    NARROW_VIZ_BARS, NARROW_VIZ_SIZE, bucket_max, faded_group, live_badge, logo_mark, search_field,
    viz_bars, volume_slider_width,
};
use super::{SearchMode, UiAction, country_label, mode_switch, single_line, truncate};

const TIGHT_HUD_WIDTH: f32 = 840.0;
const COUNTRY_SUGGESTIONS: usize = 6;
/// Ниже этой ширины строкой ввода нельзя пользоваться.
const MIN_SEARCH_WIDTH: f32 = 120.0;
/// Иконочная кнопка (`icons::SIDE`) и бейдж «● LIVE» в шапке.
const ICON_BUTTON: f32 = 30.0;
const LIVE_BADGE_SIZE: f32 = 52.0;
/// Добавочный зазор между «×» и остальной группой кнопок.
const CLOSE_GAP: f32 = 4.0;
/// Высота строки бренда в шапке панели. Строку нельзя отдавать на растяжение
/// вертикальному layout: центрирование по перекрёстной оси тогда считает от
/// всей высоты шапки и уводит поиск с режимами за её нижнюю границу.
const PANEL_BRAND_ROW: f32 = 34.0;
/// Громкость в шапке панели: штатная ширина и минимум, ниже которого с полосой
/// нельзя работать. Правый край строки режимов отдаёт ей ровно остаток после
/// визуализатора, поэтому полоса не залезает на pill-режимы.
const PANEL_VOLUME_WIDTH: f32 = 96.0;
const PANEL_VOLUME_MIN: f32 = 56.0;
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
    let tight = width < TIGHT_HUD_WIDTH;
    let spacing = ui.spacing().item_spacing.x;
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

                let search_width = search_width(ui.available_width(), tight, spacing);
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
                    actions.extend(right_side(ui, palette, &mut state, is_playing, tight));
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

/// Несгибаемая ширина правого блока: «×», кнопки окна, тема и бейдж «● LIVE»
/// с промежутками между ними. Числа берутся из тех же констант, которыми
/// контролы аллоцируются в [`right_side`], поэтому резерв поля поиска совпадает
/// с блоком без ручного дублирования цифр.
fn right_block_width(tight: bool, spacing: f32) -> f32 {
    // «×», панель, «?» и шестерёка рисуются при любой плотности.
    let mut controls = 4.0f32;
    let mut width = ICON_BUTTON * controls + CLOSE_GAP;
    if !tight {
        // Тема и бейдж эфира.
        width += ICON_BUTTON + LIVE_BADGE_SIZE;
        controls += 2.0;
    }
    width + (controls - 1.0) * spacing
}

/// Ширина поля поиска: весь остаток строки после правого блока. Поле — главный
/// контрол шапки, поэтому оно тянется до контролов, а не живёт в номинальных
/// 240 px; уступает оно только когда места физически меньше [`MIN_SEARCH_WIDTH`].
///
/// `Layout::right_to_left` не переносит содержимое на новую строку
/// (`Layout::main_wrap == false`), поэтому единственный способ не дать
/// контролам уехать поверх pill — вычесть их резерв заранее.
fn search_width(available: f32, tight: bool, spacing: f32) -> f32 {
    (available - right_block_width(tight, spacing) - spacing).max(MIN_SEARCH_WIDTH)
}

fn playing(state: &HudState<'_>) -> bool {
    is_live(state.playback)
}

/// Эфир идёт: проигрывание, буферизация или реконнект.
fn is_live(playback: &PlaybackState) -> bool {
    matches!(
        playback,
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
    pub playback: &'a PlaybackState,
    pub volume: f64,
    /// Модель стола: шапка панели прореживает её до 16 полос (ADR-0022).
    pub visualizer: &'a VisualizerModel,
    pub visualizer_enabled: bool,
    /// Разрешённый язык интерфейса (ADR-0030).
    pub language: Language,
}

/// Шапка выпадающей панели: бренд, поиск и режим поиска. В отличие от HUD
/// стола здесь нет now-playing и раскладок: панель живёт под бар-виджетом и
/// открывается ради станций.
#[allow(clippy::needless_pass_by_value)] // пучок аргументов с `&mut`-полями
pub(crate) fn panel_header(
    ui: &mut egui::Ui,
    palette: &Palette,
    state: PanelHeaderState<'_>,
) -> Vec<UiAction> {
    let mut actions = Vec::new();
    let text = i18n::catalog(state.language);
    let is_playing = is_live(state.playback);
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
                                    // «Открыть основной стол»: панель виджета не
                                    // несёт ни глобуса, ни help, ни легенды
                                    // (ADR-0033), и это единственный вход в Focus.
                                    if expand_button(ui, palette, text.common.open_focus).clicked()
                                    {
                                        actions.push(UiAction::SetWindowMode(WindowMode::Focus));
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

                ui.horizontal(|ui| {
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
                    // Правый край строки режимов: 16-полосный визуализатор и
                    // короткая громкость. `Layout::right_to_left` не переносит
                    // содержимое на новую строку, поэтому полоса берёт ровно
                    // остаток после режимов и уступает, а не уезжает за левый край.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if state.visualizer_enabled {
                            let (slot, _) = ui.allocate_exact_size(
                                egui::Vec2::from(NARROW_VIZ_SIZE),
                                egui::Sense::hover(),
                            );
                            let levels = bucket_max::<NARROW_VIZ_BARS>(state.visualizer.levels());
                            let peaks = bucket_max::<NARROW_VIZ_BARS>(state.visualizer.peaks());
                            viz_bars(ui, palette, slot, &levels, &peaks);
                            if is_playing {
                                // Панель живёт при выключенной реактивности
                                // движения (ADR-0021), поэтому лента сама просит
                                // свои кадры.
                                ui.ctx()
                                    .request_repaint_after(std::time::Duration::from_millis(33));
                            }
                        }

                        let width = ui.available_width().min(PANEL_VOLUME_WIDTH);
                        if width >= PANEL_VOLUME_MIN {
                            let mut volume = state.volume;
                            let slider = volume_slider_width(
                                ui,
                                palette,
                                &mut volume,
                                width,
                                state.language,
                            );
                            if slider.changed && !slider.committed {
                                actions.push(UiAction::PreviewVolume(volume));
                            }
                            if slider.committed {
                                actions.push(UiAction::CommitVolume(volume));
                            }
                        }
                    });
                });
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
    tight: bool,
) -> Vec<UiAction> {
    let mut actions = Vec::new();
    let text = i18n::catalog(state.language);

    // `Layout::right_to_left` ставит первым добавленным самого правого, поэтому
    // «×» идёт до остальной группы: закрытие окна сидит в правом верхнем углу,
    // как у любой кнопки закрытия, а не вплотную к полю поиска.
    if close_button(ui, palette, text.common.hide_window_hint).clicked() {
        actions.push(UiAction::Hide);
    }
    ui.add_space(CLOSE_GAP);

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

    // Шестерёка доступна и в узком окне: модалка — единственный вход в настройки.
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
        // Индикатор эфира — единственный now-playing-сигнал шапки: имя станции,
        // её логотип и подпись живут в transport и панели списка.
        live_badge(ui, palette, is_playing, state.language);
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
        assert_eq!(text.common.no_match, "No match");
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
        // Шапка панели: вход в Focus и контекст над поиском.
        assert_eq!(text.common.open_focus, "Open Focus layout");
        assert_eq!(text.panel.country_prefix, "Country: {name}");
        assert_eq!(text.panel.genre_prefix, "Genre: {genre}");
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
                text.common.no_match,
                text.common.hide_station_list,
                text.common.show_station_list,
                text.common.mouse_controls,
                text.common.settings,
                text.common.light_theme,
                text.common.dark_theme,
                text.common.hide_window_hint,
                text.common.open_focus,
                text.panel.country_prefix,
                text.panel.genre_prefix,
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

    /// На сколько пикселей правый блок вылез за левую границу строки — то
    /// есть залез на pill поиска. Отрицательное значение — запас по ширине.
    ///
    /// `row_width` — ширина строки под стеклом (`Palette::glass` снимает по
    /// 16 px с каждой стороны), левый блок — 341 px живого кадра RU 1100×800.
    fn right_block_spill(row_width: f32, tight: bool) -> f32 {
        let geo = GeoService::from_embedded().expect("встроенный датасет");
        let palette = Palette::for_ui(egui::Theme::Dark, crate::settings::AccentColor::Blue);
        let mut spill = 0.0f32;
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            time: Some(1.0),
            predicted_dt: 1.0 / 60.0,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(30.0, 24.0),
                egui::vec2(row_width, 38.0),
            )),
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            ui.horizontal(|ui| {
                ui.allocate_exact_size(egui::vec2(341.0, 30.0), egui::Sense::hover());
                let search = search_width(ui.available_width(), tight, ui.spacing().item_spacing.x);
                ui.allocate_exact_size(egui::vec2(search, 30.0), egui::Sense::hover());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let spacing = ui.spacing().item_spacing.x;
                    let bound = ui.max_rect().left();
                    let mut query = String::new();
                    let mut country_query = String::new();
                    let mut genre_query = String::new();
                    let playback = PlaybackState::Playing;
                    let mut state = HudState {
                        query: &mut query,
                        country_query: &mut country_query,
                        genre_query: &mut genre_query,
                        mode: SearchMode::Country,
                        selected_genre: None,
                        context_active: false,
                        geo: &geo,
                        playback: &playback,
                        panel_open: false,
                        help_open: false,
                        language: Language::Russian,
                    };
                    right_side(ui, &palette, &mut state, true, tight);
                    spill = bound - (ui.cursor().max.x + spacing);
                });
            });
        });
        output.drop_without_applying_deltas();
        spill
    }

    #[test]
    fn right_block_never_crosses_the_search_field() {
        for (row_width, tight) in [
            (1040.0, false), // 1100 px — полная раскладка
            (940.0, false),  // ниже `COMPACT_HUD_WIDTH`, still wide controls
            (780.0, true),   // порог `TIGHT_HUD_WIDTH`
        ] {
            let spill = right_block_spill(row_width, tight);
            assert!(
                spill <= 0.5,
                "правый блок вылез на pill на {row_width} px: {spill} px"
            );
        }
    }

    /// Блок контролов занимает ровно [`right_block_width`] от правого края
    /// строки: резерв поля поиска и фактическая раскладка считают одни и те
    /// же аллокации, поэтому «×» в правом углу не двигает остальные кнопки.
    #[test]
    fn right_block_matches_its_reserved_width() {
        for tight in [false, true] {
            let geo = GeoService::from_embedded().expect("встроенный датасет");
            let palette = Palette::for_ui(egui::Theme::Dark, crate::settings::AccentColor::Blue);
            let mut width = 0.0f32;
            let ctx = egui::Context::default();
            let input = egui::RawInput {
                time: Some(1.0),
                predicted_dt: 1.0 / 60.0,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(1040.0, 38.0),
                )),
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let spacing = ui.spacing().item_spacing.x;
                    let right_edge = ui.max_rect().right();
                    let mut query = String::new();
                    let mut country_query = String::new();
                    let mut genre_query = String::new();
                    let playback = PlaybackState::Stopped;
                    let mut state = HudState {
                        query: &mut query,
                        country_query: &mut country_query,
                        genre_query: &mut genre_query,
                        mode: SearchMode::Country,
                        selected_genre: None,
                        context_active: false,
                        geo: &geo,
                        playback: &playback,
                        panel_open: false,
                        help_open: false,
                        language: Language::Russian,
                    };
                    right_side(ui, &palette, &mut state, false, tight);
                    width = right_edge - (ui.cursor().max.x + spacing);
                    let reserved = right_block_width(tight, spacing);
                    assert!(
                        (width - reserved).abs() < 0.01,
                        "блок {width} px против резерва {reserved} px"
                    );
                });
            });
            output.drop_without_applying_deltas();
        }
    }

    #[test]
    fn search_field_stretches_to_the_row_remainder() {
        let spacing = 8.0;
        // 1100 px: после левого блока остаётся 699 px; поиск забирает всё, что
        // не занято правым блоком.
        let wide = 699.0;
        let stretched = search_width(wide, false, spacing);
        assert_eq!(
            stretched,
            wide - right_block_width(false, spacing) - spacing
        );
        assert!(
            stretched > 240.0,
            "поле поиска обязано быть шире прежнего номинала 240 px: {stretched}"
        );

        // Узкая шапка: поиск уступает правому блоку, но не право на жизнь.
        let narrow = 400.0;
        let yielded = search_width(narrow, false, spacing);
        assert!(
            (MIN_SEARCH_WIDTH..narrow).contains(&yielded),
            "поиск обязан уступить, получил {yielded}"
        );
        assert!(
            yielded + right_block_width(false, spacing) <= narrow,
            "правый блок не влезает в остаток строки"
        );

        // В узкой шапке справа меньше контролов — поиску достаётся больше.
        assert!(search_width(wide, true, spacing) > stretched);

        // И никогда не уходит в отрицательную ширину.
        assert_eq!(search_width(0.0, false, spacing), MIN_SEARCH_WIDTH);
    }
}
