//! Верхний HUD: логотип, режимы, поиск, now-playing, громкость и настройки.

use eframe::egui;

use crate::audio::PlaybackState;
use crate::geo::GeoService;
use crate::services::Station;
use crate::session::WindowMode;
use crate::settings::{
    AccentColor, MotionReactivity, MotionStyle, Settings, ThemeMode, VIZ_BAR_CHOICES,
};

use super::icons::{close_button, gear_button, help_button, moon_button, panel_button};
use super::images::FaviconCache;
use super::palette::{Palette, accent_color, accent_label, theme_label};
use super::widgets::{avatar, live_badge, logo_mark, search_field};
use super::{
    SearchMode, UiAction, country_label, mode_switch, playback_label, single_line, truncate,
};

const COMPACT_HUD_WIDTH: f32 = 1000.0;
const TIGHT_HUD_WIDTH: f32 = 840.0;
const COUNTRY_SUGGESTIONS: usize = 6;

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
    pub settings: Settings,
    pub panel_open: bool,
    pub help_open: bool,
    pub window_mode: WindowMode,
}

pub(crate) fn show(ui: &mut egui::Ui, palette: &Palette, mut state: HudState<'_>) -> Vec<UiAction> {
    let mut actions = Vec::new();
    let width = ui.available_width();
    let compact = width < COMPACT_HUD_WIDTH;
    let tight = width < TIGHT_HUD_WIDTH;

    palette.glass().show(ui, |ui| {
        ui.set_min_size(ui.available_size());
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
                        egui::RichText::new("Global Internet Radio")
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
                },
            ));
            ui.add_space(8.0);

            let search_width = if compact { 150.0 } else { 240.0 };
            if state.mode == SearchMode::Country {
                actions.extend(country_search(ui, palette, &mut state, search_width));
            } else {
                let hint = if state.context_active {
                    "Filter stations…"
                } else {
                    "Search stations…"
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

    actions
}

fn playing(state: &HudState<'_>) -> bool {
    matches!(
        state.playback,
        PlaybackState::Playing | PlaybackState::Buffering | PlaybackState::Reconnecting
    )
}

/// Поиск страны с подсказками из `GeoService`; Enter выбирает первую страну.
fn country_search(
    ui: &mut egui::Ui,
    palette: &Palette,
    state: &mut HudState<'_>,
    width: f32,
) -> Vec<UiAction> {
    let mut actions = Vec::new();
    let output = search_field(
        ui,
        palette,
        egui::Id::unique("orbitfm_hud_country"),
        "Search country…",
        state.country_query,
        width,
    );
    let query = state.country_query.clone();
    let suggestions = state.geo.search_countries(&query, COUNTRY_SUGGESTIONS);
    let show_popup = (output.response.has_focus() || output.enter) && !query.trim().is_empty();

    if show_popup && !suggestions.is_empty() {
        egui::Popup::from_response(&output.response)
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
            egui::RichText::new("No match")
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

    let hint = if state.panel_open {
        "Hide station list"
    } else {
        "Show station list"
    };
    if panel_button(ui, hint).clicked() {
        actions.push(UiAction::TogglePanel);
    }

    if help_button(ui, "Mouse controls", state.help_open).clicked() {
        actions.push(UiAction::ToggleHelp);
    }

    if !tight {
        let mut menu_action: Option<UiAction> = None;
        let gear = gear_button(ui, "Settings");
        egui::Popup::menu(&gear)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(220.0);
                    ui.label(egui::RichText::new("Theme").strong().color(palette.text));
                    for mode in ThemeMode::ALL {
                        if ui
                            .selectable_label(state.settings.theme == mode, theme_label(mode))
                            .clicked()
                            && state.settings.theme != mode
                        {
                            menu_action = Some(UiAction::SetTheme(mode));
                            ui.close();
                        }
                    }
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Accent").strong().color(palette.text));
                    ui.horizontal_wrapped(|ui| {
                        for accent in AccentColor::ALL {
                            let selected = state.settings.accent == accent;
                            let stroke = if selected {
                                egui::Stroke::new(2.0, ui.visuals().strong_text_color())
                            } else {
                                egui::Stroke::new(1.0, palette.card_stroke)
                            };
                            let response = ui
                                .add(
                                    egui::Button::new("")
                                        .fill(accent_color(accent))
                                        .stroke(stroke)
                                        .min_size(egui::vec2(24.0, 18.0)),
                                )
                                .on_hover_text(accent_label(accent));
                            if response.clicked() && !selected {
                                menu_action = Some(UiAction::SetAccent(accent));
                                ui.close();
                            }
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Visualizer bars")
                            .strong()
                            .color(palette.text),
                    );
                    ui.horizontal_wrapped(|ui| {
                        for bars in VIZ_BAR_CHOICES {
                            let selected = state.settings.viz_bars == bars;
                            if ui.selectable_label(selected, bars.to_string()).clicked()
                                && !selected
                            {
                                menu_action = Some(UiAction::SetVizBars(bars));
                                ui.close();
                            }
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Motion").strong().color(palette.text));
                    ui.horizontal_wrapped(|ui| {
                        for motion in MotionReactivity::ALL {
                            let selected = state.settings.motion == motion;
                            if ui.selectable_label(selected, motion.label()).clicked() && !selected
                            {
                                menu_action = Some(UiAction::SetMotion(motion));
                                ui.close();
                            }
                        }
                    });
                    let mut reduce_motion = state.settings.reduce_motion;
                    if ui
                        .checkbox(&mut reduce_motion, "Reduce motion")
                        .on_hover_text("Stop squash, ripple and camera sway")
                        .changed()
                    {
                        menu_action = Some(UiAction::SetReduceMotion(reduce_motion));
                    }
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Motion style")
                            .strong()
                            .color(palette.text),
                    );
                    ui.horizontal_wrapped(|ui| {
                        for style in MotionStyle::ALL {
                            let selected = state.settings.motion_style == style;
                            if ui.selectable_label(selected, style.label()).clicked() && !selected {
                                menu_action = Some(UiAction::SetMotionStyle(style));
                                ui.close();
                            }
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Window").strong().color(palette.text));
                    ui.horizontal_wrapped(|ui| {
                        for mode in WindowMode::ALL {
                            let selected = state.window_mode == mode;
                            if ui.selectable_label(selected, mode.label()).clicked() && !selected {
                                menu_action = Some(UiAction::SetWindowMode(mode));
                                ui.close();
                            }
                        }
                    });
                    ui.separator();
                    if ui.button("Hide window").clicked() {
                        menu_action = Some(UiAction::Hide);
                        ui.close();
                    }
                    if ui.button("Quit").clicked() {
                        menu_action = Some(UiAction::Quit);
                        ui.close();
                    }
                });
            });
        if let Some(action) = menu_action {
            actions.push(action);
        }

        let dark = matches!(ui.ctx().theme(), egui::Theme::Dark);
        if moon_button(ui, if dark { "Light theme" } else { "Dark theme" }).clicked() {
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
                        "Untitled".to_owned()
                    } else {
                        truncate(&name, if compact { 18 } else { 26 })
                    }
                }
                None => playback_label(state.playback),
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
                        truncate(&playback_label(state.playback), 24)
                    } else {
                        truncate(&place, if compact { 20 } else { 30 })
                    }
                }
                None => "Idle".to_owned(),
            };
            ui.label(egui::RichText::new(details).size(10.5).color(palette.muted));
        });
    }

    if !tight {
        live_badge(ui, palette, is_playing);
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

    if close_button(ui, "Hide window (audio keeps playing)").clicked() {
        actions.push(UiAction::Hide);
    }

    actions
}
