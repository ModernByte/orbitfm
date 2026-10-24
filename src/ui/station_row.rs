//! Строка списка станций по эскизу: круглый favicon, имя, город,
//! битрейт/кодек и звезда. Играющая строка — accent-рамка с эквалайзером.

use eframe::egui;

use crate::services::Station;

use super::icons::star_button;
use super::images::FaviconCache;
use super::palette::Palette;
use super::widgets::{avatar, chip, eq_bars_in, focus_ring};
use super::{UiAction, single_line, truncate};

const ROW_HEIGHT: f32 = 58.0;
const TAG_LIMIT: usize = 3;

pub(crate) struct RowContext<'a> {
    pub playing: bool,
    pub compact: bool,
    pub favorites: &'a [Station],
    pub favicons: &'a mut FaviconCache,
    /// Соседи по видимому списку: стрелки вверх/вниз переводят фокус.
    pub previous: Option<&'a str>,
    pub next: Option<&'a str>,
}

/// Стабильный id строки: по нему фокус переходит к соседу при навигации.
pub(crate) fn row_id(stationuuid: &str) -> egui::Id {
    egui::Id::unique(("orbitfm_station_row", stationuuid))
}

/// Единая строка для Results/Favorites/History.
pub(crate) fn station_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    station: &Station,
    state: &mut RowContext<'_>,
    actions: &mut Vec<UiAction>,
) {
    let favorite = state
        .favorites
        .iter()
        .any(|item| item.stationuuid == station.stationuuid);
    let previous = state.previous;
    let next = state.next;
    let id = row_id(&station.stationuuid);

    let frame = egui::Frame::default()
        .fill(if state.playing {
            palette.chip_fill
        } else {
            egui::Color32::TRANSPARENT
        })
        .stroke(if state.playing {
            egui::Stroke::new(1.4, palette.accent)
        } else {
            egui::Stroke::NONE
        })
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(egui::Margin::symmetric(
            8,
            if state.compact { 4 } else { 6 },
        ));
    let frame_response = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        if !state.compact {
            ui.set_min_height(ROW_HEIGHT - 20.0);
        }
        draw_contents(ui, palette, station, state, favorite)
    });
    let star_response = frame_response.inner;

    let clicked = ui.interact(frame_response.response.rect, id, egui::Sense::click());
    let place = row_place(station);
    clicked.widget_info(|| {
        let name = row_name(station, state.compact);
        let label = if place.is_empty() {
            name
        } else {
            format!("{name}, {place}")
        };
        egui::WidgetInfo::labeled(egui::Role::ListItem, true, label)
    });
    focus_ring(ui, &clicked, clicked.rect, 12.0);

    if clicked.has_focus() {
        let (up, down) = ui.input(|input| {
            (
                input.key_pressed(egui::Key::ArrowUp),
                input.key_pressed(egui::Key::ArrowDown),
            )
        });
        let target = if up {
            previous
        } else if down {
            next
        } else {
            None
        };
        if let Some(uuid) = target {
            ui.memory_mut(|memory| memory.request_focus(row_id(uuid)));
        }
    }
    if clicked.gained_focus() {
        clicked.scroll_to_me(Some(egui::Align::Center));
    }

    if clicked.clicked() && !star_response.clicked() {
        actions.push(UiAction::Play(station.clone()));
    } else if star_response.clicked() {
        actions.push(UiAction::ToggleFavorite(station.clone()));
    }
    clicked.on_hover_text(single_line(&station.name));

    ui.add_space(if state.compact { 2.0 } else { 6.0 });
}

fn draw_contents(
    ui: &mut egui::Ui,
    palette: &Palette,
    station: &Station,
    state: &mut RowContext<'_>,
    favorite: bool,
) -> egui::Response {
    let texture = state
        .favicons
        .texture(station.favicon.as_deref().unwrap_or_default());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        let avatar_size = (ui.available_height() - 2.0).clamp(22.0, 44.0);
        avatar(ui, palette, texture, &station.name, avatar_size);

        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = if state.compact { 0.0 } else { 1.0 };
            let name_color = if state.playing {
                palette.accent
            } else {
                palette.text
            };
            ui.label(
                egui::RichText::new(row_name(station, state.compact))
                    .size(if state.compact { 12.0 } else { 13.5 })
                    .strong()
                    .color(name_color),
            );
            if !state.compact {
                let place = row_place(station);
                if !place.is_empty() {
                    ui.label(
                        egui::RichText::new(truncate(&place, 34))
                            .size(10.5)
                            .color(palette.muted),
                    );
                }
                if state.playing {
                    let bars = egui::vec2(24.0, 12.0);
                    let (bars_rect, _) = ui.allocate_at_least(bars, egui::Sense::hover());
                    eq_bars_in(ui, palette, bars_rect, 5, true);
                }
            }
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let hint = if favorite {
                "Remove from favorites"
            } else {
                "Add to favorites"
            };
            let color = if favorite {
                palette.accent
            } else {
                palette.muted
            };
            let star = star_button(ui, favorite, color, hint);
            if !state.compact {
                ui.vertical(|ui| {
                    ui.set_width(64.0);
                    ui.label(
                        egui::RichText::new(row_bitrate(station))
                            .size(11.0)
                            .color(palette.text),
                    );
                    ui.label(
                        egui::RichText::new(row_codec(station))
                            .size(10.0)
                            .color(palette.muted),
                    );
                });
            }
            star
        })
        .inner
    })
    .inner
}

/// Чипы тегов станции; клик — глобальный поиск по тегу.
pub(crate) fn station_tag_chips(
    ui: &mut egui::Ui,
    palette: &Palette,
    station: &Station,
    actions: &mut Vec<UiAction>,
) {
    let mut shown = 0usize;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for tag in station
            .tags
            .split(',')
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
        {
            if shown >= TAG_LIMIT {
                break;
            }
            let label = truncate(tag, 14);
            if chip(ui, palette, &label, false).clicked() {
                actions.push(UiAction::SelectGenre(tag.to_owned()));
            }
            shown += 1;
        }
    });
}

fn row_name(station: &Station, compact: bool) -> String {
    let name = single_line(&station.name);
    if name.is_empty() {
        "Untitled".to_owned()
    } else {
        truncate(&name, if compact { 26 } else { 32 })
    }
}

pub(crate) fn row_place(station: &Station) -> String {
    let state_name = single_line(&station.state);
    let country = single_line(&station.country);
    match (state_name.is_empty(), country.is_empty()) {
        (false, false) => format!("{state_name}, {country}"),
        (false, true) => state_name,
        (true, false) => country,
        (true, true) => single_line(&station.countrycode),
    }
}

fn row_bitrate(station: &Station) -> String {
    if station.bitrate > 0 {
        format!("{} kbps", station.bitrate)
    } else {
        String::new()
    }
}

fn row_codec(station: &Station) -> String {
    single_line(&station.codec)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_station() -> Station {
        Station {
            stationuuid: "uuid-1".to_owned(),
            name: "  Radio   Nova \nLive".to_owned(),
            url: "http://example.com/live".to_owned(),
            url_resolved: "https://example.com/live".to_owned(),
            homepage: None,
            favicon: Some("https://example.com/icon.png".to_owned()),
            tags: "jazz, smooth, vocal".to_owned(),
            countrycode: "FR".to_owned(),
            language: "french".to_owned(),
            votes: 90,
            country: "France".to_owned(),
            state: "Île-de-France".to_owned(),
            bitrate: 320,
            codec: "MP3".to_owned(),
            geo_lat: None,
            geo_long: None,
        }
    }

    #[test]
    fn row_name_is_single_line_with_placeholder() {
        assert_eq!(row_name(&sample_station(), false), "Radio Nova Live");
        let mut blank = sample_station();
        blank.name = "   ".to_owned();
        assert_eq!(row_name(&blank, true), "Untitled");
    }

    #[test]
    fn row_place_prefers_state_then_country_then_code() {
        assert_eq!(row_place(&sample_station()), "Île-de-France, France");
        let mut only_code = sample_station();
        only_code.state.clear();
        only_code.country.clear();
        assert_eq!(row_place(&only_code), "FR");
    }

    #[test]
    fn row_bitrate_and_codec_hide_zero_bitrate() {
        assert_eq!(row_bitrate(&sample_station()), "320 kbps");
        assert_eq!(row_codec(&sample_station()), "MP3");
        let mut quiet = sample_station();
        quiet.bitrate = 0;
        assert_eq!(row_bitrate(&quiet), "");
    }

    fn second_station() -> Station {
        let mut station = sample_station();
        station.stationuuid = "uuid-2".to_owned();
        station.name = "Second Radio".to_owned();
        station
    }

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Один кадр headless-egui с двумя соседними строками списка.
    fn run_frame(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        request: Option<egui::Id>,
    ) -> Vec<UiAction> {
        let palette = Palette::for_ui(egui::Theme::Dark, crate::settings::AccentColor::Blue);
        let first = sample_station();
        let second = second_station();
        let favorites: [Station; 0] = [];
        let mut actions = Vec::new();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(420.0, 600.0),
            )),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            if let Some(id) = request {
                ui.memory_mut(|memory| memory.request_focus(id));
            }
            let mut first_cache = FaviconCache::new();
            let mut first_state = RowContext {
                playing: false,
                compact: false,
                favorites: &favorites,
                favicons: &mut first_cache,
                previous: None,
                next: Some("uuid-2"),
            };
            station_row(ui, &palette, &first, &mut first_state, &mut actions);
            let mut second_cache = FaviconCache::new();
            let mut second_state = RowContext {
                playing: false,
                compact: false,
                favorites: &favorites,
                favicons: &mut second_cache,
                previous: Some("uuid-1"),
                next: None,
            };
            station_row(ui, &palette, &second, &mut second_state, &mut actions);
        });
        output.textures_delta.clear();
        actions
    }

    #[test]
    fn arrow_keys_move_focus_along_the_visible_list() {
        let ctx = egui::Context::default();
        let first = row_id("uuid-1");
        let second = row_id("uuid-2");
        run_frame(&ctx, Vec::new(), Some(first));
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(first));

        run_frame(&ctx, vec![key(egui::Key::ArrowDown)], None);
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(second));

        run_frame(&ctx, vec![key(egui::Key::ArrowUp)], None);
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(first));
    }

    #[test]
    fn space_activates_the_focused_row() {
        let ctx = egui::Context::default();
        run_frame(&ctx, Vec::new(), Some(row_id("uuid-1")));
        let actions = run_frame(&ctx, vec![key(egui::Key::Space)], None);
        assert!(
            matches!(actions.as_slice(), [UiAction::Play(station)] if station.stationuuid == "uuid-1"),
            "Space на сфокусированной строке должен запускать станцию: {actions:?}"
        );
    }
}
