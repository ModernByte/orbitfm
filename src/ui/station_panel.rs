//! Правый drawer со списком станций, избранным и историей.

use eframe::egui;

use crate::geo::Country;
use crate::services::Station;

use super::icons;
use super::palette::Palette;
use super::{UiAction, single_line, truncate};

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
    pub markers_total: usize,
    pub results: &'a [Station],
    pub favorites: &'a [Station],
    pub history: &'a [Station],
    pub pending: bool,
    pub empty: bool,
    pub error: Option<&'a str>,
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
                egui::RichText::new("Найдите станцию по имени или кликните по точке на глобусе.")
                    .size(12.0)
                    .color(palette.muted),
            );
            return;
        }

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(
                state.tab,
                LibraryTab::Results,
                format!("Результаты ({})", state.results.len()),
            );
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
                                "Выберите страну или найдите станцию по имени."
                            };
                            ui.label(egui::RichText::new(message).color(palette.muted));
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

pub(crate) fn country_label(country: &Country) -> String {
    let name = single_line(&country.name);
    let name = if name.is_empty() {
        country.iso_a2.clone()
    } else {
        name
    };

    match country.iso_a3.as_deref() {
        Some(iso_a3) => format!("{name} ({}, {iso_a3})", country.iso_a2),
        None => format!("{name} ({})", country.iso_a2),
    }
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
            bitrate: 128,
            codec: "MP3".to_owned(),
            geo_lat: None,
            geo_long: None,
        }
    }

    #[test]
    fn country_label_includes_iso_codes() {
        let country = Country {
            name: "Germany".to_owned(),
            iso_a2: "DE".to_owned(),
            iso_a3: Some("DEU".to_owned()),
        };
        assert_eq!(country_label(&country), "Germany (DE, DEU)");
    }

    #[test]
    fn country_label_falls_back_to_iso_codes() {
        let country = Country {
            name: "  ".to_owned(),
            iso_a2: "XK".to_owned(),
            iso_a3: None,
        };
        assert_eq!(country_label(&country), "XK (XK)");
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
}
