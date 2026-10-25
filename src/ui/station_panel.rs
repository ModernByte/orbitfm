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
use super::images::FaviconCache;
use super::mode_switch::genre_label;
use super::palette::Palette;
use super::station_filter::{self, CountryFilterState, GenreTag, StationSort};
use super::station_row::{RowContext, station_row, station_tag_chips};
use super::widgets::{Tab, TabIcon, chip, faded_group, search_field, tab_bar};
use super::{UiAction, single_line, truncate};

/// Позиция панели в общем stagger входа Focus-стола: совпадает с индексом,
/// под которым `app.rs` гасит каркас этого стекла.
const SCENE_INDEX: usize = 1;
/// Корень именованного перехода вкладок. `Id::unique` детерминирован, поэтому
/// прогресс переживает кадры и не зависит от порядка аллокаций.
const TAB_KEY: &str = "orbitfm_panel_tab";
/// Число вкладок: столько ключей живёт в `PresenceStore`.
const TAB_COUNT: usize = 3;

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

    let results: &[Station] = state.results;
    let favorites: &[Station] = state.favorites;
    let history: &[Station] = state.history;
    let filter = &mut *state.filter;
    let favicons = &mut *state.favicons;
    let playing_uuid = state.playing_uuid;
    let language = state.language;
    let text = i18n::catalog(language);
    let tab = state.tab;
    let shown = content_alpha(motion.scene, motion.panel);

    palette.panel(16).show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        // Содержимое входит синхронно со стеклом панели и отдельно от неё не
        // смещается: сдвиг каркаса делает `app.rs`, хиттестcontent остаётся на
        // исходных прямоугольниках (ADR-0032 §7).
        faded_group(ui, shown, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(text.panel.stations).size(16.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button("×")
                        .on_hover_text(text.common.hide_station_list)
                        .clicked()
                    {
                        actions.push(UiAction::TogglePanel);
                    }
                });
            });

            if let Some(country) = state.country {
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
            } else if let Some(genre) = state.genre {
                // Известные теги подписывает `mode_switch`, здесь локализуется префикс.
                let tag = genre_label(language, genre);
                ui.label(
                    egui::RichText::new(i18n::fill(text.panel.genre_prefix, &[("genre", &tag)]))
                        .size(12.0)
                        .color(palette.accent)
                        .strong(),
                );
            }
            if state.markers_total > 0 {
                let count = state.markers_total.to_string();
                ui.label(
                    egui::RichText::new(i18n::fill(text.panel.on_map, &[("count", &count)]))
                        .size(11.0)
                        .color(palette.muted),
                );
            }
            if state.pending {
                ui.label(egui::RichText::new(text.panel.searching).color(palette.muted));
            }
            if let Some(error) = state.error {
                ui.label(egui::RichText::new(error).color(palette.error));
            } else if state.empty && !state.pending {
                ui.label(egui::RichText::new(text.panel.no_stations).color(palette.muted));
            }

            let has_library = !favorites.is_empty() || !history.is_empty();
            if results.is_empty() && !has_library {
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(text.panel.intro)
                        .size(12.0)
                        .color(palette.muted),
                );
                return;
            }

            let context_mode = state.country.is_some() || state.genre.is_some();
            let filtered = if context_mode {
                station_filter::filtered_results(results, filter)
            } else {
                results.iter().collect()
            };

            ui.add_space(6.0);
            let shown_count = filtered.len().to_string();
            let total_count = results.len().to_string();
            let results_label = if context_mode && filter.is_active() {
                // «Results (3 of 10)» — два существующих поля каталога без новой строки.
                let of = i18n::fill(
                    text.panel.results_of,
                    &[("count", &shown_count), ("total", &total_count)],
                );
                format!("{} ({})", text.panel.results, of)
            } else {
                i18n::fill(text.panel.results_count, &[("count", &shown_count)])
            };
            let labels = [
                results_label,
                i18n::fill(
                    text.panel.favorites_count,
                    &[("count", &favorites.len().to_string())],
                ),
                i18n::fill(
                    text.panel.history_count,
                    &[("count", &history.len().to_string())],
                ),
            ];
            let tabs = [
                Tab {
                    icon: TabIcon::Results,
                    label: &labels[0],
                },
                Tab {
                    icon: TabIcon::Star,
                    label: &labels[1],
                },
                Tab {
                    icon: TabIcon::Clock,
                    label: &labels[2],
                },
            ];
            // Прогресс вкладки = «эта вкладка открыта». Опрашиваем все три ключа
            // ровно один раз за кадр (`PresenceStore` не переносит повторного опроса
            // одного Id, ADR-0032 §5): тогда уходящая вкладка догорает до нуля и
            // повторный возврат снова проигрывает вход, а не показывает список скачком.
            let selected_before = tab_index(*tab);
            let mut tab_progress = [0.0; TAB_COUNT];
            for (index, progress) in tab_progress.iter_mut().enumerate() {
                *progress = animation::alpha(motion.keyed.value(
                    ui.ctx(),
                    tab_id(index),
                    index == selected_before,
                    animation::TAB,
                    motion.reduce,
                ));
            }
            // Индикатор и подпись открытой вкладки идут одним прогрессом: подчёркивание
            // отрастает, пока список делает slide-in.
            let indicator = tab_progress[selected_before];
            if let Some(clicked) = tab_bar(ui, palette, &tabs, selected_before, indicator) {
                *tab = tab_from_index(clicked);
            }
            ui.separator();

            // Вкладка, выбранная этим кликом, начинает slide-in с нуля, а не
            // дозавершает переход соседней.
            let selected = tab_index(*tab);
            let tab_progress = if selected == selected_before {
                tab_progress[selected]
            } else {
                0.0
            };

            if context_mode && *tab == LibraryTab::Results && !results.is_empty() {
                draw_local_filters(ui, palette, state.country, filter, language);
                ui.add_space(4.0);
            }

            let compact = filter.compact_rows;
            egui::ScrollArea::vertical()
                .id_salt("station_panel_scroll")
                .auto_shrink([false, false])
                .max_height(ui.available_height())
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    // Slide относится только к отрисовке: скролл-область и её
                    // id остаются за трансформацией, иначе смещение сбросило бы
                    // прокрутку списка.
                    slide_body(ui, tab_offset(tab_progress), |ui| match *tab {
                        LibraryTab::Results => {
                            if results.is_empty() {
                                let message = if state.empty {
                                    text.panel.no_stations
                                } else {
                                    text.panel.pick_country_or_genre
                                };
                                ui.label(egui::RichText::new(message).color(palette.muted));
                            } else if filtered.is_empty() {
                                ui.add_space(8.0);
                                ui.label(
                                    egui::RichText::new(text.panel.quiet_sector)
                                        .size(13.0)
                                        .color(palette.muted),
                                );
                                ui.label(
                                    egui::RichText::new(text.panel.no_filter_match)
                                        .size(11.0)
                                        .color(palette.muted),
                                );
                                if ui.button(text.panel.reset_filters).clicked() {
                                    filter.reset();
                                }
                            } else if context_mode {
                                for (index, station) in filtered.iter().enumerate() {
                                    let mut row = RowContext {
                                        playing: playing_uuid == Some(&station.stationuuid),
                                        compact,
                                        favorites,
                                        language,
                                        favicons: &mut *favicons,
                                        previous: index
                                            .checked_sub(1)
                                            .map(|item| filtered[item].stationuuid.as_str()),
                                        next: filtered
                                            .get(index + 1)
                                            .map(|item| item.stationuuid.as_str()),
                                    };
                                    station_row(ui, palette, station, &mut row, &mut actions);
                                    if !compact {
                                        ui.horizontal(|ui| {
                                            ui.add_space(46.0);
                                            station_tag_chips(ui, palette, station, &mut actions);
                                        });
                                    }
                                }
                            } else {
                                for (index, station) in filtered.iter().enumerate() {
                                    let mut row = RowContext {
                                        playing: playing_uuid == Some(&station.stationuuid),
                                        compact,
                                        favorites,
                                        language,
                                        favicons: &mut *favicons,
                                        previous: index
                                            .checked_sub(1)
                                            .map(|item| filtered[item].stationuuid.as_str()),
                                        next: filtered
                                            .get(index + 1)
                                            .map(|item| item.stationuuid.as_str()),
                                    };
                                    station_row(ui, palette, station, &mut row, &mut actions);
                                }
                            }
                        }
                        LibraryTab::Favorites => {
                            if favorites.is_empty() {
                                ui.label(
                                    egui::RichText::new(text.panel.favorites_empty)
                                        .color(palette.muted),
                                );
                            } else {
                                for (index, station) in favorites.iter().enumerate() {
                                    let mut row = RowContext {
                                        playing: playing_uuid == Some(&station.stationuuid),
                                        compact,
                                        favorites,
                                        language,
                                        favicons: &mut *favicons,
                                        previous: index
                                            .checked_sub(1)
                                            .map(|item| favorites[item].stationuuid.as_str()),
                                        next: favorites
                                            .get(index + 1)
                                            .map(|item| item.stationuuid.as_str()),
                                    };
                                    station_row(ui, palette, station, &mut row, &mut actions);
                                }
                            }
                        }
                        LibraryTab::History => {
                            if history.is_empty() {
                                ui.label(
                                    egui::RichText::new(text.panel.history_empty)
                                        .color(palette.muted),
                                );
                            } else {
                                for (index, station) in history.iter().enumerate() {
                                    let mut row = RowContext {
                                        playing: playing_uuid == Some(&station.stationuuid),
                                        compact,
                                        favorites,
                                        language,
                                        favicons: &mut *favicons,
                                        previous: index
                                            .checked_sub(1)
                                            .map(|item| history[item].stationuuid.as_str()),
                                        next: history
                                            .get(index + 1)
                                            .map(|item| item.stationuuid.as_str()),
                                    };
                                    station_row(ui, palette, station, &mut row, &mut actions);
                                }
                                ui.add_space(6.0);
                                if ui.button(text.panel.clear_history).clicked() {
                                    actions.push(UiAction::ClearHistory);
                                }
                            }
                        }
                    });
                });
        });
    });

    actions
}

/// Альфа содержимого панели: вход стола по индексу [`SCENE_INDEX`] умножается на
/// собственный прогресс стеклянной панели (`motion.panel`), ровно как это делает
/// `app.rs` для каркаса.
fn content_alpha(scene: f32, panel: f32) -> f32 {
    animation::alpha(animation::stagger(SCENE_INDEX, scene) * panel)
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

/// Локальные фильтры выбранной страны: поиск, жанровые чипы, сортировка, плотность.
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

    ui.horizontal_wrapped(|ui| {
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

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(text.panel.sort_by)
                .size(11.0)
                .color(palette.muted),
        );
        egui::ComboBox::from_id_salt("country_station_sort")
            .selected_text(filter.sort.label(language))
            .width(130.0)
            .show_ui(ui, |ui| {
                for sort in StationSort::ALL {
                    ui.selectable_value(&mut filter.sort, sort, sort.label(language));
                }
            });
        if chip(ui, palette, text.panel.compact, filter.compact_rows).clicked() {
            filter.compact_rows = !filter.compact_rows;
        }
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
}
