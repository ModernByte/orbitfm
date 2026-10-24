//! Правая плавающая панель списка станций (эскиз «Orbital Desk»).

use eframe::egui;

use crate::geo::Country;
use crate::services::Station;
use crate::session::LibraryTab;

use super::images::FaviconCache;
use super::mode_switch::genre_label;
use super::palette::Palette;
use super::station_filter::{self, CountryFilterState, GenreTag, StationSort};
use super::station_row::{RowContext, station_row, station_tag_chips};
use super::widgets::{Tab, TabIcon, chip, search_field, tab_bar};
use super::{UiAction, single_line, truncate};

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

pub(crate) fn show(ui: &mut egui::Ui, palette: &Palette, state: PanelState<'_>) -> Vec<UiAction> {
    let mut actions = Vec::new();

    let results: &[Station] = state.results;
    let favorites: &[Station] = state.favorites;
    let history: &[Station] = state.history;
    let filter = &mut *state.filter;
    let favicons = &mut *state.favicons;
    let playing_uuid = state.playing_uuid;
    let tab = state.tab;

    palette.panel(16).show(ui, |ui| {
        ui.set_min_size(ui.available_size());

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Stations").size(16.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("×").on_hover_text("Hide station list").clicked() {
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
            let title = format!("Country: {name}");
            ui.label(
                egui::RichText::new(title)
                    .size(12.0)
                    .color(palette.accent)
                    .strong(),
            );
        } else if let Some(genre) = state.genre {
            ui.label(
                egui::RichText::new(format!("Genre: {}", genre_label(genre)))
                    .size(12.0)
                    .color(palette.accent)
                    .strong(),
            );
        }
        if state.markers_total > 0 {
            ui.label(
                egui::RichText::new(format!("On map: {} stations", state.markers_total))
                    .size(11.0)
                    .color(palette.muted),
            );
        }
        if state.pending {
            ui.label(egui::RichText::new("Searching…").color(palette.muted));
        }
        if let Some(error) = state.error {
            ui.label(egui::RichText::new(error).color(palette.error));
        } else if state.empty && !state.pending {
            ui.label(egui::RichText::new("No stations found").color(palette.muted));
        }

        let has_library = !favorites.is_empty() || !history.is_empty();
        if results.is_empty() && !has_library {
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new(
                    "Search a station by name, pick a genre or click a country on the globe.",
                )
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
        let results_label = if context_mode && filter.is_active() {
            format!("Results ({} of {})", filtered.len(), results.len())
        } else {
            format!("Results ({})", filtered.len())
        };
        let labels = [
            results_label,
            format!("Favorites ({})", favorites.len()),
            format!("History ({})", history.len()),
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
        if let Some(clicked) = tab_bar(ui, palette, &tabs, tab_index(*tab)) {
            *tab = tab_from_index(clicked);
        }
        ui.separator();

        if context_mode && *tab == LibraryTab::Results && !results.is_empty() {
            draw_local_filters(ui, palette, state.country, filter);
            ui.add_space(4.0);
        }

        let compact = filter.compact_rows;
        egui::ScrollArea::vertical()
            .id_salt("station_panel_scroll")
            .auto_shrink([false, false])
            .max_height(ui.available_height())
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                match *tab {
                    LibraryTab::Results => {
                        if results.is_empty() {
                            let message = if state.empty {
                                "No stations found"
                            } else {
                                "Pick a country or genre, or search a station by name."
                            };
                            ui.label(egui::RichText::new(message).color(palette.muted));
                        } else if filtered.is_empty() {
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new("Quiet in this sector")
                                    .size(13.0)
                                    .color(palette.muted),
                            );
                            ui.label(
                                egui::RichText::new("No station matches the filters.")
                                    .size(11.0)
                                    .color(palette.muted),
                            );
                            if ui.button("Reset filters").clicked() {
                                filter.reset();
                            }
                        } else if context_mode {
                            for (index, station) in filtered.iter().enumerate() {
                                let mut row = RowContext {
                                    playing: playing_uuid == Some(&station.stationuuid),
                                    compact,
                                    favorites,
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
                                egui::RichText::new("Favorites are empty").color(palette.muted),
                            );
                        } else {
                            for (index, station) in favorites.iter().enumerate() {
                                let mut row = RowContext {
                                    playing: playing_uuid == Some(&station.stationuuid),
                                    compact,
                                    favorites,
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
                            ui.label(egui::RichText::new("History is empty").color(palette.muted));
                        } else {
                            for (index, station) in history.iter().enumerate() {
                                let mut row = RowContext {
                                    playing: playing_uuid == Some(&station.stationuuid),
                                    compact,
                                    favorites,
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
                            if ui.button("Clear history").clicked() {
                                actions.push(UiAction::ClearHistory);
                            }
                        }
                    }
                }
            });
    });

    actions
}

/// Локальные фильтры выбранной страны: поиск, жанровые чипы, сортировка, плотность.
fn draw_local_filters(
    ui: &mut egui::Ui,
    palette: &Palette,
    country: Option<&Country>,
    filter: &mut CountryFilterState,
) {
    let hint = match country {
        Some(country) => format!(
            "Search stations in {}…",
            truncate(&single_line(&country.name), 18)
        ),
        None => "Search stations here…".to_owned(),
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
        if chip(ui, palette, "All", all_selected).clicked() {
            filter.genre = None;
        }
        for genre in GenreTag::ALL {
            let selected = filter.genre == Some(genre);
            if chip(ui, palette, genre.label(), selected).clicked() {
                filter.genre = if selected { None } else { Some(genre) };
            }
        }
    });

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Sort by")
                .size(11.0)
                .color(palette.muted),
        );
        egui::ComboBox::from_id_salt("country_station_sort")
            .selected_text(filter.sort.label())
            .width(130.0)
            .show_ui(ui, |ui| {
                for sort in StationSort::ALL {
                    ui.selectable_value(&mut filter.sort, sort, sort.label());
                }
            });
        if chip(ui, palette, "Compact", filter.compact_rows).clicked() {
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
