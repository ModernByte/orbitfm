//! Правый drawer со списком станций, избранным и историей.

use eframe::egui;

use crate::geo::Country;
use crate::services::Station;

use super::icons;
use super::mode_switch::genre_label;
use super::palette::Palette;
use super::station_filter::{self, CountryFilterState, GenreTag, StationSort};
use super::{UiAction, country_label, single_line, truncate};

/// Активная вкладка списка станций.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum LibraryTab {
    #[default]
    Results,
    Favorites,
    History,
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
}

pub(crate) fn show(ui: &mut egui::Ui, palette: &Palette, state: PanelState<'_>) -> Vec<UiAction> {
    let mut actions = Vec::new();

    palette.glass().show(ui, |ui| {
        ui.set_min_size(ui.available_size());

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Станции").size(18.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("×").on_hover_text("Свернуть список").clicked() {
                    actions.push(UiAction::TogglePanel);
                }
            });
        });

        if let Some(country) = state.country {
            ui.label(
                egui::RichText::new(format!("Страна: {}", country_label(country)))
                    .size(12.0)
                    .color(palette.accent)
                    .strong(),
            );
        } else if let Some(genre) = state.genre {
            ui.label(
                egui::RichText::new(format!("Жанр: {}", genre_label(genre)))
                    .size(12.0)
                    .color(palette.accent)
                    .strong(),
            );
        }
        if state.markers_total > 0 {
            ui.label(
                egui::RichText::new(format!("На карте: {} станций", state.markers_total))
                    .size(11.0)
                    .color(palette.muted),
            );
        }
        if state.pending {
            ui.label(egui::RichText::new("Поиск…").color(palette.muted));
        }
        if let Some(error) = state.error {
            ui.label(egui::RichText::new(error).color(palette.error));
        } else if state.empty && !state.pending {
            ui.label(egui::RichText::new("Станции не найдены").color(palette.muted));
        }

        let has_library = !state.favorites.is_empty() || !state.history.is_empty();
        if state.results.is_empty() && !has_library {
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new(
                    "Найдите станцию по имени, выберите жанр или кликните по стране на глобусе.",
                )
                .size(12.0)
                .color(palette.muted),
            );
            return;
        }

        let context_mode = state.country.is_some() || state.genre.is_some();
        let filtered = if context_mode {
            station_filter::filtered_results(state.results, state.filter)
        } else {
            state.results.iter().collect()
        };

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            let results_label = if context_mode && state.filter.is_active() {
                format!("Результаты ({} из {})", filtered.len(), state.results.len())
            } else {
                format!("Результаты ({})", filtered.len())
            };
            ui.selectable_value(state.tab, LibraryTab::Results, results_label);
            ui.selectable_value(
                state.tab,
                LibraryTab::Favorites,
                format!("Избранное ({})", state.favorites.len()),
            );
            ui.selectable_value(
                state.tab,
                LibraryTab::History,
                format!("История ({})", state.history.len()),
            );
        });
        ui.separator();

        if context_mode && *state.tab == LibraryTab::Results && !state.results.is_empty() {
            draw_local_filters(ui, palette, state.filter);
            ui.add_space(4.0);
        }

        egui::ScrollArea::vertical()
            .id_salt("station_panel_scroll")
            .auto_shrink([false, false])
            .max_height(ui.available_height())
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                match *state.tab {
                    LibraryTab::Results => {
                        if state.results.is_empty() {
                            let message = if state.empty {
                                "Станции не найдены"
                            } else {
                                "Выберите страну или жанр, либо найдите станцию по имени."
                            };
                            ui.label(egui::RichText::new(message).color(palette.muted));
                        } else if filtered.is_empty() {
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new("В этом секторе пока тихо")
                                    .size(13.0)
                                    .color(palette.muted),
                            );
                            ui.label(
                                egui::RichText::new("Под фильтры не попала ни одна станция.")
                                    .size(11.0)
                                    .color(palette.muted),
                            );
                            if ui.button("Сбросить фильтры").clicked() {
                                state.filter.reset();
                            }
                        } else if context_mode {
                            for station in &filtered {
                                station_card(ui, palette, station, state.favorites, &mut actions);
                            }
                        } else {
                            for station in state.results {
                                station_row(ui, palette, station, state.favorites, &mut actions);
                            }
                        }
                    }
                    LibraryTab::Favorites => {
                        if state.favorites.is_empty() {
                            ui.label(
                                egui::RichText::new("В избранном пока пусто").color(palette.muted),
                            );
                        } else {
                            for station in state.favorites {
                                station_row(ui, palette, station, state.favorites, &mut actions);
                            }
                        }
                    }
                    LibraryTab::History => {
                        if state.history.is_empty() {
                            ui.label(
                                egui::RichText::new("История пока пуста").color(palette.muted),
                            );
                        } else {
                            for station in state.history {
                                station_row(ui, palette, station, state.favorites, &mut actions);
                            }
                            ui.add_space(6.0);
                            if ui.button("Очистить историю").clicked() {
                                actions.push(UiAction::ClearHistory);
                            }
                        }
                    }
                }
            });
    });

    actions
}

fn draw_local_filters(ui: &mut egui::Ui, palette: &Palette, filter: &mut CountryFilterState) {
    ui.add(
        egui::TextEdit::singleline(&mut filter.text)
            .hint_text("Имя, жанр, страна или штат")
            .desired_width(f32::INFINITY),
    );
    ui.horizontal_wrapped(|ui| {
        if ui.selectable_label(filter.genre.is_none(), "Все").clicked() {
            filter.genre = None;
        }
        for genre in GenreTag::ALL {
            let selected = filter.genre == Some(genre);
            if ui.selectable_label(selected, genre.label()).clicked() {
                filter.genre = if selected { None } else { Some(genre) };
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Сортировка")
                .size(11.0)
                .color(palette.muted),
        );
        egui::ComboBox::from_id_salt("country_station_sort")
            .selected_text(filter.sort.label())
            .width(140.0)
            .show_ui(ui, |ui| {
                for sort in StationSort::ALL {
                    ui.selectable_value(&mut filter.sort, sort, sort.label());
                }
            });
    });
}

fn station_card(
    ui: &mut egui::Ui,
    palette: &Palette,
    station: &Station,
    favorites: &[Station],
    actions: &mut Vec<UiAction>,
) {
    let favorite = favorites
        .iter()
        .any(|item| item.stationuuid == station.stationuuid);
    let (hint, color) = if favorite {
        ("Убрать из избранного", palette.accent)
    } else {
        ("Добавить в избранное", palette.muted)
    };

    egui::Frame::default()
        .fill(palette.subtle_fill)
        .stroke(egui::Stroke::new(1.0, palette.card_stroke))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if icons::star_button(ui, favorite, color, hint).clicked() {
                    actions.push(UiAction::ToggleFavorite(station.clone()));
                }
                if ui
                    .add(egui::Button::selectable(false, card_name(station)).truncate())
                    .on_hover_text(single_line(&station.name))
                    .clicked()
                {
                    actions.push(UiAction::Play(station.clone()));
                }
            });

            let meta = station_meta(station);
            if !meta.is_empty() {
                ui.label(egui::RichText::new(meta).size(10.5).color(palette.muted));
            }
            let extra = station_extra(station);
            if !extra.is_empty() {
                ui.label(egui::RichText::new(extra).size(10.5).color(palette.muted));
            }
        });
    ui.add_space(6.0);
}

fn card_name(station: &Station) -> String {
    let name = single_line(&station.name);
    if name.is_empty() {
        "Без названия".to_owned()
    } else {
        truncate(&name, 34)
    }
}

fn station_meta(station: &Station) -> String {
    let mut parts = Vec::new();

    let state_name = single_line(&station.state);
    let country = single_line(&station.country);
    let location = match (state_name.is_empty(), country.is_empty()) {
        (false, false) => format!("{state_name}, {country}"),
        (false, true) => state_name,
        (true, false) => country,
        (true, true) => single_line(&station.countrycode),
    };
    if !location.is_empty() {
        parts.push(location);
    }
    if station.bitrate > 0 {
        parts.push(format!("{} kbps", station.bitrate));
    }
    let codec = single_line(&station.codec);
    if !codec.is_empty() {
        parts.push(codec);
    }

    parts.join(" · ")
}

fn station_extra(station: &Station) -> String {
    let mut parts = Vec::new();

    let tags = single_line(&station.tags);
    if !tags.is_empty() {
        parts.push(truncate(&tags, 56));
    }
    if station.votes > 0 {
        parts.push(format!("голосов: {}", station.votes));
    }

    parts.join(" · ")
}

fn station_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    station: &Station,
    favorites: &[Station],
    actions: &mut Vec<UiAction>,
) {
    let favorite = favorites
        .iter()
        .any(|item| item.stationuuid == station.stationuuid);
    let (hint, color) = if favorite {
        ("Убрать из избранного", palette.accent)
    } else {
        ("Добавить в избранное", palette.muted)
    };

    ui.horizontal(|ui| {
        if icons::star_button(ui, favorite, color, hint).clicked() {
            actions.push(UiAction::ToggleFavorite(station.clone()));
        }
        if ui
            .add(egui::Button::selectable(false, station_label(station)).truncate())
            .on_hover_text(single_line(&station.name))
            .clicked()
        {
            actions.push(UiAction::Play(station.clone()));
        }
    });
}

pub(crate) fn station_label(station: &Station) -> String {
    let name = single_line(&station.name);
    let name = if name.is_empty() {
        "Без названия".to_owned()
    } else {
        truncate(&name, 42)
    };

    let mut details = Vec::new();
    let country = station.countrycode.trim();
    if !country.is_empty() {
        details.push(country.to_owned());
    }
    let codec = station.codec.trim();
    if !codec.is_empty() {
        details.push(codec.to_owned());
    }
    if station.bitrate > 0 {
        details.push(format!("{} kbps", station.bitrate));
    }

    if details.is_empty() {
        name
    } else {
        format!("{name} — {}", details.join(" · "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_station() -> Station {
        Station {
            stationuuid: "c685c086-b716-4edd-b313-d4a7baf1084c".to_owned(),
            name: "Test Radio".to_owned(),
            url: "http://example.com/live".to_owned(),
            url_resolved: "https://example.com/live".to_owned(),
            homepage: None,
            favicon: None,
            tags: "jazz".to_owned(),
            countrycode: "US".to_owned(),
            language: "english".to_owned(),
            votes: 120,
            country: "United States".to_owned(),
            state: "New York".to_owned(),
            bitrate: 128,
            codec: "MP3".to_owned(),
            geo_lat: None,
            geo_long: None,
        }
    }

    #[test]
    fn station_label_includes_country_codec_and_bitrate() {
        assert_eq!(
            station_label(&sample_station()),
            "Test Radio — US · MP3 · 128 kbps"
        );
    }

    #[test]
    fn station_label_falls_back_for_empty_name() {
        let mut station = sample_station();
        station.name = "   ".to_owned();
        assert_eq!(
            station_label(&station),
            "Без названия — US · MP3 · 128 kbps"
        );
    }

    #[test]
    fn card_meta_shows_state_country_bitrate_and_codec() {
        assert_eq!(
            station_meta(&sample_station()),
            "New York, United States · 128 kbps · MP3"
        );

        let mut station = sample_station();
        station.state.clear();
        station.country.clear();
        assert_eq!(station_meta(&station), "US · 128 kbps · MP3");
    }

    #[test]
    fn card_extra_shows_tags_and_votes() {
        assert_eq!(station_extra(&sample_station()), "jazz · голосов: 120");

        let mut station = sample_station();
        station.tags.clear();
        station.votes = 0;
        assert_eq!(station_extra(&station), "");
    }

    #[test]
    fn card_name_is_single_line_and_truncated() {
        let mut station = sample_station();
        station.name = "Very Long Radio Station Name With Many Words In It".to_owned();
        let name = card_name(&station);
        assert!(name.ends_with('…'));
        assert!(name.chars().count() <= 34);
    }
}
