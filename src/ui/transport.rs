//! Нижний transport «Orbital Desk»: what's playing, центральные контролы,
//! громкость, синтетический визуализатор (ADR-0019) и параметры эфира.
//!
//! Из `FrameMotion` transport берёт только вход стола (ADR-0032 §4): каркас
//! панели гасит `app.rs`, здесь гаснет содержимое. Кроме визуализатора ничего
//! своего не анимируется — пульсация полос читается из аудио-каденции.

use eframe::egui::{self, Response, Sense, Stroke, Vec2, pos2};

use crate::audio::PlaybackState;
use crate::i18n;
use crate::render::VisualizerModel;
use crate::services::Station;
use crate::settings::Language;

use super::animation;
use super::icons::{self, heart_button, next_button, prev_button, stop_button};
use super::images::FaviconCache;
use super::palette::Palette;
use super::station_row::row_place;
use super::widgets::{
    avatar, chip, circle_button, eq_bars_in, faded_group, focus_ring, live_badge, viz_bars,
    volume_slider,
};
use super::{UiAction, playback_label, single_line, truncate};

/// Позиция transport в общем stagger входа Focus-стола: зона приходит последней,
/// индексы задает `app.rs`, где тем же значением гаснет каркас панели.
const SCENE_INDEX: usize = 3;

/// Ширина поля визуализатора, px (ТЗ: 40–56 px по высоте; cava-стилю нужна
/// ширина, чтобы столбцы не слипались).
const VIZ_SIZE: (f32, f32) = (156.0, 46.0);
/// Правой группе (визуализатор + чип + громкость) нужно столько места.
const RIGHT_GROUP_WIDTH: f32 = 410.0;
/// Окно уже — просим компактный чип рулетки (кубик без подписи).
const COMPACT_ROW_WIDTH: f32 = 1100.0;
/// Запас левой группы под контролы: LIVE, эквалайзер, пять кнопок и рулетка.
const LEFT_CONTROLS_WIDTH: (f32, f32) = (320.0, 430.0);
/// Средняя ширина глифа в подписи 13.5 px — грубая оценка для обрезки по размеру.
const NAME_CHAR_WIDTH: f32 = 7.2;
/// Сколько места нужно строке параметров эфира, чтобы не толкаться с контролами.
const SUMMARY_MIN_WIDTH: f32 = 150.0;
/// Зазор между левой и правой группами transport.
const ROW_GAP: f32 = 12.0;

pub(crate) struct TransportState<'a> {
    pub playback: &'a PlaybackState,
    pub current: Option<&'a Station>,
    pub favorite: bool,
    pub can_resume: bool,
    pub spectrum_enabled: bool,
    pub roulette_pending: bool,
    pub volume: f64,
    pub visualizer: &'a VisualizerModel,
    pub favicons: &'a mut FaviconCache,
    /// Разрешённый язык интерфейса (ADR-0030).
    pub language: Language,
}

pub(crate) fn show(
    ui: &mut egui::Ui,
    palette: &Palette,
    motion: &mut animation::FrameMotion<'_>,
    state: TransportState<'_>,
) -> Vec<UiAction> {
    let mut actions = Vec::new();

    let playing = matches!(
        state.playback,
        PlaybackState::Playing | PlaybackState::Buffering | PlaybackState::Reconnecting
    );
    let favicon = state
        .current
        .and_then(|station| station.favicon.as_deref())
        .unwrap_or_default();
    let texture = state.favicons.texture(favicon);
    let current = state.current;
    let playback: &PlaybackState = state.playback;
    let spectrum_enabled = state.spectrum_enabled;
    let visualizer = state.visualizer;
    let roulette_pending = state.roulette_pending;
    let can_resume = state.can_resume;
    let favorite = state.favorite;
    let language = state.language;
    let text = i18n::catalog(language);
    let mut volume = state.volume;
    let shown = content_alpha(motion.scene);

    palette.panel(16).show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        // Содержимое transport входит тем же stagger, что и каркас панели из
        // `app.rs`: гасим группой, чтобы иконки и слайдер брали прозрачность из
        // `visuals` (ADR-0032 §7 — смещений и пересчёта аллокаций нет).
        faded_group(ui, shown, |ui| {
            // Левая и правая группы получают непересекающиеся прямоугольники: в одной
            // строке длинное название станции иначе выезжает на громкость и
            // визуализатор, а высокий слот уползает за нижний край панели.
            let row = ui.available_rect_before_wrap();
            let compact = row.width() < COMPACT_ROW_WIDTH;
            let right_width = RIGHT_GROUP_WIDTH.min(row.width() * 0.45);
            let right_rect =
                egui::Rect::from_min_max(pos2(row.right() - right_width, row.top()), row.max);
            let left_rect =
                egui::Rect::from_min_max(row.min, pos2(right_rect.left() - ROW_GAP, row.bottom()));

            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(left_rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                |ui| {
                    avatar(
                        ui,
                        palette,
                        texture,
                        current.map(|station| station.name.as_str()).unwrap_or("?"),
                        44.0,
                    );
                    // Подпись режем по остатку ширины, с запасом на контролы справа от неё.
                    let controls = if compact {
                        LEFT_CONTROLS_WIDTH.0
                    } else {
                        LEFT_CONTROLS_WIDTH.1
                    };
                    let name_chars = (((ui.available_width() - controls) / NAME_CHAR_WIDTH)
                        .clamp(10.0, 36.0)) as usize;
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        let name = current
                            .map(|station| single_line(&station.name))
                            .filter(|name| !name.is_empty())
                            .unwrap_or_else(|| text.common.nothing_playing.to_owned());
                        ui.label(
                            egui::RichText::new(truncate(&name, name_chars))
                                .size(13.5)
                                .strong()
                                .color(if playing {
                                    palette.accent
                                } else {
                                    palette.text
                                }),
                        );
                        let place = current.map(row_place).unwrap_or_default();
                        if !place.is_empty() {
                            ui.label(
                                egui::RichText::new(truncate(&place, name_chars))
                                    .size(10.5)
                                    .color(palette.muted),
                            );
                        }
                        ui.label(
                            egui::RichText::new(now_playing_line(
                                language, current, playback, playing,
                            ))
                            .size(10.5)
                            .color(palette.muted),
                        );
                    });

                    ui.add_space(20.0);
                    live_badge(ui, palette, playing, language);
                    if playing {
                        let (bars, _) =
                            ui.allocate_exact_size(Vec2::new(26.0, 18.0), Sense::hover());
                        eq_bars_in(ui, palette, bars, 5, true);
                    }

                    ui.add_space(12.0);
                    let has_station = current.is_some();
                    if prev_button(ui, has_station, language).clicked() {
                        actions.push(UiAction::PlayRelative(-1));
                    }
                    let hint = if playing {
                        text.common.pause
                    } else {
                        text.common.resume
                    };
                    if circle_button(ui, palette, playing, playing || can_resume, hint).clicked() {
                        actions.push(UiAction::TogglePlayback);
                    }
                    if next_button(ui, has_station, language).clicked() {
                        actions.push(UiAction::PlayRelative(1));
                    }
                    let stop_enabled = !matches!(playback, PlaybackState::Stopped);
                    if stop_button(ui, stop_enabled, language).clicked() {
                        actions.push(UiAction::Stop);
                    }
                    if let Some(station) = current {
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
                        if heart_button(ui, favorite, color, hint).clicked() {
                            actions.push(UiAction::ToggleFavorite(station.clone()));
                        }
                    }
                    if ui
                        .add(RouletteButton {
                            palette,
                            enabled: !roulette_pending,
                            pending: roulette_pending,
                            compact,
                            language,
                        })
                        .clicked()
                    {
                        actions.push(UiAction::Roulette);
                    }
                },
            );

            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(right_rect)
                    .layout(egui::Layout::right_to_left(egui::Align::Center)),
                |ui| {
                    // Полосу выделяем на всю высоту строки и центрируем столбцы сами:
                    // в right-to-left ряду `available_height()` считает слот от верха
                    // строки, и высокий виджет уезжает за нижний край панели.
                    let viz_width = VIZ_SIZE.0.min(right_width);
                    let (slot, _) = ui.allocate_exact_size(
                        Vec2::new(viz_width, right_rect.height()),
                        Sense::hover(),
                    );
                    let rect = egui::Rect::from_center_size(
                        pos2(slot.center().x, right_rect.center().y),
                        Vec2::new(viz_width, VIZ_SIZE.1.min(right_rect.height())),
                    );
                    viz_bars(ui, palette, rect, visualizer.levels(), visualizer.peaks());
                    if spectrum_enabled && playing {
                        ui.ctx()
                            .request_repaint_after(std::time::Duration::from_millis(33));
                    }

                    let mut enabled = spectrum_enabled;
                    if chip(ui, palette, text.transport.visualizer, enabled).clicked() {
                        enabled = !enabled;
                        actions.push(UiAction::ToggleSpectrum(enabled));
                    }
                    let slider = volume_slider(ui, palette, &mut volume, language);
                    if slider.changed && !slider.committed {
                        actions.push(UiAction::PreviewVolume(volume));
                    }
                    if slider.committed {
                        actions.push(UiAction::CommitVolume(volume));
                    }
                    if ui.available_width() > SUMMARY_MIN_WIDTH {
                        ui.label(
                            egui::RichText::new(stream_summary(language, current))
                                .size(11.0)
                                .color(palette.muted),
                        );
                    }
                },
            );
        });
    });

    actions
}

/// Альфа содержимого transport по прогрессу входа стола.
///
/// Transport — самая поздняя зона Focus-стола, индекс [`SCENE_INDEX`] совпадает
/// с тем, под которым `app.rs` гасит каркас панели. При reduce motion `scene`
/// уже единица, и погашения нет.
fn content_alpha(scene: f32) -> f32 {
    animation::alpha(animation::stagger(SCENE_INDEX, scene))
}

/// Кнопка «Radio Roulette» с кубиком; в узкой строке — только кубик.
struct RouletteButton<'a> {
    palette: &'a Palette,
    enabled: bool,
    pending: bool,
    compact: bool,
    /// Разрешённый язык подписи и AccessKit-метки (ADR-0030).
    language: Language,
}

impl egui::Widget for RouletteButton<'_> {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let text = i18n::catalog(self.language);
        let label = if self.pending {
            text.transport.rolling
        } else if self.compact {
            ""
        } else {
            text.transport.radio_roulette
        };
        let color = if self.enabled {
            self.palette.text
        } else {
            self.palette.muted
        };
        let galley =
            ui.painter()
                .layout_no_wrap(label.to_owned(), egui::FontId::proportional(12.5), color);
        let text_width = galley.size().x;
        let size = if label.is_empty() {
            Vec2::new(34.0, 26.0)
        } else {
            Vec2::new(text_width + 38.0, 26.0)
        };
        let sense = if self.enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(size, sense);
        if ui.is_rect_visible(rect) {
            let fill = if response.hovered() && self.enabled {
                self.palette.accent
            } else {
                self.palette.chip_fill
            };
            ui.painter().rect(
                rect,
                egui::CornerRadius::same(13),
                fill,
                Stroke::new(1.0, self.palette.chip_stroke),
                egui::StrokeKind::Middle,
            );
            let dice_center = if label.is_empty() {
                rect.center()
            } else {
                pos2(rect.left() + 16.0, rect.center().y)
            };
            icons::draw_dice(ui.painter(), dice_center, 8.0, color);
            if !label.is_empty() {
                ui.painter().galley(
                    pos2(rect.left() + 30.0, rect.center().y - galley.size().y * 0.5),
                    galley,
                    if response.hovered() && self.enabled {
                        super::widgets::contrast_color(self.palette.accent)
                    } else {
                        color
                    },
                );
            }
            focus_ring(ui, &response, rect, 13.0);
        }
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::Button,
                self.enabled,
                text.transport.random_station,
            )
        });
        if self.enabled {
            response.on_hover_text(text.transport.random_station)
        } else {
            response
        }
    }
}

fn now_playing_line(
    language: Language,
    station: Option<&Station>,
    playback: &PlaybackState,
    playing: bool,
) -> String {
    let text = i18n::catalog(language);
    match station {
        Some(station) if playing => {
            let tags: Vec<&str> = station
                .tags
                .split(',')
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .take(2)
                .collect();
            if tags.is_empty() {
                text.transport.now_playing.to_owned()
            } else {
                let joined = tags.join(", ");
                i18n::fill(text.transport.now_playing_tags, &[("tags", &joined)])
            }
        }
        Some(_) => playback_label(language, playback),
        None => text.transport.no_station.to_owned(),
    }
}

fn stream_summary(language: Language, station: Option<&Station>) -> String {
    let Some(station) = station else {
        return String::new();
    };
    let text = i18n::catalog(language);
    // Кодек — данные Radio Browser, он не переводится; битрейт собирается из
    // шаблона каталога.
    let codec = single_line(&station.codec);
    let bitrate = if station.bitrate > 0 {
        let count = station.bitrate.to_string();
        i18n::fill(text.transport.kbps, &[("count", &count)])
    } else {
        String::new()
    };
    [codec, bitrate]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("  ")
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
            tags: "ambient, new age".to_owned(),
            countrycode: "US".to_owned(),
            language: "english".to_owned(),
            votes: 120,
            country: "United States".to_owned(),
            state: "New York".to_owned(),
            bitrate: 128,
            codec: "AAC".to_owned(),
            geo_lat: None,
            geo_long: None,
        }
    }

    #[test]
    fn now_playing_line_lists_first_tags_when_playing() {
        let english = i18n::catalog(Language::English);
        assert_eq!(
            now_playing_line(
                Language::English,
                Some(&station()),
                &PlaybackState::Playing,
                true
            ),
            "Now Playing: ambient, new age"
        );
        assert_eq!(
            now_playing_line(Language::English, None, &PlaybackState::Stopped, false),
            "No station yet"
        );
        assert_eq!(english.transport.no_station, "No station yet");
        // Теги подставляются в шаблон каталога, а не в литерал.
        assert_eq!(
            now_playing_line(
                Language::English,
                Some(&station()),
                &PlaybackState::Playing,
                true
            ),
            i18n::fill(
                english.transport.now_playing_tags,
                &[("tags", "ambient, new age")]
            )
        );
    }

    #[test]
    fn now_playing_line_without_tags_keeps_plain_caption() {
        let english = i18n::catalog(Language::English);
        let mut quiet = station();
        quiet.tags = "  ".to_owned();
        assert_eq!(
            now_playing_line(
                Language::English,
                Some(&quiet),
                &PlaybackState::Playing,
                true
            ),
            english.transport.now_playing,
            "подпись без тегов берётся из каталога"
        );
        assert_eq!(
            now_playing_line(
                Language::English,
                Some(&station()),
                &PlaybackState::Paused,
                false
            ),
            "paused"
        );
    }

    #[test]
    fn transport_labels_come_from_the_catalog() {
        // Любой разрешённый язык обязан отдавать непустые строки транспорта.
        for language in [Language::English, Language::Russian] {
            let text = i18n::catalog(language);
            for value in [
                text.transport.radio_roulette,
                text.transport.rolling,
                text.transport.random_station,
                text.transport.visualizer,
                text.transport.now_playing,
                text.transport.now_playing_tags,
                text.transport.no_station,
                text.transport.kbps,
                text.common.pause,
                text.common.resume,
                text.common.add_favorite,
                text.common.remove_favorite,
                text.common.nothing_playing,
            ] {
                assert!(!value.trim().is_empty(), "пустая строка на {language:?}");
            }
        }
    }

    #[test]
    fn stream_summary_joins_codec_and_bitrate() {
        assert_eq!(
            stream_summary(Language::English, Some(&station())),
            "AAC  128 kbps"
        );
        let mut quiet = station();
        quiet.bitrate = 0;
        quiet.codec = " ".to_owned();
        assert_eq!(stream_summary(Language::English, Some(&quiet)), "");
        assert_eq!(stream_summary(Language::English, None), "");
    }

    #[test]
    fn content_fade_follows_the_shared_scene_stagger() {
        assert_eq!(content_alpha(0.0), 0.0);
        assert_eq!(content_alpha(1.0), 1.0);
        for scene in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert_eq!(
                content_alpha(scene),
                animation::alpha(animation::stagger(SCENE_INDEX, scene)),
                "прозрачность содержимого разошлась с каркасом панели"
            );
        }
    }

    #[test]
    fn transport_enters_after_the_hud() {
        // Индекс 3 против HUD-нулевого: на середине входа транспорт ещё тише.
        let middle = 0.5;
        assert!(content_alpha(middle) < animation::alpha(animation::stagger(0, middle)));
        assert_eq!(content_alpha(f32::NAN), 0.0);
    }
}
