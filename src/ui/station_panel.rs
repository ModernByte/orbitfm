//! Правая плавающая панель списка станций (эскиз «Orbital Desk»).
//!
//! Движение панели: каркас стекла гасит и смещает `app.rs`, здесь гаснет
//! содержимое (тот же stagger-индекс), а смена вкладки Results/Favorites/History
//! идёт отдельным переходом [`animation::TAB`] — fade + slide по
//! [`animation::SLIDE_X`] (ADR-0032 §4).

use eframe::egui;

use crate::geo::Country;
use crate::i18n;
use crate::services::Station;
use crate::session::LibraryTab;
use crate::settings::Language;

use super::animation;
use super::icons;
use super::images::FaviconCache;
use super::mode_switch::genre_label;
use super::palette::Palette;
use super::station_filter::{self, CountryFilterState, GenreTag, StationSort};
use super::station_row::{RowContext, station_row, station_tag_chips};
use super::widgets::{
    EmptyMark, EmptyState, chip, empty_state, error_card, faded_group, quiet_button, search_field,
    segmented_tabs, skeleton_rows,
};
use super::{UiAction, single_line, truncate};

/// Корень именованного перехода вкладок. `Id::unique` детерминирован, поэтому
/// прогресс переживает кадры и не зависит от порядка аллокаций.
const TAB_KEY: &str = "orbitfm_panel_tab";
/// Число вкладок: столько ключей живёт в `PresenceStore`.
const TAB_COUNT: usize = 3;
/// Высота закреплённой кнопки «Очистить историю» вместе с отступом.
const CLEAR_HISTORY_HEIGHT: f32 = 38.0;

/// Оформление карточки списка. В Focus панель — самостоятельная карточка со
/// своим заголовком; в выпадающей панели шапку рисует `hud::panel_header`, и
/// список начинается сразу с вкладок (ADR-0033).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PanelChrome {
    Full,
    Embedded,
}

pub(crate) struct PanelState<'a> {
    pub tab: &'a mut LibraryTab,
    pub country: Option<&'a Country>,
    pub genre: Option<&'a str>,
    pub markers_total: usize,
    pub results: &'a [Station],
    pub favorites: &'a [Station],
    pub history: &'a [Station],
    pub pending: bool,
    pub empty: bool,
    pub error: Option<&'a str>,
    pub filter: &'a mut CountryFilterState,
    pub favicons: &'a mut FaviconCache,
    pub playing_uuid: Option<&'a str>,
    /// Альфа содержимого: вход Focus-стола или появление выпадающей панели.
    pub entry: f32,
    pub chrome: PanelChrome,
    /// Разрешённый язык интерфейса (ADR-0030).
    pub language: Language,
}

fn tab_index(tab: LibraryTab) -> usize {
    match tab {
        LibraryTab::Results => 0,
        LibraryTab::Favorites => 1,
        LibraryTab::History => 2,
    }
}

fn tab_from_index(index: usize) -> LibraryTab {
    match index {
        1 => LibraryTab::Favorites,
        2 => LibraryTab::History,
        _ => LibraryTab::Results,
    }
}

pub(crate) fn show(
    ui: &mut egui::Ui,
    palette: &Palette,
    motion: &mut animation::FrameMotion<'_>,
    state: PanelState<'_>,
) -> Vec<UiAction> {
    let mut actions = Vec::new();

    let PanelState {
        tab,
        country,
        genre,
        markers_total,
        results,
        favorites,
        history,
        pending,
        empty,
        error,
        filter,
        favicons,
        playing_uuid,
        entry: shown,
        chrome,
        language,
    } = state;
    let embedded = chrome == PanelChrome::Embedded;
    let reduce = motion.reduce;
    let text = i18n::catalog(language);

    palette.panel(16).show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        // Содержимое входит синхронно со стеклом панели и отдельно от неё не
        // смещается: сдвиг каркаса делает `app.rs`, хиттест остаётся на
        // исходных прямоугольниках (ADR-0032 §7).
        faded_group(ui, shown, |ui| {
            if !embedded {
                draw_header(
                    ui,
                    palette,
                    country,
                    genre,
                    markers_total,
                    language,
                    &mut actions,
                );
            }

            let context_mode = country.is_some() || genre.is_some();
            let filtered: Vec<&Station> = if context_mode {
                station_filter::filtered_results(results, filter)
            } else {
                results.iter().collect()
            };
            let total = match *tab {
                LibraryTab::Results => results.len(),
                LibraryTab::Favorites => favorites.len(),
                LibraryTab::History => history.len(),
            };

            // Вкладки видны всегда — даже на пустом списке: это карта разделов,
            // а не заголовок содержимого. Счётчики переехали в подпись под ними.
            let selected_before = tab_index(*tab);
            let mut tab_progress = [0.0; TAB_COUNT];
            for (index, progress) in tab_progress.iter_mut().enumerate() {
                *progress = animation::alpha(motion.keyed.value(
                    ui.ctx(),
                    tab_id(index),
                    index == selected_before,
                    animation::TAB,
                    reduce,
                ));
            }
            let labels = [text.panel.results, text.panel.favorites, text.panel.history];
            if let Some(clicked) = segmented_tabs(
                ui,
                palette,
                "station_tabs",
                &labels,
                selected_before,
                reduce,
            ) {
                *tab = tab_from_index(clicked);
            }

            // Вкладка, выбранная этим кликом, начинает slide-in с нуля, а не
            // дозавершает переход соседней.
            let selected = tab_index(*tab);
            let tab_progress = if selected == selected_before {
                tab_progress[selected]
            } else {
                0.0
            };
            let compact = filter.compact_rows;

            if total > 0 {
                ui.add_space(4.0);
                draw_meta(
                    ui,
                    palette,
                    *tab,
                    total,
                    filtered.len(),
                    context_mode,
                    filter,
                    language,
                );
            }
            ui.add_space(6.0);

            if context_mode && *tab == LibraryTab::Results && !results.is_empty() {
                draw_local_filters(ui, palette, country, filter, language);
                ui.add_space(6.0);
            }

            let show_clear = *tab == LibraryTab::History && !history.is_empty();
            let reserve = if show_clear {
                CLEAR_HISTORY_HEIGHT
            } else {
                0.0
            };
            let viewport = (ui.available_height() - reserve).max(80.0);
            egui::ScrollArea::vertical()
                .id_salt("station_panel_scroll")
                .auto_shrink([false, false])
                .max_height(viewport)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    // Slide относится только к отрисовке: скролл-область и её
                    // id остаются за трансформацией, иначе смещение сбросило бы
                    // прокрутку списка.
                    slide_body(ui, tab_offset(tab_progress), |ui| match *tab {
                        LibraryTab::Results => {
                            if let Some(error) = error {
                                ui.add_space(4.0);
                                if error_card(
                                    ui,
                                    palette,
                                    text.panel.search_failed,
                                    error,
                                    text.common.retry,
                                ) {
                                    actions.push(UiAction::RetrySearch);
                                }
                            } else if pending {
                                skeleton_rows(
                                    ui,
                                    palette,
                                    compact,
                                    viewport,
                                    text.panel.searching,
                                    reduce,
                                );
                            } else if results.is_empty() {
                                let state = if empty {
                                    EmptyState {
                                        mark: EmptyMark::Search,
                                        title: text.panel.no_stations,
                                        hint: Some(text.panel.intro),
                                        action: None,
                                    }
                                } else {
                                    EmptyState {
                                        mark: EmptyMark::Globe,
                                        title: text.panel.pick_country_or_genre,
                                        hint: Some(text.panel.intro),
                                        action: None,
                                    }
                                };
                                empty_state(ui, palette, state, viewport);
                            } else if filtered.is_empty() {
                                let reset = empty_state(
                                    ui,
                                    palette,
                                    EmptyState {
                                        mark: EmptyMark::Filter,
                                        title: text.panel.quiet_sector,
                                        hint: Some(text.panel.no_filter_match),
                                        action: Some(text.panel.reset_filters),
                                    },
                                    viewport,
                                );
                                if reset {
                                    filter.reset();
                                }
                            } else {
                                draw_rows(
                                    ui,
                                    palette,
                                    &filtered,
                                    playing_uuid,
                                    compact,
                                    context_mode,
                                    favorites,
                                    language,
                                    favicons,
                                    &mut actions,
                                );
                            }
                        }
                        LibraryTab::Favorites => {
                            if favorites.is_empty() {
                                empty_state(
                                    ui,
                                    palette,
                                    EmptyState {
                                        mark: EmptyMark::Star,
                                        title: text.panel.favorites_empty,
                                        hint: Some(text.capsule.favorite_hint),
                                        action: None,
                                    },
                                    viewport,
                                );
                            } else {
                                let rows: Vec<&Station> = favorites.iter().collect();
                                draw_rows(
                                    ui,
                                    palette,
                                    &rows,
                                    playing_uuid,
                                    compact,
                                    false,
                                    favorites,
                                    language,
                                    favicons,
                                    &mut actions,
                                );
                            }
                        }
                        LibraryTab::History => {
                            if history.is_empty() {
                                empty_state(
                                    ui,
                                    palette,
                                    EmptyState {
                                        mark: EmptyMark::Clock,
                                        title: text.panel.history_empty,
                                        hint: None,
                                        action: None,
                                    },
                                    viewport,
                                );
                            } else {
                                let rows: Vec<&Station> = history.iter().collect();
                                draw_rows(
                                    ui,
                                    palette,
                                    &rows,
                                    playing_uuid,
                                    compact,
                                    false,
                                    favorites,
                                    language,
                                    favicons,
                                    &mut actions,
                                );
                            }
                        }
                    });
                });

            // Очистка истории закреплена под списком: длинная история не
            // уводит действие за пределы прокрутки.
            if show_clear {
                ui.add_space(4.0);
                if quiet_button(ui, palette, text.panel.clear_history, palette.error).clicked() {
                    actions.push(UiAction::ClearHistory);
                }
            }
        });
    });

    actions
}

/// Шапка самостоятельной карточки Focus: заголовок, контекст и счётчик карты.
fn draw_header(
    ui: &mut egui::Ui,
    palette: &Palette,
    country: Option<&Country>,
    genre: Option<&str>,
    markers_total: usize,
    language: Language,
    actions: &mut Vec<UiAction>,
) {
    let text = i18n::catalog(language);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(text.panel.stations)
                .size(15.0)
                .strong()
                .color(palette.text),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::close_button(ui, palette, text.common.hide_station_list).clicked() {
                actions.push(UiAction::TogglePanel);
            }
        });
    });

    if let Some(country) = country {
        let name = single_line(&country.name);
        let name = if name.is_empty() {
            country.iso_a2.clone()
        } else {
            name
        };
        let title = i18n::fill(text.panel.country_prefix, &[("name", &name)]);
        ui.label(
            egui::RichText::new(title)
                .size(12.0)
                .color(palette.accent)
                .strong(),
        );
    } else if let Some(genre) = genre {
        // Известные теги подписывает `mode_switch`, здесь локализуется префикс.
        let tag = genre_label(language, genre);
        ui.label(
            egui::RichText::new(i18n::fill(text.panel.genre_prefix, &[("genre", &tag)]))
                .size(12.0)
                .color(palette.accent)
                .strong(),
        );
    }
    if markers_total > 0 {
        let count = markers_total.to_string();
        ui.label(
            egui::RichText::new(i18n::fill(text.panel.on_map, &[("count", &count)]))
                .size(11.0)
                .color(palette.muted),
        );
    }
}

/// Подпись под вкладками: «N из M» при фильтрах, иначе число станций, плюс
/// сортировка и плотность строк. Счётчики в самих вкладках больше не живут.
#[allow(clippy::too_many_arguments)]
fn draw_meta(
    ui: &mut egui::Ui,
    palette: &Palette,
    tab: LibraryTab,
    total: usize,
    shown_count: usize,
    context_mode: bool,
    filter: &mut CountryFilterState,
    language: Language,
) {
    let text = i18n::catalog(language);
    let caption = if tab == LibraryTab::Results && context_mode && filter.is_active() {
        i18n::fill(
            text.panel.results_of,
            &[
                ("count", &shown_count.to_string()),
                ("total", &total.to_string()),
            ],
        )
    } else {
        i18n::fill(text.panel.station_count, &[("count", &total.to_string())])
    };
    let sortable = tab == LibraryTab::Results && context_mode;

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(caption).size(11.0).color(palette.muted));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::density_button(ui, palette, text.panel.compact, filter.compact_rows).clicked()
            {
                filter.compact_rows = !filter.compact_rows;
            }
            if sortable {
                let response = icons::sort_button(ui, palette, text.panel.sort_by);
                egui::Popup::menu(&response).show(|ui| {
                    for sort in StationSort::ALL {
                        let selected = filter.sort == sort;
                        if ui
                            .selectable_label(selected, sort.label(language))
                            .clicked()
                        {
                            filter.sort = sort;
                            ui.close();
                        }
                    }
                });
            }
        });
    });
}

/// Стабильный Id перехода вкладки: три вкладки — три ключа в `PresenceStore`.
fn tab_id(index: usize) -> egui::Id {
    egui::Id::unique((TAB_KEY, index))
}

/// Горизонтальное смещение содержимого вкладки: `SLIDE_X` пикселей слева в
/// начале перехода, ноль в конце. Отрицательное направление совпадает со
/// сдвигом каркаса панели в `app.rs`.
fn tab_offset(progress: f32) -> f32 {
    -animation::slide_x(animation::alpha(progress))
}

/// Сдвиг только что нарисованного содержимого на `shift` пикселей по X.
///
/// Трансформируются формы, добавленные внутри `add`, — аллокации, хиттест и id
/// виджетов не меняются (ADR-0032 §7). В покое (`shift == 0`) обходимся без
/// прохода по списку форм: `Shape::transform` для текста и мешей копирует данные,
/// поэтому гонять его каждый кадр нельзя.
fn slide_body<T>(ui: &mut egui::Ui, shift: f32, add: impl FnOnce(&mut egui::Ui) -> T) -> T {
    if shift == 0.0 {
        return add(ui);
    }
    let layer = ui.layer_id();
    let start = ui.ctx().graphics(|g| {
        g.get(layer)
            .map_or(egui::layers::ShapeIdx(0), |l| l.next_idx())
    });
    let result = add(ui);
    ui.ctx().graphics_mut(|g| {
        if let Some(list) = g.get_mut(layer) {
            let end = list.next_idx();
            list.transform_range(
                start,
                end,
                egui::emath::TSTransform::from_translation(egui::vec2(shift, 0.0)),
            );
        }
    });
    result
}

/// Строки списка с соседями для навигации стрелками. `tag_chips` рисует
/// жанровые чипы под станцией выбранной страны.
#[allow(clippy::too_many_arguments)]
fn draw_rows(
    ui: &mut egui::Ui,
    palette: &Palette,
    stations: &[&Station],
    playing_uuid: Option<&str>,
    compact: bool,
    tag_chips: bool,
    favorites: &[Station],
    language: Language,
    favicons: &mut FaviconCache,
    actions: &mut Vec<UiAction>,
) {
    for (index, station) in stations.iter().enumerate() {
        let mut row = RowContext {
            playing: playing_uuid == Some(&station.stationuuid),
            compact,
            favorites,
            language,
            favicons: &mut *favicons,
            previous: index
                .checked_sub(1)
                .map(|item| stations[item].stationuuid.as_str()),
            next: stations
                .get(index + 1)
                .map(|item| item.stationuuid.as_str()),
        };
        station_row(ui, palette, station, &mut row, actions);
        if tag_chips && !compact {
            ui.horizontal(|ui| {
                ui.add_space(46.0);
                station_tag_chips(ui, palette, station, actions);
            });
        }
    }
}

/// Локальные фильтры выбранной страны: поиск и жанровые чипы одной строкой.
fn draw_local_filters(
    ui: &mut egui::Ui,
    palette: &Palette,
    country: Option<&Country>,
    filter: &mut CountryFilterState,
    language: Language,
) {
    let text = i18n::catalog(language);
    let hint = match country {
        Some(country) => {
            let name = truncate(&single_line(&country.name), 18);
            i18n::fill(text.panel.search_in_country, &[("country", &name)])
        }
        None => text.panel.search_here.to_owned(),
    };
    let output = search_field(
        ui,
        palette,
        egui::Id::unique("orbitfm_country_filter"),
        &hint,
        &mut filter.text,
        ui.available_width(),
    );
    let _ = output;

    ui.add_space(4.0);
    // Чипы всегда в одну строку: перенос ломал ритм панели, а длинный набор
    // жанров прокручивается колесом внутри своей полосы.
    egui::ScrollArea::horizontal()
        .id_salt("orbitfm_country_filter_chips")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let all_selected = filter.genre.is_none();
                if chip(ui, palette, text.panel.all, all_selected).clicked() {
                    filter.genre = None;
                }
                for genre in GenreTag::ALL {
                    let selected = filter.genre == Some(genre);
                    if chip(ui, palette, genre.label(language), selected).clicked() {
                        filter.genre = if selected { None } else { Some(genre) };
                    }
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_roundtrip_index_and_back() {
        for tab in [
            LibraryTab::Results,
            LibraryTab::Favorites,
            LibraryTab::History,
        ] {
            assert_eq!(tab_from_index(tab_index(tab)), tab);
        }
        assert_eq!(tab_from_index(7), LibraryTab::Results);
    }

    #[test]
    fn tab_slide_starts_offset_and_settles_at_zero() {
        assert_eq!(tab_offset(0.0), -animation::SLIDE_X);
        assert_eq!(tab_offset(1.0), 0.0);
        assert_eq!(tab_offset(f32::NAN), -animation::SLIDE_X);
    }
}
