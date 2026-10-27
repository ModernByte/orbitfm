//! Капсулы WindowMode (ADR-0021): Overlay — станция, мини-глобус и избранное;
//! Listening — одна строка now-playing с пульсом эфира.
//!
//! Переходы содержимого — по ADR-0032: вход 180 мс вместе с каркасом капсулы,
//! строки избранного одним stagger, overshoot шкалы разрешён только кнопке
//! play/stop. Правило простое: элемент гасится ровно одним способом. Контролы,
//! чей цвет приходит из палитры, получают погашенную палитру
//! ([`super::widgets::faded_palette`]); всё, что рисуется цветом из `visuals`
//! egui или favicon-текстурой ([`super::station_row`], кнопки-иконки,
//! [`super::widgets::avatar`]), — групповой прозрачностью
//! [`super::widgets::faded_group`]. Оба механизма влияют только на отрисовку:
//! аллокации и hit-testing остаются на исходных прямоугольниках.

use eframe::egui::{self, Color32};

use crate::audio::PlaybackState;
use crate::i18n;
use crate::services::Station;
use crate::session::WindowMode;
use crate::settings::Language;

use super::animation::{self, FrameMotion};
use super::icons::{close_button, expand_button, gear_button, heart_button, stop_button};
use super::images::FaviconCache;
use super::palette::{Palette, mix_colors};
use super::station_row::{RowContext, row_place, station_row};
use super::widgets::{
    CapsuleDisc, avatar, capsule_button, faded, faded_group, faded_palette, pop_scale,
    volume_slider_width,
};
use super::{UiAction, playback_label, single_line, truncate};

/// Сколько избранных показывает капсула Overlay (design-vision §6: 5–8).
const FAVORITE_LIMIT: usize = 8;
const OVERLAY_VOLUME_WIDTH: f32 = 72.0;
/// Приглушение диска в паузе: accent остаётся акцентом, но полупрозрачнее
/// (ТЗ §3.4 требует сплошной accent только для активного play).
const IDLE_DIM: f32 = 0.55;
/// Ключ перехода цвета play/stop верхней капсулы. `egui::Id::unique`
/// детерминирован для одного источника, поэтому id стабилен между кадрами.
const PLAY_TOP: &str = "orbitfm_capsule_play_color_top";
/// Тот же переход для строки Listening.
const PLAY_LISTENING: &str = "orbitfm_capsule_play_color_listening";
/// Ключ press-перехода капсульной кнопки.
const PRESS_TOP: &str = "orbitfm_capsule_press_top";
const PRESS_LISTENING: &str = "orbitfm_capsule_press_listening";

pub(crate) struct OverlayTop<'a> {
    pub playback: &'a PlaybackState,
    pub current: Option<&'a Station>,
    pub can_resume: bool,
    pub volume: f64,
    pub favicons: &'a mut FaviconCache,
    /// Разрешённый язык интерфейса (ADR-0030).
    pub language: Language,
}

pub(crate) struct OverlayFavorites<'a> {
    pub favorites: &'a [Station],
    pub playing_uuid: Option<&'a str>,
    pub favicons: &'a mut FaviconCache,
    /// Разрешённый язык интерфейса (ADR-0030).
    pub language: Language,
}

pub(crate) struct Listening<'a> {
    pub playback: &'a PlaybackState,
    pub current: Option<&'a Station>,
    pub favorite: bool,
    pub can_resume: bool,
    pub pulse: f32,
    pub favicons: &'a mut FaviconCache,
    /// Разрешённый язык интерфейса (ADR-0030).
    pub language: Language,
}

fn playing(state: &PlaybackState) -> bool {
    matches!(
        state,
        PlaybackState::Playing | PlaybackState::Buffering | PlaybackState::Reconnecting
    )
}

/// Прогресс появления одной строки избранного: общий вход капсулы и 18 мс
/// задержки на каждую следующую (ADR-0032 §2). На терминальном прогрессе
/// `stagger` даёт 1.0 для любой строки, поэтому reduce motion не требует
/// отдельного выключателя — список сразу виден целиком.
fn row_progress(enter: f32, index: usize) -> f32 {
    animation::alpha(animation::stagger(index, enter))
}

/// Прогресс смены цвета play/stop. Своего состояния на окно у капсулы нет,
/// поэтому переход живёт в `motion.keyed` (им владеет оркестратор) и опрашивается
/// ровно один раз за кадр.
fn play_progress(
    ctx: &egui::Context,
    motion: &mut FrameMotion<'_>,
    key: &str,
    playing: bool,
) -> f32 {
    motion.keyed.value(
        ctx,
        egui::Id::unique(key),
        playing,
        animation::CAPSULE,
        motion.reduce,
    )
}

/// Цвет диска play/stop: приглушённый accent в паузе, сплошной — в эфире.
/// Оба конца уже умножены на альфу входа, поэтому смешение непрерывно и смена
/// `PlaybackState` не щёлкает яркостью.
fn disc_fill(palette: &Palette, progress: f32) -> Color32 {
    let idle = faded(palette.accent, IDLE_DIM);
    mix_colors(idle, palette.accent, progress)
}

/// Имя станции или заглушка из каталога (ADR-0030): пустое имя станции из
/// Radio Browser тоже обязано быть переведённым.
fn station_name(language: Language, station: Option<&Station>) -> String {
    let text = i18n::catalog(language);
    station
        .map(|station| single_line(&station.name))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| text.common.nothing_playing.to_owned())
}

fn overlay_subtitle(
    language: Language,
    station: Option<&Station>,
    playback: &PlaybackState,
) -> String {
    let place = station.map(row_place).unwrap_or_default();
    let status = playback_label(language, playback);
    if place.is_empty() {
        status
    } else {
        format!("{status} · {place}")
    }
}

fn listening_line(
    language: Language,
    station: Option<&Station>,
    playback: &PlaybackState,
) -> String {
    let text = i18n::catalog(language);
    let Some(station) = station else {
        return playback_label(language, playback);
    };
    let bitrate = if station.bitrate > 0 {
        let count = station.bitrate.to_string();
        i18n::fill(text.transport.kbps, &[("count", &count)])
    } else {
        String::new()
    };
    [
        playback_label(language, playback),
        row_place(station),
        bitrate,
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
    motion: &mut FrameMotion<'_>,
    state: OverlayTop<'_>,
) -> Vec<UiAction> {
    let mut actions = Vec::new();
    let OverlayTop {
        playback,
        current,
        can_resume,
        volume,
        favicons,
        language,
    } = state;
    let is_playing = playing(playback);
    let text = i18n::catalog(language);
    let name = station_name(language, current);
    let subtitle = overlay_subtitle(language, current, playback);
    let texture = current
        .and_then(|station| station.favicon.as_deref())
        .and_then(|url| favicons.texture(url));
    let mut volume = volume;

    // Содержимое входит теми же 180 мс, что и каркас капсулы.
    let enter = animation::alpha(motion.capsules);
    let shell = faded_palette(palette, enter);
    let pop = pop_scale(enter, motion.reduce);
    let fill = disc_fill(
        &shell,
        play_progress(ui.ctx(), motion, PLAY_TOP, is_playing),
    );

    palette.glass().show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        ui.horizontal_centered(|ui| {
            // Favicon рисуется текстурой, на которую палитра не влияет,
            // поэтому аватар гасится групповой прозрачностью.
            faded_group(ui, enter, |ui| {
                avatar(ui, palette, texture, &name, 30.0);
            });
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(
                    egui::RichText::new(truncate(&name, 18))
                        .size(12.5)
                        .strong()
                        .color(if is_playing { shell.accent } else { shell.text }),
                );
                ui.label(
                    egui::RichText::new(truncate(&subtitle, 24))
                        .size(10.0)
                        .color(shell.muted),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                faded_group(ui, enter, |ui| {
                    if close_button(ui, palette, text.common.hide_window_hint).clicked() {
                        actions.push(UiAction::Hide);
                    }
                    if expand_button(ui, palette, text.common.open_focus).clicked() {
                        actions.push(UiAction::SetWindowMode(WindowMode::Focus));
                    }
                    if gear_button(ui, palette, text.common.settings).clicked() {
                        actions.push(UiAction::ToggleSettings);
                    }
                });
                let slider =
                    volume_slider_width(ui, &shell, &mut volume, OVERLAY_VOLUME_WIDTH, language);
                if slider.changed && !slider.committed {
                    actions.push(UiAction::PreviewVolume(volume));
                }
                if slider.committed {
                    actions.push(UiAction::CommitVolume(volume));
                }
                faded_group(ui, enter, |ui| {
                    if stop_button(
                        ui,
                        palette,
                        !matches!(playback, PlaybackState::Stopped),
                        language,
                    )
                    .clicked()
                    {
                        actions.push(UiAction::Stop);
                    }
                });
                let hint = if is_playing {
                    text.common.pause
                } else {
                    text.common.resume
                };
                let disc = CapsuleDisc {
                    palette: &shell,
                    size: 30.0,
                    pop,
                    fill,
                    playing: is_playing,
                    enabled: is_playing || can_resume,
                    hint,
                };
                if capsule_button(ui, motion, egui::Id::unique(PRESS_TOP), &disc).clicked() {
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
    motion: &mut FrameMotion<'_>,
    state: OverlayFavorites<'_>,
) -> Vec<UiAction> {
    let mut actions = Vec::new();
    let OverlayFavorites {
        favorites,
        playing_uuid,
        favicons,
        language,
    } = state;
    let text = i18n::catalog(language);
    let visible = visible_favorites(favorites);

    // Заголовок гаснет вместе с каркасом, строки — одним stagger поверх него.
    let enter = animation::alpha(motion.favorites);
    let shell = faded_palette(palette, enter);

    palette.panel(14).show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(text.capsule.favorites)
                    .size(12.5)
                    .strong()
                    .color(shell.text),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let count = if visible.len() < favorites.len() {
                    let shown = visible.len().to_string();
                    let total = favorites.len().to_string();
                    i18n::fill(
                        text.capsule.count_of,
                        &[("count", &shown), ("total", &total)],
                    )
                } else {
                    visible.len().to_string()
                };
                ui.label(egui::RichText::new(count).size(10.5).color(shell.muted));
            });
        });
        if visible.is_empty() {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(text.capsule.no_favorites)
                    .size(12.5)
                    .color(shell.muted),
            );
            ui.label(
                egui::RichText::new(text.capsule.favorite_hint)
                    .size(10.5)
                    .color(shell.muted),
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
                        language,
                        favicons: &mut *favicons,
                        previous: index
                            .checked_sub(1)
                            .map(|item| visible[item].stationuuid.as_str()),
                        next: visible.get(index + 1).map(|item| item.stationuuid.as_str()),
                    };
                    // Строка целиком (палитра, favicon, звезда из `visuals`) гаснет
                    // одной групповой прозрачностью: двойного приглушения нет, а
                    // аллокации остаются на своих местах.
                    faded_group(ui, row_progress(enter, index), |ui| {
                        station_row(ui, palette, station, &mut row, &mut actions);
                    });
                }
            });
    });

    actions
}

/// Listening: одна строка now-playing; обод аватара пульсирует в такт эфиру.
pub(crate) fn listening(
    ui: &mut egui::Ui,
    palette: &Palette,
    motion: &mut FrameMotion<'_>,
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
        language,
    } = state;
    let is_playing = playing(playback);
    let text = i18n::catalog(language);
    let name = station_name(language, current);
    let details = listening_line(language, current, playback);
    let texture = current
        .and_then(|station| station.favicon.as_deref())
        .and_then(|url| favicons.texture(url));

    // Строка входит вместе со своим каркасом; пульс эфира остаётся отдельным
    // слоем, который оркестратор обслуживает своими кадрами.
    let enter = animation::alpha(motion.listening);
    let shell = faded_palette(palette, enter);
    let pop = pop_scale(enter, motion.reduce);
    let fill = disc_fill(
        &shell,
        play_progress(ui.ctx(), motion, PLAY_LISTENING, is_playing),
    );
    let glow = animation::alpha(pulse);

    palette.glass().show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        ui.horizontal_centered(|ui| {
            let avatar_rect =
                faded_group(ui, enter, |ui| avatar(ui, palette, texture, &name, 44.0));
            if is_playing {
                // Reduce motion: обод не двигается, пульс остаётся только в
                // прозрачности (ADR-0032 §3 — приглушённом движении, а не цвете).
                let radius =
                    avatar_rect.width() * 0.5 + 3.5 + if motion.reduce { 0.0 } else { 3.0 * glow };
                ui.painter().circle_stroke(
                    avatar_rect.center(),
                    radius,
                    egui::Stroke::new(1.4, faded(shell.accent, 0.30 + 0.45 * glow)),
                );
            }
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.label(
                    egui::RichText::new(truncate(&name, 22))
                        .size(15.0)
                        .strong()
                        .color(if is_playing { shell.accent } else { shell.text }),
                );
                ui.label(
                    egui::RichText::new(truncate(&details, 30))
                        .size(10.5)
                        .color(shell.muted),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                faded_group(ui, enter, |ui| {
                    if close_button(ui, palette, text.common.hide_window_hint).clicked() {
                        actions.push(UiAction::Hide);
                    }
                    if expand_button(ui, palette, text.common.open_focus).clicked() {
                        actions.push(UiAction::SetWindowMode(WindowMode::Focus));
                    }
                    if let Some(station) = current {
                        // Внутри `faded_group` цвет берётся из чистой палитры:
                        // групповая прозрачность уже умножает альфу на `enter`,
                        // второе приглушение сделало бы звезду бледнее текста.
                        let color = if favorite {
                            palette.accent
                        } else {
                            palette.muted
                        };
                        let hint = if favorite {
                            text.common.remove_favorite
                        } else {
                            text.common.add_favorite
                        };
                        if heart_button(ui, palette, favorite, color, hint).clicked() {
                            actions.push(UiAction::ToggleFavorite(station.clone()));
                        }
                    }
                });
                let hint = if is_playing {
                    text.common.pause
                } else {
                    text.common.resume
                };
                let disc = CapsuleDisc {
                    palette: &shell,
                    size: 40.0,
                    pop,
                    fill,
                    playing: is_playing,
                    enabled: is_playing || can_resume,
                    hint,
                };
                if capsule_button(ui, motion, egui::Id::unique(PRESS_LISTENING), &disc).clicked() {
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
    use crate::settings::AccentColor;

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
        let text = i18n::catalog(Language::English);
        assert_eq!(
            listening_line(
                Language::English,
                Some(&station("Radio Nova")),
                &PlaybackState::Playing
            ),
            "on air · New York, United States · 128 kbps"
        );
        assert_eq!(
            listening_line(Language::English, None, &PlaybackState::Stopped),
            "stopped"
        );
        // Битрейт собирается из шаблона каталога, а не из литерала.
        let count = "128".to_owned();
        assert_eq!(
            i18n::fill(text.transport.kbps, &[("count", &count)]),
            "128 kbps"
        );
    }

    #[test]
    fn overlay_subtitle_falls_back_to_playback_state() {
        assert_eq!(
            overlay_subtitle(
                Language::English,
                Some(&station("Radio Nova")),
                &PlaybackState::Playing
            ),
            "on air · New York, United States"
        );
        assert_eq!(
            overlay_subtitle(Language::English, None, &PlaybackState::Buffering),
            "buffering"
        );
    }

    #[test]
    fn station_name_has_a_placeholder() {
        let text = i18n::catalog(Language::English);
        assert_eq!(
            station_name(Language::English, Some(&station("  "))),
            text.common.nothing_playing,
            "заглушка имени читается из каталога"
        );
        assert_eq!(station_name(Language::English, None), "Nothing playing");
        assert_eq!(
            station_name(Language::English, Some(&station("Radio\nNova"))),
            "Radio Nova"
        );
    }

    #[test]
    fn capsule_labels_come_from_the_catalog() {
        // Любой разрешённый язык обязан отдавать непустые строки капсулы.
        for language in [Language::English, Language::Russian] {
            let text = i18n::catalog(language);
            for value in [
                text.capsule.favorites,
                text.capsule.no_favorites,
                text.capsule.favorite_hint,
                text.capsule.count_of,
                text.common.pause,
                text.common.resume,
                text.common.settings,
                text.common.open_focus,
                text.common.hide_window_hint,
                text.common.add_favorite,
                text.common.remove_favorite,
            ] {
                assert!(!value.trim().is_empty(), "пустая строка на {language:?}");
            }
        }
        let english = i18n::catalog(Language::English);
        assert_eq!(english.capsule.favorites, "Favorites");
        assert_eq!(english.capsule.no_favorites, "No favorites yet");
        assert_eq!(
            english.capsule.favorite_hint,
            "Star a station in Focus to keep it here."
        );
        assert_eq!(
            i18n::fill(english.capsule.count_of, &[("count", "5"), ("total", "9")]),
            "5 of 9"
        );
    }

    #[test]
    fn favorite_rows_share_one_stagger() {
        assert_eq!(row_progress(0.0, 0), 0.0);
        // Терминальный вход показывает все строки целиком, поэтому reduce motion
        // не нуждается в отдельном выключателе списка.
        for index in [0usize, 1, 7, 40] {
            assert_eq!(row_progress(1.0, index), 1.0, "строка {index}");
        }
        assert!(row_progress(0.4, 0) > row_progress(0.4, 3));
        assert_eq!(row_progress(f32::NAN, 2), 0.0);
    }

    #[test]
    fn disc_color_darkens_only_while_paused() {
        let palette = Palette::for_ui(egui::Theme::Dark, AccentColor::Blue);
        assert_eq!(disc_fill(&palette, 1.0), palette.accent);
        let idle = disc_fill(&palette, 0.0);
        assert_eq!(idle, faded(palette.accent, IDLE_DIM));
        let middle = disc_fill(&palette, 0.5);
        assert!(f32::from(idle.a()) < f32::from(middle.a()));
        assert!(f32::from(middle.a()) < f32::from(palette.accent.a()));

        // Сквозь общий фэйд входа диск не светится ярче самой капсулы.
        let shell = faded_palette(&palette, 0.5);
        assert_eq!(disc_fill(&shell, 1.0), shell.accent);
        for progress in [0.0, 0.5, 1.0] {
            assert!(f32::from(disc_fill(&shell, progress).a()) <= f32::from(shell.accent.a()));
        }
    }

    /// Один кадр перехода с фиксированным временем — тот же приём, что в тестах
    /// ядра: прогресс живёт в `motion.keyed`, который владеет окном.
    fn drive(
        ctx: &egui::Context,
        transitions: &mut animation::Transitions,
        playing: bool,
        seconds: f64,
    ) -> f32 {
        let input = egui::RawInput {
            time: Some(seconds),
            predicted_dt: 1.0 / 60.0,
            ..Default::default()
        };
        let mut progress = -1.0;
        let output = ctx.run_pass(input, |ctx| {
            let mut motion = transitions.frame(
                ctx,
                animation::ZoneFlags {
                    capsules: true,
                    favorites: true,
                    listening: true,
                    ..Default::default()
                },
                false,
            );
            progress = play_progress(ctx, &mut motion, PLAY_TOP, playing);
        });
        output.drop_without_applying_deltas();
        progress
    }

    #[test]
    fn play_color_carries_its_state_between_frames() {
        let ctx = egui::Context::default();
        let mut transitions = animation::Transitions::new();
        // Первый кадр сидирует паузу: цвет не прыгает на старте окна.
        assert_eq!(drive(&ctx, &mut transitions, true, 0.0), 0.0);

        let mut previous = -1.0;
        let mut last = 0.0;
        for step in 1..=20u32 {
            let value = drive(&ctx, &mut transitions, true, f64::from(step) / 60.0);
            assert!(value >= previous, "цвет идёт назад на шаге {step}");
            previous = value;
            last = value;
        }
        assert!(last > 0.99, "переход цвета не завершился: {last}");
    }

    #[test]
    fn reduce_motion_yields_the_final_play_color() {
        let ctx = egui::Context::default();
        let mut transitions = animation::Transitions::new();
        let mut motion = transitions.frame(&ctx, animation::ZoneFlags::default(), true);
        assert_eq!(play_progress(&ctx, &mut motion, PLAY_TOP, true), 1.0);
        assert_eq!(play_progress(&ctx, &mut motion, PLAY_TOP, false), 0.0);
        assert_eq!(pop_scale(motion.capsules, motion.reduce), 1.0);
    }
}
