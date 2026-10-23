//! Верхний HUD: логотип, поиск, now-playing, громкость и настройки.

use eframe::egui;

use crate::audio::PlaybackState;
use crate::services::Station;
use crate::settings::{AccentColor, Settings, ThemeMode};

use super::palette::{Palette, accent_color, accent_label, theme_label};
use super::{UiAction, playback_label, single_line, truncate};

const COMPACT_HUD_WIDTH: f32 = 1000.0;

pub(crate) struct HudState<'a> {
    pub query: &'a mut String,
    pub now_playing: Option<&'a Station>,
    pub playback: &'a PlaybackState,
    pub volume: f64,
    pub settings: Settings,
    pub panel_open: bool,
}

pub(crate) fn show(ui: &mut egui::Ui, palette: &Palette, state: HudState<'_>) -> Vec<UiAction> {
    let mut actions = Vec::new();
    let compact = ui.available_width() < COMPACT_HUD_WIDTH;

    palette.glass().show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("OrbitFM").size(22.0).strong());
                ui.label(
                    egui::RichText::new("Радио Земли")
                        .size(11.0)
                        .color(palette.muted),
                );
            });
            ui.add_space(12.0);

            let search_width = if compact { 150.0 } else { 260.0 };
            let response = ui.add(
                egui::TextEdit::singleline(state.query)
                    .hint_text("Поиск станции")
                    .desired_width(search_width),
            );
            let enter =
                response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if ui.button("Найти").clicked() || enter {
                actions.push(UiAction::RunSearch);
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut menu_action: Option<UiAction> = None;
                ui.menu_button("Настройки", |ui| {
                    ui.set_min_width(220.0);
                    ui.label(egui::RichText::new("Тема").strong());
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
                    ui.label(egui::RichText::new("Акцент").strong());
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
                    ui.separator();
                    if ui.button("Скрыть окно").clicked() {
                        menu_action = Some(UiAction::Hide);
                        ui.close();
                    }
                    if ui.button("Выход").clicked() {
                        menu_action = Some(UiAction::Quit);
                        ui.close();
                    }
                })
                .response
                .on_hover_text("Настройки");
                if let Some(action) = menu_action {
                    actions.push(action);
                }

                ui.spacing_mut().slider_width = if compact { 64.0 } else { 90.0 };
                let mut volume = state.volume;
                if ui
                    .add(egui::Slider::new(&mut volume, 0.0..=1.0).show_value(false))
                    .changed()
                {
                    actions.push(UiAction::SetVolume(volume));
                }
                if !compact {
                    ui.label(
                        egui::RichText::new(format!("{:>3.0}%", state.volume * 100.0))
                            .size(11.0)
                            .color(palette.muted),
                    );
                }

                let playing = matches!(
                    state.playback,
                    PlaybackState::Playing | PlaybackState::Buffering | PlaybackState::Reconnecting
                );
                let (name, details) = match state.now_playing {
                    Some(station) => {
                        let name = single_line(&station.name);
                        let name = if name.is_empty() {
                            "Без названия".to_owned()
                        } else {
                            truncate(&name, if compact { 18 } else { 26 })
                        };
                        let details = if compact {
                            String::new()
                        } else {
                            format!(" · {}", playback_label(state.playback))
                        };
                        (name, details)
                    }
                    None => (playback_label(state.playback), String::new()),
                };
                ui.label(
                    egui::RichText::new(format!("{name}{details}"))
                        .size(12.0)
                        .color(if playing {
                            palette.accent
                        } else {
                            palette.muted
                        }),
                );

                let panel_hint = if state.panel_open {
                    "Скрыть список станций"
                } else {
                    "Показать список станций"
                };
                if ui.button("Список").on_hover_text(panel_hint).clicked() {
                    actions.push(UiAction::TogglePanel);
                }
            });
        });
    });

    actions
}
