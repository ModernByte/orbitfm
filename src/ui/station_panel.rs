//! Правая плавающая панель списка станций (эскиз «Orbital Desk»).
//!
//! Движение панели: каркас стекла гасит и смещает `app.rs`, здесь гаснет
//! содержимое (тот же stagger-индекс), а смена вкладки Results/Favorites/History
//! идёт отдельным переходом [`animation::TAB`] — fade + slide по
//! [`animation::SLIDE_X`] (ADR-0032 §4).

use eframe::egui;

use crate::geo::Country;
use crate::i18n;
use crate::services::Station;
use crate::session::LibraryTab;
use crate::settings::Language;

use super::animation;
use super::icons;
use super::images::FaviconCache;
use super::mode_switch::genre_label;
use super::palette::{Palette, mix_colors};
use super::station_row::{RowContext, station_row, station_tag_chips};
use super::widgets::{
    EmptyMark, EmptyState, contrast_color, control_motion, empty_state, error_card, faded,
    faded_group, focus_ring, quiet_button, search_field, segmented_tabs, skeleton_rows,
};
use super::{UiAction, single_line, truncate};
use crate::station_filter::{self, CountryFilterState, GenreTag, StationSort};

/// Корень именованного перехода вкладок. `Id::unique` детерминирован, поэтому
/// прогресс переживает кадры и не зависит от порядка аллокаций.
const TAB_KEY: &str = "orbitfm_panel_tab";
/// Число вкладок: столько ключей живёт в `PresenceStore`.
const TAB_COUNT: usize = 3;
/// Высота закреплённой кнопки «Очистить историю» вместе с отступом.
const CLEAR_HISTORY_HEIGHT: f32 = 38.0;
/// Высота закреплённой кнопки «Загрузить ещё» вместе с отступом.
const LOAD_MORE_HEIGHT: f32 = 38.0;
/// Промежуток между полем поиска и иконками фильтров.
const FILTER_GAP: f32 = 8.0;

/// Оформление карточки списка. В Focus панель — самостоятельная карточка со
/// своим заголовком; в выпадающей панели шапку рисует `hud::panel_header`, и
/// список начинается сразу с вкладок (ADR-0033).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PanelChrome {
    Full,
    Embedded,
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
    /// Всего станций страны по мировому кэшу: «100 из 1244».
    pub total_available: Option<u32>,
    /// Идёт догрузка следующей страницы, список остаётся на экране.
    pub loading_more: bool,
    /// У текущей выборки есть ещё страницы.
    pub has_more: bool,
    pub filter: &'a mut CountryFilterState,
    pub favicons: &'a mut FaviconCache,
    pub playing_uuid: Option<&'a str>,
    /// Выбранная станция: маркер глобуса и строка списка делят одно выделение
    /// (ADR-0036).
    pub selected_uuid: Option<&'a str>,
    /// Разовая просьба прокрутить строку в центр после клика по маркеру.
    pub reveal_uuid: Option<&'a str>,
    /// Альфа содержимого: вход Focus-стола или появление выпадающей панели.
    pub entry: f32,
    pub chrome: PanelChrome,
    /// Разрешённый язык интерфейса (ADR-0030).
    pub language: Language,
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

pub(crate) fn show(
    ui: &mut egui::Ui,
    palette: &Palette,
    motion: &mut animation::FrameMotion<'_>,
    state: PanelState<'_>,
) -> Vec<UiAction> {
    let mut actions = Vec::new();

    let PanelState {
        tab,
        country,
        genre,
        markers_total,
        results,
        favorites,
        history,
        pending,
        empty,
        error,
        total_available,
        loading_more,
        has_more,
        filter,
        favicons,
        playing_uuid,
        selected_uuid,
        reveal_uuid,
        entry: shown,
        chrome,
        language,
    } = state;
    let embedded = chrome == PanelChrome::Embedded;
    let reduce = motion.reduce;
    let text = i18n::catalog(language);

    palette.panel(16).show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        // Содержимое входит синхронно со стеклом панели и отдельно от неё не
        // смещается: сдвиг каркаса делает `app.rs`, хиттест остаётся на
        // исходных прямоугольниках (ADR-0032 §7).
        faded_group(ui, shown, |ui| {
            if !embedded {
                draw_header(
                    ui,
                    palette,
                    country,
                    genre,
                    markers_total,
                    language,
                    &mut actions,
                );
            }

            let context_mode = country.is_some() || genre.is_some();
            let filtered: Vec<&Station> = if context_mode {
                station_filter::filtered_results(results, filter)
            } else {
                results.iter().collect()
            };
            let total = match *tab {
                LibraryTab::Results => results.len(),
                LibraryTab::Favorites => favorites.len(),
                LibraryTab::History => history.len(),
            };

            // Вкладки видны всегда — даже на пустом списке: это карта разделов,
            // а не заголовок содержимого. Счётчики переехали в подпись под ними.
            let selected_before = tab_index(*tab);
            let mut tab_progress = [0.0; TAB_COUNT];
            for (index, progress) in tab_progress.iter_mut().enumerate() {
                *progress = animation::alpha(motion.keyed.value(
                    ui.ctx(),
                    tab_id(index),
                    index == selected_before,
                    animation::TAB,
                    reduce,
                ));
            }
            let labels = [text.panel.results, text.panel.favorites, text.panel.history];
            if let Some(clicked) = segmented_tabs(
                ui,
                palette,
                "station_tabs",
                &labels,
                selected_before,
                reduce,
            ) {
                *tab = tab_from_index(clicked);
            }

            // Вкладка, выбранная этим кликом, начинает slide-in с нуля, а не
            // дозавершает переход соседней.
            let selected = tab_index(*tab);
            let tab_progress = if selected == selected_before {
                tab_progress[selected]
            } else {
                0.0
            };
            let compact = filter.compact_rows;

            if total > 0 {
                ui.add_space(4.0);
                draw_meta(
                    ui,
                    palette,
                    *tab,
                    total,
                    filtered.len(),
                    context_mode,
                    total_available,
                    filter,
                    language,
                );
            }
            ui.add_space(6.0);

            if context_mode && *tab == LibraryTab::Results && !results.is_empty() {
                draw_local_filters(ui, palette, country, filter, language);
                ui.add_space(6.0);
            }

            let show_clear = *tab == LibraryTab::History && !history.is_empty();
            let show_load_more = *tab == LibraryTab::Results
                && context_mode
                && has_more
                && error.is_none()
                && !pending;
            let reserve = if show_clear {
                CLEAR_HISTORY_HEIGHT
            } else {
                0.0
            } + if show_load_more {
                LOAD_MORE_HEIGHT
            } else {
                0.0
            };
            let viewport = (ui.available_height() - reserve).max(80.0);
            egui::ScrollArea::vertical()
                .id_salt("station_panel_scroll")
                .auto_shrink([false, false])
                .max_height(viewport)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    // Slide относится только к отрисовке: скролл-область и её
                    // id остаются за трансформацией, иначе смещение сбросило бы
                    // прокрутку списка.
                    slide_body(ui, tab_offset(tab_progress), |ui| match *tab {
                        LibraryTab::Results => {
                            if let Some(error) = error {
                                ui.add_space(4.0);
                                if error_card(
                                    ui,
                                    palette,
                                    text.panel.search_failed,
                                    error,
                                    text.common.retry,
                                ) {
                                    actions.push(UiAction::RetrySearch);
                                }
                            } else if pending {
                                skeleton_rows(
                                    ui,
                                    palette,
                                    compact,
                                    viewport,
                                    text.panel.searching,
                                    reduce,
                                );
                            } else if results.is_empty() {
                                let state = if empty {
                                    EmptyState {
                                        mark: EmptyMark::Search,
                                        title: text.panel.no_stations,
                                        hint: Some(text.panel.intro),
                                        action: None,
                                    }
                                } else {
                                    EmptyState {
                                        mark: EmptyMark::Globe,
                                        title: text.panel.pick_country_or_genre,
                                        hint: Some(text.panel.intro),
                                        action: None,
                                    }
                                };
                                empty_state(ui, palette, state, viewport);
                            } else if filtered.is_empty() {
                                let reset = empty_state(
                                    ui,
                                    palette,
                                    EmptyState {
                                        mark: EmptyMark::Filter,
                                        title: text.panel.quiet_sector,
                                        hint: Some(text.panel.no_filter_match),
                                        action: Some(text.panel.reset_filters),
                                    },
                                    viewport,
                                );
                                if reset {
                                    filter.reset();
                                }
                            } else {
                                draw_rows(
                                    ui,
                                    palette,
                                    &filtered,
                                    playing_uuid,
                                    selected_uuid,
                                    reveal_uuid,
                                    compact,
                                    context_mode,
                                    favorites,
                                    language,
                                    favicons,
                                    &mut actions,
                                );
                            }
                        }
                        LibraryTab::Favorites => {
                            if favorites.is_empty() {
                                empty_state(
                                    ui,
                                    palette,
                                    EmptyState {
                                        mark: EmptyMark::Star,
                                        title: text.panel.favorites_empty,
                                        hint: Some(text.capsule.favorite_hint),
                                        action: None,
                                    },
                                    viewport,
                                );
                            } else {
                                let rows: Vec<&Station> = favorites.iter().collect();
                                draw_rows(
                                    ui,
                                    palette,
                                    &rows,
                                    playing_uuid,
                                    selected_uuid,
                                    reveal_uuid,
                                    compact,
                                    false,
                                    favorites,
                                    language,
                                    favicons,
                                    &mut actions,
                                );
                            }
                        }
                        LibraryTab::History => {
                            if history.is_empty() {
                                empty_state(
                                    ui,
                                    palette,
                                    EmptyState {
                                        mark: EmptyMark::Clock,
                                        title: text.panel.history_empty,
                                        hint: None,
                                        action: None,
                                    },
                                    viewport,
                                );
                            } else {
                                let rows: Vec<&Station> = history.iter().collect();
                                draw_rows(
                                    ui,
                                    palette,
                                    &rows,
                                    playing_uuid,
                                    selected_uuid,
                                    reveal_uuid,
                                    compact,
                                    false,
                                    favorites,
                                    language,
                                    favicons,
                                    &mut actions,
                                );
                            }
                        }
                    });
                });

            // Догрузка закреплена под списком: длинная страница не уводит
            // кнопку за пределы прокрутки.
            if show_load_more {
                ui.add_space(4.0);
                let (label, color) = if loading_more {
                    (text.panel.loading_more, palette.muted)
                } else {
                    (text.panel.load_more, palette.accent)
                };
                if quiet_button(ui, palette, label, color).clicked() && !loading_more {
                    actions.push(UiAction::LoadMore);
                }
            }

            // Очистка истории закреплена под списком: длинная история не
            // уводит действие за пределы прокрутки.
            if show_clear {
                ui.add_space(4.0);
                if quiet_button(ui, palette, text.panel.clear_history, palette.error).clicked() {
                    actions.push(UiAction::ClearHistory);
                }
            }
        });
    });

    actions
}

/// Шапка самостоятельной карточки Focus: заголовок, контекст и счётчик карты.
fn draw_header(
    ui: &mut egui::Ui,
    palette: &Palette,
    country: Option<&Country>,
    genre: Option<&str>,
    markers_total: usize,
    language: Language,
    actions: &mut Vec<UiAction>,
) {
    let text = i18n::catalog(language);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(text.panel.stations)
                .size(15.0)
                .strong()
                .color(palette.text),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::close_button(ui, palette, text.common.hide_station_list).clicked() {
                actions.push(UiAction::TogglePanel);
            }
        });
    });

    if let Some(country) = country {
        let name = single_line(&country.name);
        let name = if name.is_empty() {
            country.iso_a2.clone()
        } else {
            name
        };
        let title = i18n::fill(text.panel.country_prefix, &[("name", &name)]);
        ui.label(
            egui::RichText::new(title)
                .size(12.0)
                .color(palette.accent)
                .strong(),
        );
    } else if let Some(genre) = genre {
        // Известные теги подписывает `mode_switch`, здесь локализуется префикс.
        let tag = genre_label(language, genre);
        ui.label(
            egui::RichText::new(i18n::fill(text.panel.genre_prefix, &[("genre", &tag)]))
                .size(12.0)
                .color(palette.accent)
                .strong(),
        );
    }
    if markers_total > 0 {
        let count = markers_total.to_string();
        ui.label(
            egui::RichText::new(i18n::fill(text.panel.on_map, &[("count", &count)]))
                .size(11.0)
                .color(palette.muted),
        );
    }
}

/// Подпись под вкладками: «N из M» при фильтрах или неполной загрузке, иначе
/// число станций, плюс сортировка и плотность строк. Счётчики в самих вкладках
/// больше не живут.
#[allow(clippy::too_many_arguments)]
fn draw_meta(
    ui: &mut egui::Ui,
    palette: &Palette,
    tab: LibraryTab,
    total: usize,
    shown_count: usize,
    context_mode: bool,
    total_available: Option<u32>,
    filter: &mut CountryFilterState,
    language: Language,
) {
    let text = i18n::catalog(language);
    let caption = if tab == LibraryTab::Results && context_mode && filter.is_active() {
        i18n::fill(
            text.panel.results_of,
            &[
                ("count", &shown_count.to_string()),
                ("total", &total.to_string()),
            ],
        )
    } else if tab == LibraryTab::Results && context_mode {
        // Страна целиком больше загруженной страницы: «100 из 1244» вместо
        // молчаливого обрезания списка (ADR-0036, ревизия 2026-10-28).
        total_available
            .filter(|available| *available as usize > total)
            .map_or_else(
                || i18n::fill(text.panel.station_count, &[("count", &total.to_string())]),
                |available| {
                    i18n::fill(
                        text.panel.loaded_of,
                        &[
                            ("loaded", &total.to_string()),
                            ("total", &available.to_string()),
                        ],
                    )
                },
            )
    } else {
        i18n::fill(text.panel.station_count, &[("count", &total.to_string())])
    };
    let sortable = tab == LibraryTab::Results && context_mode;

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(caption).size(11.0).color(palette.muted));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::density_button(ui, palette, text.panel.compact, filter.compact_rows).clicked()
            {
                filter.compact_rows = !filter.compact_rows;
            }
            if sortable {
                let response = icons::sort_button(ui, palette, text.panel.sort_by);
                egui::Popup::menu(&response).show(|ui| {
                    for sort in StationSort::ALL {
                        let selected = filter.sort == sort;
                        if ui
                            .selectable_label(selected, sort.label(language))
                            .clicked()
                        {
                            filter.sort = sort;
                            ui.close();
                        }
                    }
                });
            }
        });
    });
}

/// Стабильный Id перехода вкладки: три вкладки — три ключа в `PresenceStore`.
fn tab_id(index: usize) -> egui::Id {
    egui::Id::unique((TAB_KEY, index))
}

/// Горизонтальное смещение содержимого вкладки: `SLIDE_X` пикселей слева в
/// начале перехода, ноль в конце. Отрицательное направление совпадает со
/// сдвигом каркаса панели в `app.rs`.
fn tab_offset(progress: f32) -> f32 {
    -animation::slide_x(animation::alpha(progress))
}

/// Сдвиг только что нарисованного содержимого на `shift` пикселей по X.
///
/// Трансформируются формы, добавленные внутри `add`, — аллокации, хиттест и id
/// виджетов не меняются (ADR-0032 §7). В покое (`shift == 0`) обходимся без
/// прохода по списку форм: `Shape::transform` для текста и мешей копирует данные,
/// поэтому гонять его каждый кадр нельзя.
fn slide_body<T>(ui: &mut egui::Ui, shift: f32, add: impl FnOnce(&mut egui::Ui) -> T) -> T {
    if shift == 0.0 {
        return add(ui);
    }
    let layer = ui.layer_id();
    let start = ui.ctx().graphics(|g| {
        g.get(layer)
            .map_or(egui::layers::ShapeIdx(0), |l| l.next_idx())
    });
    let result = add(ui);
    ui.ctx().graphics_mut(|g| {
        if let Some(list) = g.get_mut(layer) {
            let end = list.next_idx();
            list.transform_range(
                start,
                end,
                egui::emath::TSTransform::from_translation(egui::vec2(shift, 0.0)),
            );
        }
    });
    result
}

/// Строки списка с соседями для навигации стрелками. `tag_chips` рисует
/// жанровые чипы под станцией выбранной страны.
#[allow(clippy::too_many_arguments)]
fn draw_rows(
    ui: &mut egui::Ui,
    palette: &Palette,
    stations: &[&Station],
    playing_uuid: Option<&str>,
    selected_uuid: Option<&str>,
    reveal_uuid: Option<&str>,
    compact: bool,
    tag_chips: bool,
    favorites: &[Station],
    language: Language,
    favicons: &mut FaviconCache,
    actions: &mut Vec<UiAction>,
) {
    for (index, station) in stations.iter().enumerate() {
        let uuid = station.stationuuid.as_str();
        let mut row = RowContext {
            playing: playing_uuid == Some(uuid),
            selected: selected_uuid == Some(uuid),
            reveal: reveal_uuid == Some(uuid),
            compact,
            favorites,
            language,
            favicons: &mut *favicons,
            previous: index
                .checked_sub(1)
                .map(|item| stations[item].stationuuid.as_str()),
            next: stations
                .get(index + 1)
                .map(|item| item.stationuuid.as_str()),
        };
        station_row(ui, palette, station, &mut row, actions);
        if tag_chips && !compact {
            ui.horizontal(|ui| {
                ui.add_space(46.0);
                station_tag_chips(ui, palette, station, actions);
            });
        }
    }
}

/// Локальные фильтры выбранной страны: поиск, кнопки показа/сброса и строка
/// мультивыбора жанров с бейджами выбранных фильтров (ревизия 2026-10-28).
fn draw_local_filters(
    ui: &mut egui::Ui,
    palette: &Palette,
    country: Option<&Country>,
    filter: &mut CountryFilterState,
    language: Language,
) {
    let text = i18n::catalog(language);
    let hint = match country {
        Some(country) => {
            let name = truncate(&single_line(&country.name), 18);
            i18n::fill(text.panel.search_in_country, &[("country", &name)])
        }
        None => text.panel.search_here.to_owned(),
    };
    let search_width =
        (ui.available_width() - 2.0 * icons::SMALL_SIDE - 2.0 * FILTER_GAP).max(120.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = FILTER_GAP;
        search_field(
            ui,
            palette,
            egui::Id::unique("orbitfm_country_filter"),
            &hint,
            &mut filter.text,
            search_width,
        );
        if icons::filters_button(ui, palette, text.panel.filters, filter.filters_visible).clicked()
        {
            filter.filters_visible = !filter.filters_visible;
        }
        if icons::clear_filters_button(ui, palette, filter.is_active(), text.panel.clear_filters)
            .clicked()
        {
            filter.reset();
        }
    });

    if !filter.filters_visible {
        return;
    }

    ui.add_space(6.0);
    genre_selector(ui, palette, filter, language);
    if !filter.genres.is_empty() {
        ui.add_space(6.0);
        let mut removed = Vec::new();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
            for genre in &filter.genres {
                if genre_badge(ui, palette, genre.label(language), text.panel.remove_filter) {
                    removed.push(*genre);
                }
            }
        });
        for genre in removed {
            filter.toggle_genre(genre);
        }
    }
}

/// Выпадающий мультивыбор жанров: кнопка-чип с шевроном и меню, которое не
/// закрывается после каждого выбора.
fn genre_selector(
    ui: &mut egui::Ui,
    palette: &Palette,
    filter: &mut CountryFilterState,
    language: Language,
) {
    let text = i18n::catalog(language);
    let label = match filter.genres.as_slice() {
        [] => text.panel.all_genres.to_owned(),
        [only] => only.label(language).to_owned(),
        selected => i18n::fill(
            text.panel.genres_selected,
            &[("count", &selected.len().to_string())],
        ),
    };
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 28.0), egui::Sense::click());
    let (hover, press) = control_motion(
        ui,
        response.id,
        response.hovered(),
        response.is_pointer_button_down_on(),
    );
    if ui.is_rect_visible(rect) {
        let fill = mix_colors(
            palette.chip_fill,
            palette.accent,
            0.10 * hover + 0.05 * press,
        );
        ui.painter().rect_filled(rect, 8.0, fill);
        ui.painter().rect_stroke(
            rect,
            8.0,
            egui::Stroke::new(1.0, mix_colors(palette.chip_stroke, palette.accent, hover)),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            egui::pos2(rect.left() + 12.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label.as_str(),
            egui::FontId::proportional(11.5),
            palette.text,
        );
        let chevron = egui::pos2(rect.right() - 14.0, rect.center().y);
        let stroke = egui::Stroke::new(
            1.4,
            if filter.genres.is_empty() {
                palette.muted
            } else {
                palette.accent
            },
        );
        ui.painter().line_segment(
            [
                chevron + egui::vec2(-3.5, -2.0),
                chevron + egui::vec2(0.0, 2.0),
            ],
            stroke,
        );
        ui.painter().line_segment(
            [
                chevron + egui::vec2(0.0, 2.0),
                chevron + egui::vec2(3.5, -2.0),
            ],
            stroke,
        );
    }
    focus_ring(ui, &response, rect, 8.0);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, label.as_str()));

    let frame = egui::Frame::popup(ui.style())
        .fill(palette.card_fill)
        .stroke(egui::Stroke::new(1.0, palette.card_stroke));
    egui::Popup::menu(&response)
        .id(response.id.with("genres_menu"))
        .width(width)
        .frame(frame)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(width - 16.0);
            ui.visuals_mut().selection.bg_fill = palette.accent;
            ui.visuals_mut().selection.stroke =
                egui::Stroke::new(1.0, contrast_color(palette.accent));
            ui.label(
                egui::RichText::new(text.panel.genres)
                    .size(11.0)
                    .strong()
                    .color(palette.muted),
            );
            for genre in GenreTag::ALL {
                let selected = filter.has_genre(genre);
                if ui
                    .selectable_label(selected, genre.label(language))
                    .clicked()
                {
                    filter.toggle_genre(genre);
                }
            }
            if !filter.genres.is_empty() {
                ui.separator();
                if ui
                    .selectable_label(false, text.panel.clear_filters)
                    .clicked()
                {
                    filter.genres.clear();
                }
            }
        });
}

/// Бейдж выбранного жанра: клик в любом месте, включая крестик, снимает
/// фильтр. Геометрия фиксирована, hover меняет только цвет (ADR-0032 §7).
fn genre_badge(ui: &mut egui::Ui, palette: &Palette, label: &str, hint: &str) -> bool {
    let color = contrast_color(palette.accent);
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_owned(), egui::FontId::proportional(11.5), color);
    let text_size = galley.size();
    let cross = 10.0;
    let width = text_size.x + 12.0 + cross + 12.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 24.0), egui::Sense::click());
    let (hover, press) = control_motion(
        ui,
        response.id,
        response.hovered(),
        response.is_pointer_button_down_on(),
    );
    if ui.is_rect_visible(rect) {
        let fill = mix_colors(
            palette.accent,
            egui::Color32::WHITE,
            0.10 * hover + 0.05 * press,
        );
        ui.painter().rect_filled(rect, 12.0, fill);
        ui.painter().galley(
            egui::pos2(rect.left() + 12.0, rect.center().y - text_size.y * 0.5),
            galley,
            color,
        );
        let center = egui::pos2(rect.right() - 12.0, rect.center().y);
        let stroke = egui::Stroke::new(1.6, faded(color, 0.75 + 0.25 * hover));
        ui.painter().line_segment(
            [
                center + egui::vec2(-4.0, -4.0),
                center + egui::vec2(4.0, 4.0),
            ],
            stroke,
        );
        ui.painter().line_segment(
            [
                center + egui::vec2(-4.0, 4.0),
                center + egui::vec2(4.0, -4.0),
            ],
            stroke,
        );
    }
    focus_ring(ui, &response, rect, 12.0);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, hint));
    response.on_hover_text(hint).clicked()
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

    #[test]
    fn tab_slide_starts_offset_and_settles_at_zero() {
        assert_eq!(tab_offset(0.0), -animation::SLIDE_X);
        assert_eq!(tab_offset(1.0), 0.0);
        assert_eq!(tab_offset(f32::NAN), -animation::SLIDE_X);
    }
}
