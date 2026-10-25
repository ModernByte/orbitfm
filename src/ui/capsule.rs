//! Капсулы WindowMode (ADR-0021): Overlay — станция, мини-глобус и избранное;
//! Listening — одна строка now-playing с пульсом эфира.

use std::time::Duration;

use eframe::egui;

use crate::audio::PlaybackState;
use crate::services::Station;
use crate::session::WindowMode;

use super::icons::{close_button, expand_button, gear_button, heart_button, stop_button};
use super::images::FaviconCache;
use super::palette::Palette;
use super::station_row::{RowContext, row_place, station_row};
use super::widgets::{avatar, circle_button_sized, faded, volume_slider_width};
use super::{UiAction, playback_label, single_line, truncate};

/// Сколько избранных показывает капсула Overlay (design-vision §6: 5–8).
const FAVORITE_LIMIT: usize = 8;
/// 20 fps достаточно для «дыхания» обода; капсула не грузит GPU лишним.
const PULSE_REPAINT: Duration = Duration::from_millis(50);
const OVERLAY_VOLUME_WIDTH: f32 = 72.0;

pub(crate) struct OverlayTop<'a> {
    pub playback: &'a PlaybackState,
    pub current: Option<&'a Station>,
    pub can_resume: bool,
    pub volume: f64,
    pub favicons: &'a mut FaviconCache,
}

pub(crate) struct OverlayFavorites<'a> {
    pub favorites: &'a [Station],
    pub playing_uuid: Option<&'a str>,
    pub favicons: &'a mut FaviconCache,
}

pub(crate) struct Listening<'a> {
    pub playback: &'a PlaybackState,
    pub current: Option<&'a Station>,
    pub favorite: bool,
    pub can_resume: bool,
    pub pulse: f32,
    pub favicons: &'a mut FaviconCache,
}

fn playing(state: &PlaybackState) -> bool {
    matches!(
        state,
        PlaybackState::Playing | PlaybackState::Buffering | PlaybackState::Reconnecting
    )
}

fn station_name(station: Option<&Station>) -> String {
    station
        .map(|station| single_line(&station.name))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Nothing playing".to_owned())
}

fn overlay_subtitle(station: Option<&Station>, playback: &PlaybackState) -> String {
    let place = station.map(row_place).unwrap_or_default();
    let status = playback_label(playback);
    if place.is_empty() {
        status
    } else {
        format!("{status} · {place}")
    }
}

fn listening_line(station: Option<&Station>, playback: &PlaybackState) -> String {
    let Some(station) = station else {
        return playback_label(playback);
    };
    [
        playback_label(playback),
        row_place(station),
        if station.bitrate > 0 {
            format!("{} kbps", station.bitrate)
        } else {
            String::new()
        },
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" · ")
}

/// Верх капсулы Overlay: станция, play/stop, громкость и выход в Focus.
pub(crate) fn overlay_top(
    ui: &mut egui::Ui,
    palette: &Palette,
    state: OverlayTop<'_>,
) -> Vec<UiAction> {
    let mut actions = Vec::new();
    let OverlayTop {
        playback,
        current,
        can_resume,
        volume,
        favicons,
    } = state;
    let is_playing = playing(playback);
    let name = station_name(current);
    let subtitle = overlay_subtitle(current, playback);
    let texture = current
        .and_then(|station| station.favicon.as_deref())
        .and_then(|url| favicons.texture(url));
    let mut volume = volume;

    palette.glass().show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        ui.horizontal_centered(|ui| {
            avatar(ui, palette, texture, &name, 30.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(
                    egui::RichText::new(truncate(&name, 18))
                        .size(12.5)
                        .strong()
                        .color(if is_playing {
                            palette.accent
                        } else {
                            palette.text
                        }),
                );
                ui.label(
                    egui::RichText::new(truncate(&subtitle, 24))
                        .size(10.0)
                        .color(palette.muted),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if close_button(ui, "Hide window (audio keeps playing)").clicked() {
                    actions.push(UiAction::Hide);
                }
                if expand_button(ui, "Open Focus layout").clicked() {
                    actions.push(UiAction::SetWindowMode(WindowMode::Focus));
                }
                if gear_button(ui, "Settings").clicked() {
                    actions.push(UiAction::ToggleSettings);
                }
                let slider = volume_slider_width(ui, palette, &mut volume, OVERLAY_VOLUME_WIDTH);
                if slider.changed && !slider.committed {
                    actions.push(UiAction::PreviewVolume(volume));
                }
                if slider.committed {
                    actions.push(UiAction::CommitVolume(volume));
                }
                if stop_button(ui, !matches!(playback, PlaybackState::Stopped)).clicked() {
                    actions.push(UiAction::Stop);
                }
                let hint = if is_playing { "Pause" } else { "Resume" };
                if circle_button_sized(
                    ui,
                    palette,
                    30.0,
                    is_playing,
                    is_playing || can_resume,
                    hint,
                )
                .clicked()
                {
                    actions.push(UiAction::TogglePlayback);
                }
            });
        });
    });

    actions
}

/// Низ капсулы Overlay: до восьми избранных станций, строка запускает эфир.
pub(crate) fn overlay_favorites(
    ui: &mut egui::Ui,
    palette: &Palette,
    state: OverlayFavorites<'_>,
) -> Vec<UiAction> {
    let mut actions = Vec::new();
    let OverlayFavorites {
        favorites,
        playing_uuid,
        favicons,
    } = state;
    let visible = visible_favorites(favorites);

    palette.panel(14).show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Favorites").size(12.5).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let count = if visible.len() < favorites.len() {
                    format!("{} of {}", visible.len(), favorites.len())
                } else {
                    visible.len().to_string()
                };
                ui.label(egui::RichText::new(count).size(10.5).color(palette.muted));
            });
        });
        if visible.is_empty() {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("No favorites yet")
                    .size(12.5)
                    .color(palette.muted),
            );
            ui.label(
                egui::RichText::new("Star a station in Focus to keep it here.")
                    .size(10.5)
                    .color(palette.muted),
            );
            return;
        }

        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .id_salt("overlay_favorites")
            .auto_shrink([false, false])
            .max_height(ui.available_height())
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                for (index, station) in visible.iter().enumerate() {
                    let mut row = RowContext {
                        playing: playing_uuid == Some(station.stationuuid.as_str()),
                        compact: true,
                        favorites,
                        favicons: &mut *favicons,
                        previous: index
                            .checked_sub(1)
                            .map(|item| visible[item].stationuuid.as_str()),
                        next: visible.get(index + 1).map(|item| item.stationuuid.as_str()),
                    };
                    station_row(ui, palette, station, &mut row, &mut actions);
                }
            });
    });

    actions
}

/// Listening: одна строка now-playing; обод аватара пульсирует в такт эфиру.
pub(crate) fn listening(
    ui: &mut egui::Ui,
    palette: &Palette,
    state: Listening<'_>,
) -> Vec<UiAction> {
    let mut actions = Vec::new();
    let Listening {
        playback,
        current,
        favorite,
        can_resume,
        pulse,
        favicons,
    } = state;
    let is_playing = playing(playback);
    let name = station_name(current);
    let details = listening_line(current, playback);
    let texture = current
        .and_then(|station| station.favicon.as_deref())
        .and_then(|url| favicons.texture(url));

    palette.glass().show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        ui.horizontal_centered(|ui| {
            let avatar_rect = avatar(ui, palette, texture, &name, 44.0);
            if is_playing {
                let time = ui.input(|input| input.time) as f32;
                let breath = 0.5 + 0.5 * (time * 2.0).sin();
                let radius = avatar_rect.width() * 0.5 + 3.5 + 3.0 * pulse + 1.5 * breath;
                ui.painter().circle_stroke(
                    avatar_rect.center(),
                    radius,
                    egui::Stroke::new(
                        1.4,
                        faded(palette.accent, 0.30 + 0.45 * pulse.clamp(0.0, 1.0)),
                    ),
                );
                ui.ctx().request_repaint_after(PULSE_REPAINT);
            }
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.label(
                    egui::RichText::new(truncate(&name, 22))
                        .size(15.0)
                        .strong()
                        .color(if is_playing {
                            palette.accent
                        } else {
                            palette.text
                        }),
                );
                ui.label(
                    egui::RichText::new(truncate(&details, 30))
                        .size(10.5)
                        .color(palette.muted),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if close_button(ui, "Hide window (audio keeps playing)").clicked() {
                    actions.push(UiAction::Hide);
                }
                if expand_button(ui, "Open Focus layout").clicked() {
                    actions.push(UiAction::SetWindowMode(WindowMode::Focus));
                }
                if let Some(station) = current {
                    let color = if favorite {
                        palette.accent
                    } else {
                        palette.muted
                    };
                    let hint = if favorite {
                        "Remove from favorites"
                    } else {
                        "Add to favorites"
                    };
                    if heart_button(ui, favorite, color, hint).clicked() {
                        actions.push(UiAction::ToggleFavorite(station.clone()));
                    }
                }
                let hint = if is_playing { "Pause" } else { "Resume" };
                if circle_button_sized(
                    ui,
                    palette,
                    40.0,
                    is_playing,
                    is_playing || can_resume,
                    hint,
                )
                .clicked()
                {
                    actions.push(UiAction::TogglePlayback);
                }
            });
        });
    });

    actions
}

/// Видимый срез избранного: капсула не пытается показать всю библиотеку.
fn visible_favorites(favorites: &[Station]) -> &[Station] {
    &favorites[..favorites.len().min(FAVORITE_LIMIT)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn station(name: &str) -> Station {
        Station {
            stationuuid: "uuid".to_owned(),
            name: name.to_owned(),
            url: "http://example.com/live".to_owned(),
            url_resolved: "https://example.com/live".to_owned(),
            homepage: None,
            favicon: None,
            tags: "jazz".to_owned(),
            countrycode: "US".to_owned(),
            language: "english".to_owned(),
            votes: 10,
            country: "United States".to_owned(),
            state: "New York".to_owned(),
            bitrate: 128,
            codec: "MP3".to_owned(),
            geo_lat: None,
            geo_long: None,
        }
    }

    #[test]
    fn favorites_are_capped_for_the_capsule() {
        let list: Vec<Station> = (0..12).map(|_| station("Radio")).collect();
        assert_eq!(visible_favorites(&list).len(), FAVORITE_LIMIT);
        assert_eq!(visible_favorites(&list[..3]).len(), 3);
        assert!(visible_favorites(&[]).is_empty());
    }

    #[test]
    fn listening_line_joins_status_place_and_bitrate() {
        assert_eq!(
            listening_line(Some(&station("Radio Nova")), &PlaybackState::Playing),
            "on air · New York, United States · 128 kbps"
        );
        assert_eq!(listening_line(None, &PlaybackState::Stopped), "stopped");
    }

    #[test]
    fn overlay_subtitle_falls_back_to_playback_state() {
        assert_eq!(
            overlay_subtitle(Some(&station("Radio Nova")), &PlaybackState::Playing),
            "on air · New York, United States"
        );
        assert_eq!(
            overlay_subtitle(None, &PlaybackState::Buffering),
            "buffering"
        );
    }

    #[test]
    fn station_name_has_a_placeholder() {
        assert_eq!(station_name(Some(&station("  "))), "Nothing playing");
        assert_eq!(station_name(None), "Nothing playing");
        assert_eq!(station_name(Some(&station("Radio\nNova"))), "Radio Nova");
    }
}
