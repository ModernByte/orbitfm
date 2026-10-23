//! Нижний transport: управление воспроизведением и параметры эфира.

use eframe::egui;

use crate::audio::PlaybackState;
use crate::services::Station;

use super::UiAction;
use super::icons;
use super::palette::Palette;

pub(crate) struct TransportState<'a> {
    pub playback: &'a PlaybackState,
    pub current: Option<&'a Station>,
    pub favorite: bool,
    pub can_resume: bool,
    pub spectrum_enabled: bool,
}

pub(crate) fn show(
    ui: &mut egui::Ui,
    palette: &Palette,
    state: TransportState<'_>,
) -> Vec<UiAction> {
    let mut actions = Vec::new();

    palette.glass().show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        ui.horizontal(|ui| {
            let playing = matches!(
                state.playback,
                PlaybackState::Playing | PlaybackState::Buffering | PlaybackState::Reconnecting
            );
            let hint = if playing {
                "Пауза"
            } else {
                "Продолжить"
            };
            if icons::play_pause_button(ui, playing, state.can_resume, hint).clicked() {
                actions.push(UiAction::TogglePlayback);
            }

            let stop_enabled = !matches!(state.playback, PlaybackState::Stopped);
            if icons::stop_button(ui, stop_enabled).clicked() {
                actions.push(UiAction::Stop);
            }

            if let Some(station) = state.current {
                let (star_hint, star_color) = if state.favorite {
                    ("Убрать из избранного", palette.accent)
                } else {
                    ("Добавить в избранное", palette.muted)
                };
                if icons::star_button(ui, state.favorite, star_color, star_hint).clicked() {
                    actions.push(UiAction::ToggleFavorite(station.clone()));
                }
            }

            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(station_details(state.current))
                    .size(12.0)
                    .color(palette.muted),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut spectrum = state.spectrum_enabled;
                if ui
                    .checkbox(&mut spectrum, "FFT")
                    .on_hover_text(
                        "Экспериментальный FFT-визуализатор: агрегированные \
                         метрики mpv (без PCM)",
                    )
                    .changed()
                {
                    actions.push(UiAction::ToggleSpectrum(spectrum));
                }
            });
        });
    });

    actions
}

fn station_details(station: Option<&Station>) -> String {
    let Some(station) = station else {
        return "Эфир не выбран".to_owned();
    };

    let mut details = Vec::new();
    let codec = station.codec.trim();
    if !codec.is_empty() {
        details.push(codec.to_owned());
    }
    if station.bitrate > 0 {
        details.push(format!("{} kbps", station.bitrate));
    }
    let country = station.countrycode.trim();
    if !country.is_empty() {
        details.push(country.to_owned());
    }

    if details.is_empty() {
        "поток без параметров".to_owned()
    } else {
        details.join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn station() -> Station {
        Station {
            stationuuid: "uuid".to_owned(),
            name: "Groove Salad".to_owned(),
            url: "http://example.com/live".to_owned(),
            url_resolved: "https://example.com/live".to_owned(),
            homepage: None,
            favicon: None,
            tags: "ambient".to_owned(),
            countrycode: "US".to_owned(),
            language: "english".to_owned(),
            bitrate: 128,
            codec: "AAC".to_owned(),
            geo_lat: None,
            geo_long: None,
        }
    }

    #[test]
    fn details_include_codec_bitrate_and_country() {
        assert_eq!(station_details(Some(&station())), "AAC · 128 kbps · US");
    }

    #[test]
    fn details_fall_back_to_placeholder() {
        assert_eq!(station_details(None), "Эфир не выбран");
    }

    #[test]
    fn details_skip_empty_fields() {
        let mut station = station();
        station.codec = " ".to_owned();
        station.bitrate = 0;
        station.countrycode = String::new();
        assert_eq!(station_details(Some(&station)), "поток без параметров");
    }
}
