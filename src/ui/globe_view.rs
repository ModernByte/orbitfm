//! 3D-глобус: ввод камеры, маркеры, спектр и подсказки.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use eframe::egui;

use crate::geo::{Country, GeoService};
use crate::render::{
    CountryStyle, GlobeCamera, GlobeInput, MarkerColors, MarkerVisual, SpectrumOverlay,
    SpectrumTarget, paint_country, paint_globe, paint_markers, sphere_point,
};
use crate::services::Station;

use super::layout::DeskLayout;
use super::palette::{Palette, heat_color};
use super::station_row::row_place;
use super::widgets::{faded, flag_emoji, glass_panel};
use super::{UiAction, single_line, truncate};

const MARKER_CLICK_RADIUS: f32 = 10.0;
const MARKER_GROUP_SCALE: f64 = 100.0;
const SPECTRUM_REPAINT: std::time::Duration = std::time::Duration::from_millis(33);
const COUNTRY_HEAT_MIN_ALPHA: u8 = 26;
const COUNTRY_HEAT_MAX_ALPHA: u8 = 96;
const COUNTRY_HOVER_ALPHA: u8 = 120;
const COUNTRY_HOVER_FILL_ALPHA: u8 = 30;
const STAR_COUNT: usize = 160;
/// Полосы вертикального градиента космоса и глубина виньетки (ТЗ §3.1, §5.1 п. 5).
const SPACE_BANDS: usize = 12;
const VIGNETTE_BANDS: usize = 6;
/// Длительность fade-in/out callout (ТЗ §1.2: 120–180 мс).
const CALLOUT_FADE_SECONDS: f32 = 0.15;
const CALLOUT_HOVER_ALPHA: f32 = 0.85;
/// Расстояние от якоря до карточки — длина leader line.
const CALLOUT_LEADER: f32 = 26.0;
/// Смещение карточки от якоря на время появления.
const CALLOUT_SLIDE: f32 = 6.0;
const CALLOUT_ROUNDING: f32 = 11.0;
const CALLOUT_EPSILON: f32 = 0.004;
const CALLOUT_REPAINT: std::time::Duration = std::time::Duration::from_millis(16);

/// Звезда фона: нормализованные координаты 0..1 и яркость.
#[derive(Clone, Copy)]
struct Star {
    position: egui::Pos2,
    size: f32,
    brightness: u8,
}

/// Детерминированный звёздный фон: считается один раз, за кадр — только масштаб.
fn starfield() -> &'static [Star] {
    static STARS: OnceLock<Box<[Star]>> = OnceLock::new();
    STARS.get_or_init(|| {
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        (0..STAR_COUNT)
            .map(|_| Star {
                position: egui::pos2(
                    next() as f32 / u64::MAX as f32,
                    next() as f32 / u64::MAX as f32,
                ),
                size: 0.5 + next() as f32 / u64::MAX as f32,
                brightness: 24 + (next() % 96) as u8,
            })
            .collect()
    })
}

/// Космос: не плоский чёрный, а лёгкий вертикальный градиент (ТЗ §3.1).
fn paint_space(painter: &egui::Painter, rect: egui::Rect, palette: &Palette) {
    if !rect.is_finite() || rect.height() <= 0.0 {
        return;
    }
    let band = rect.height() / SPACE_BANDS as f32;
    for index in 0..SPACE_BANDS {
        let top = rect.top() + band * index as f32;
        let bottom = if index + 1 == SPACE_BANDS {
            rect.bottom()
        } else {
            top + band
        };
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), top),
                egui::pos2(rect.right(), bottom),
            ),
            0.0,
            super::palette::mix_colors(
                palette.space_top,
                palette.space_bottom,
                (index as f32 + 0.5) / SPACE_BANDS as f32,
            ),
        );
    }
}

/// Виньетка: полутёмные полосы по краям, чтобы центр читался сценой.
fn paint_vignette(painter: &egui::Painter, rect: egui::Rect) {
    let shortest = rect.width().min(rect.height());
    if !rect.is_finite() || shortest <= 0.0 {
        return;
    }
    let depth = (shortest * 0.10) / VIGNETTE_BANDS as f32;
    for band in 0..VIGNETTE_BANDS {
        let falloff = 1.0 - band as f32 / VIGNETTE_BANDS as f32;
        painter.rect_stroke(
            rect.shrink(depth * (band as f32 + 0.5)),
            0.0,
            egui::Stroke::new(
                depth,
                egui::Color32::from_black_alpha((34.0 * falloff * falloff).round() as u8),
            ),
            egui::StrokeKind::Middle,
        );
    }
}

fn paint_starfield(painter: &egui::Painter, rect: egui::Rect) {
    for star in starfield() {
        let position = egui::pos2(
            rect.left() + star.position.x * rect.width(),
            rect.top() + star.position.y * rect.height(),
        );
        painter.circle_filled(
            position,
            star.size,
            egui::Color32::from_gray(star.brightness),
        );
    }
}

/// Станция (или группа станций), отображаемая на глобусе.
#[derive(Debug, Clone)]
pub(crate) struct GlobeMarker {
    pub station: Station,
    pub lat: f32,
    pub lon: f32,
    pub count: usize,
    /// Сигнал популярности (максимум голосов по группе): только размер маркера.
    pub popularity: f32,
    /// Лучший битрейт по группе: кодирует цвет маркера (ТЗ §2.3).
    pub bitrate: u32,
    /// UUID всех станций группы — играющей считается любая из них.
    pub members: Vec<String>,
}

/// Callout переживает кадры: у карточки своя прозрачность и цель.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CalloutCard {
    iso: String,
    name: String,
    /// Текущая прозрачность 0..1 (умножается на цвета карточки).
    alpha: f32,
    target: f32,
}

/// Состояние country callout: видимая и исчезающая карточки (ТЗ §1.2).
#[derive(Debug, Default, Clone)]
pub(crate) struct CalloutState {
    current: Option<CalloutCard>,
    previous: Option<CalloutCard>,
}

pub(crate) struct GlobeState<'a> {
    pub camera: &'a mut GlobeCamera,
    pub ready: bool,
    pub markers: &'a [GlobeMarker],
    pub current_uuid: Option<&'a str>,
    pub geo: &'a GeoService,
    pub selected_country: Option<&'a Country>,
    /// В режиме жанра страна не выбирается и не подсвечивается.
    pub country_enabled: bool,
    pub spectrum_overlay: &'a mut SpectrumOverlay,
    pub spectrum_target: Option<SpectrumTarget>,
    /// Плавающие зоны HUD: ввод под ними не доходит до глобуса.
    pub layout: DeskLayout,
    /// Callout страны: переживает кадр ради fade-in/out (ТЗ §1.2).
    pub callout: &'a mut CalloutState,
}

#[derive(Debug, Default)]
pub(crate) struct GlobeResponse {
    pub actions: Vec<UiAction>,
    /// Точка на сфере, если клик не попал в маркер.
    pub clicked_point: Option<(f64, f64, f64)>,
}

pub(crate) fn show(ui: &mut egui::Ui, palette: &Palette, state: GlobeState<'_>) -> GlobeResponse {
    let mut response = GlobeResponse::default();

    ui.set_min_size(ui.available_size());
    let (globe_rect, sense) =
        ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
    paint_space(ui.painter(), globe_rect, palette);
    paint_starfield(ui.painter(), globe_rect);
    paint_vignette(ui.painter(), globe_rect);

    if !state.ready {
        ui.painter().text(
            globe_rect.center(),
            egui::Align2::CENTER_CENTER,
            "3D globe unavailable",
            egui::FontId::proportional(16.0),
            palette.muted,
        );
        return response;
    }

    let hover_pos = sense
        .hover_pos()
        .filter(|position| !state.layout.blocks_point(*position));

    let mut input = GlobeInput::default();
    if sense.dragged_by(egui::PointerButton::Secondary) {
        input.drag_delta = ui.input(|input| input.pointer.delta());
    }
    if hover_pos.is_some() {
        input.scroll_delta = ui.input(|input| input.smooth_scroll_delta.y);
    }
    if let Some(position) = sense
        .clicked_by(egui::PointerButton::Primary)
        .then(|| sense.interact_pointer_pos())
        .flatten()
        .filter(|position| !state.layout.blocks_point(*position))
    {
        input.primary_clicked = Some(position);
    }

    let dt = ui.input(|input| input.stable_dt);
    state.camera.update(dt, &input);

    let marker_screens = marker_screens(state.camera, state.markers, globe_rect);

    if let Some(position) = input.primary_clicked {
        if let Some(index) = nearest_marker(&marker_screens, position) {
            response
                .actions
                .push(UiAction::Play(state.markers[index].station.clone()));
        } else if let Some(point) = state.camera.screen_to_sphere(position, globe_rect) {
            response.clicked_point =
                Some((f64::from(point.x), f64::from(point.y), f64::from(point.z)));
        }
    }

    state
        .spectrum_overlay
        .update(state.spectrum_target, ui.input(|input| input.stable_dt));
    let pulse = state.spectrum_overlay.pulse();
    paint_globe(ui.painter(), globe_rect, state.camera, pulse);

    let hovered = hover_pos.and_then(|position| nearest_marker(&marker_screens, position));
    let hovered_country = if !state.country_enabled
        || sense.dragged_by(egui::PointerButton::Secondary)
    {
        None
    } else {
        hover_pos
            .and_then(|position| country_at_position(state.camera, globe_rect, state.geo, position))
    };

    if state.country_enabled {
        paint_country_highlights(
            ui.painter(),
            globe_rect,
            state.camera,
            state.geo,
            CountryHighlight {
                markers: state.markers,
                selected: state.selected_country,
                hovered: hovered_country,
            },
            palette,
        );
    }

    if hovered.is_some() || hovered_country.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    } else if sense.dragged_by(egui::PointerButton::Secondary) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }

    let mut any_playing = false;
    if !marker_screens.is_empty() {
        let visuals: Vec<MarkerVisual> = marker_screens
            .iter()
            .map(|(index, position)| {
                let marker = &state.markers[*index];
                MarkerVisual {
                    pos: *position,
                    playing: state
                        .current_uuid
                        .is_some_and(|uuid| marker.members.iter().any(|member| member == uuid)),
                    count: marker.count,
                    popularity: marker.popularity,
                    bitrate: marker.bitrate,
                }
            })
            .collect();
        any_playing = visuals.iter().any(|visual| visual.playing);
        let hovered_label = hovered.map(|index| marker_label(&state.markers[index]));
        paint_markers(
            ui.painter(),
            globe_rect,
            &visuals,
            hovered,
            hovered_label.as_deref(),
            MarkerColors {
                marker: palette.marker,
                active: palette.live,
            },
            ui.input(|input| input.time),
        );
    }

    let active_callout = state
        .country_enabled
        .then_some(match state.selected_country {
            Some(country) => Some((country, 1.0)),
            None => hovered_country.map(|country| (country, CALLOUT_HOVER_ALPHA)),
        });
    let active_callout = active_callout.flatten();
    update_callout(state.callout, active_callout, dt);
    let scene = CalloutScene {
        camera: state.camera,
        globe_rect,
        layout: state.layout,
        palette,
        geo: state.geo,
        markers: state.markers,
    };
    let mut callout_fading = false;
    for card in state
        .callout
        .previous
        .iter()
        .chain(state.callout.current.iter())
        .filter(|card| card.alpha > CALLOUT_EPSILON)
    {
        paint_country_callout(ui.painter(), &scene, card);
        callout_fading |= (card.alpha - card.target).abs() > CALLOUT_EPSILON;
    }
    if callout_fading {
        ui.ctx().request_repaint_after(CALLOUT_REPAINT);
    }

    if pulse > 0.005 || any_playing {
        // Пульс обода и пульс играющего маркера живут на кадрах — держим ~30 fps.
        ui.ctx().request_repaint_after(SPECTRUM_REPAINT);
    }

    if state.camera.is_animating() {
        ui.ctx().request_repaint();
    }

    response
}

fn country_at_position<'a>(
    camera: &GlobeCamera,
    viewport: egui::Rect,
    geo: &'a GeoService,
    position: egui::Pos2,
) -> Option<&'a Country> {
    let point = camera.screen_to_sphere(position, viewport)?;
    geo.country_at_sphere_point(f64::from(point.x), f64::from(point.y), f64::from(point.z))
}

struct CountryHighlight<'a> {
    markers: &'a [GlobeMarker],
    selected: Option<&'a Country>,
    hovered: Option<&'a Country>,
}

fn paint_country_highlights(
    painter: &egui::Painter,
    globe_rect: egui::Rect,
    camera: &GlobeCamera,
    geo: &GeoService,
    highlight: CountryHighlight<'_>,
    palette: &Palette,
) {
    let accent = palette.accent;

    if let Some(country) = highlight.selected
        && let Some(shape) = geo.country_shape(&country.iso_a2)
    {
        let count = country_station_count(highlight.markers, &country.iso_a2);
        // Заливка — по рампе плотности (ТЗ §3.2), контур остаётся акцентным.
        let heat = heat_color(heat_intensity(count));
        paint_country(
            painter,
            globe_rect,
            camera,
            &shape.rings,
            CountryStyle {
                fill: Some(egui::Color32::from_rgba_unmultiplied(
                    heat.r(),
                    heat.g(),
                    heat.b(),
                    heat_alpha(count),
                )),
                stroke: egui::Stroke::new(2.0, accent),
            },
        );
    }

    if let Some(country) = highlight.hovered
        && highlight
            .selected
            .is_none_or(|selected| selected.iso_a2 != country.iso_a2)
        && let Some(shape) = geo.country_shape(&country.iso_a2)
    {
        let count = country_station_count(highlight.markers, &country.iso_a2);
        let heat = heat_intensity(count);
        let color = heat_color(heat);
        let fill_alpha = (f32::from(COUNTRY_HOVER_FILL_ALPHA) * (0.35 + 0.65 * heat)).round() as u8;
        paint_country(
            painter,
            globe_rect,
            camera,
            &shape.rings,
            CountryStyle {
                fill: (count > 0).then_some(egui::Color32::from_rgba_unmultiplied(
                    color.r(),
                    color.g(),
                    color.b(),
                    fill_alpha,
                )),
                stroke: egui::Stroke::new(
                    1.4,
                    egui::Color32::from_rgba_unmultiplied(
                        accent.r(),
                        accent.g(),
                        accent.b(),
                        COUNTRY_HOVER_ALPHA,
                    ),
                ),
            },
        );
    }
}

/// Заливка растёт логарифмически: 1 станция → минимум, 100 → максимум.
fn heat_intensity(count: usize) -> f32 {
    if count == 0 {
        return 0.0;
    }
    ((count as f32).log10() / 2.0).clamp(0.15, 1.0)
}

fn heat_alpha(count: usize) -> u8 {
    let min = f32::from(COUNTRY_HEAT_MIN_ALPHA);
    let span = f32::from(COUNTRY_HEAT_MAX_ALPHA - COUNTRY_HEAT_MIN_ALPHA);
    (min + span * heat_intensity(count)).round() as u8
}

fn country_station_count(markers: &[GlobeMarker], iso_a2: &str) -> usize {
    markers
        .iter()
        .filter(|marker| marker.station.countrycode.eq_ignore_ascii_case(iso_a2))
        .map(|marker| marker.count)
        .sum()
}

/// Прозрачность цвета с учётом фэйда карточки.
/// Плавное появление и исчезновение callout (ТЗ §1.2): карточки идут к своей цели.
fn update_callout(state: &mut CalloutState, active: Option<(&Country, f32)>, dt: f32) {
    match active {
        Some((country, target)) => {
            let same = state
                .current
                .as_ref()
                .is_some_and(|card| card.iso == country.iso_a2);
            if same && let Some(card) = state.current.as_mut() {
                card.target = target;
            }
            if !same {
                state.previous = state.current.take().map(|mut card| {
                    card.target = 0.0;
                    card
                });
                state.current = Some(CalloutCard {
                    iso: country.iso_a2.clone(),
                    name: single_line(&country.name),
                    alpha: 0.0,
                    target,
                });
            }
        }
        None => {
            if let Some(card) = state.current.as_mut() {
                card.target = 0.0;
            }
        }
    }

    let step = if dt > 0.0 {
        1.0 - (-dt / CALLOUT_FADE_SECONDS).exp()
    } else {
        1.0
    };
    for card in [&mut state.current, &mut state.previous]
        .into_iter()
        .flatten()
    {
        card.alpha += (card.target - card.alpha) * step;
        if (card.alpha - card.target).abs() < CALLOUT_EPSILON {
            card.alpha = card.target;
        }
    }
    if state
        .previous
        .as_ref()
        .is_none_or(|card| card.alpha <= CALLOUT_EPSILON)
    {
        state.previous = None;
    }
}

/// Контекст кадра для карточки страны: камера, раскладка, палитра и данные.
struct CalloutScene<'a> {
    camera: &'a GlobeCamera,
    globe_rect: egui::Rect,
    layout: DeskLayout,
    palette: &'a Palette,
    geo: &'a GeoService,
    markers: &'a [GlobeMarker],
}

/// Карточка страны у якоря: флаг, имя, число станций, leader line и пин (ТЗ §1.1).
fn paint_country_callout(painter: &egui::Painter, scene: &CalloutScene<'_>, card: &CalloutCard) {
    let CalloutScene {
        camera,
        globe_rect,
        layout,
        palette,
        geo,
        markers,
    } = *scene;
    // Якорь — focus_point страны, тот же путь, что у доворота камеры.
    let Some(anchor) = geo
        .country_shape(&card.iso)
        .and_then(|shape| shape.focus_point())
        .and_then(|(lat, lon)| {
            camera.project_on_sphere(sphere_point(lat as f32, lon as f32), globe_rect)
        })
    else {
        return;
    };

    let alpha = card.alpha;
    let count = country_station_count(markers, &card.iso);
    let flag = flag_emoji(&card.iso);
    let title = if flag.is_empty() {
        card.name.clone()
    } else {
        format!("{flag} {}", card.name)
    };
    let subtitle = if count == 1 {
        "1 station".to_owned()
    } else {
        format!("{count} stations")
    };

    let title_galley = painter.layout_no_wrap(
        title,
        egui::FontId::proportional(13.5),
        faded(palette.text, alpha),
    );
    let subtitle_galley = painter.layout_no_wrap(
        subtitle,
        egui::FontId::proportional(11.0),
        faded(palette.muted, alpha),
    );
    let padding = egui::vec2(11.0, 8.0);
    let gap = 3.0;
    let size = egui::vec2(
        title_galley.size().x.max(subtitle_galley.size().x) + padding.x * 2.0,
        title_galley.size().y + subtitle_galley.size().y + gap + padding.y * 2.0,
    );

    // Свободная зона между HUD, transport и панелью станций.
    let top = layout.hud.bottom() + 6.0;
    let bottom = layout.transport.top() - 6.0;
    let left = layout.globe.left() + 6.0;
    let right = layout
        .panel
        .map_or(layout.globe.right() - 6.0, |panel| panel.left() - 8.0);

    // По умолчанию карточка над якорем, при нехватке места — под ним.
    let slide = CALLOUT_SLIDE * (1.0 - alpha);
    let below = anchor.y - CALLOUT_LEADER - size.y < top;
    let mut rect = if below {
        egui::Rect::from_min_size(
            egui::pos2(anchor.x - size.x / 2.0, anchor.y + CALLOUT_LEADER + slide),
            size,
        )
    } else {
        egui::Rect::from_min_size(
            egui::pos2(
                anchor.x - size.x / 2.0,
                anchor.y - CALLOUT_LEADER - size.y - slide,
            ),
            size,
        )
    };
    if rect.right() > right {
        rect = rect.translate(egui::vec2(right - rect.right(), 0.0));
    }
    if rect.left() < left {
        rect = rect.translate(egui::vec2(left - rect.left(), 0.0));
    }
    if rect.top() < top {
        rect = rect.translate(egui::vec2(0.0, top - rect.top()));
    }
    if rect.bottom() > bottom {
        rect = rect.translate(egui::vec2(0.0, bottom - rect.bottom()));
    }

    let tail = if below {
        egui::pos2(rect.center().x, rect.top())
    } else {
        egui::pos2(rect.center().x, rect.bottom())
    };
    painter.line_segment(
        [anchor, tail],
        egui::Stroke::new(1.0, faded(palette.accent, 0.75 * alpha)),
    );
    painter.circle_filled(anchor, 2.5, faded(palette.live, alpha));
    painter.circle_stroke(
        anchor,
        5.0,
        egui::Stroke::new(1.0, faded(palette.live, 0.5 * alpha)),
    );

    glass_panel(painter, rect, palette, CALLOUT_ROUNDING, alpha);
    let title_height = title_galley.size().y;
    painter.galley(rect.min + padding, title_galley, egui::Color32::WHITE);
    painter.galley(
        egui::pos2(
            rect.left() + padding.x,
            rect.top() + padding.y + title_height + gap,
        ),
        subtitle_galley,
        egui::Color32::WHITE,
    );
}

/// Сигнал популярности станции: голоса Radio Browser. Битрейт на размер не влияет —
/// он кодирует цвет (ТЗ §2.3), формулу размера знает render-модуль.
fn station_popularity(station: &Station) -> f32 {
    station.votes as f32
}

pub(crate) fn build_markers(stations: Vec<Station>) -> Vec<GlobeMarker> {
    let mut markers: Vec<GlobeMarker> = Vec::new();
    let mut groups: HashMap<(i64, i64), usize> = HashMap::new();
    let mut seen = HashSet::new();

    for station in stations {
        if !seen.insert(station.stationuuid.clone()) {
            continue;
        }
        let Some((lat, lon)) = station_geo(&station) else {
            continue;
        };
        let popularity = station_popularity(&station);
        let bitrate = station.bitrate;
        let key = (
            (lat * MARKER_GROUP_SCALE).round() as i64,
            (lon * MARKER_GROUP_SCALE).round() as i64,
        );
        if let Some(&index) = groups.get(&key) {
            let marker = &mut markers[index];
            marker.count += 1;
            marker.popularity = marker.popularity.max(popularity);
            marker.bitrate = marker.bitrate.max(bitrate);
            marker.members.push(station.stationuuid);
        } else {
            groups.insert(key, markers.len());
            markers.push(GlobeMarker {
                members: vec![station.stationuuid.clone()],
                station,
                lat: lat as f32,
                lon: lon as f32,
                count: 1,
                popularity,
                bitrate,
            });
        }
    }

    markers
}

fn station_geo(station: &Station) -> Option<(f64, f64)> {
    let lat = station.geo_lat?;
    let lon = station.geo_long?;
    if !lat.is_finite()
        || !lon.is_finite()
        || !(-90.0..=90.0).contains(&lat)
        || !(-180.0..=180.0).contains(&lon)
    {
        return None;
    }
    if lat == 0.0 && lon == 0.0 {
        return None;
    }
    Some((lat, lon))
}

fn marker_screens(
    camera: &GlobeCamera,
    markers: &[GlobeMarker],
    viewport: egui::Rect,
) -> Vec<(usize, egui::Pos2)> {
    markers
        .iter()
        .enumerate()
        .filter_map(|(index, marker)| {
            camera
                .project_on_sphere(sphere_point(marker.lat, marker.lon), viewport)
                .map(|position| (index, position))
        })
        .collect()
}

fn nearest_marker(screens: &[(usize, egui::Pos2)], position: egui::Pos2) -> Option<usize> {
    screens
        .iter()
        .filter_map(|(index, marker)| {
            let distance = (*marker - position).length();
            (distance <= MARKER_CLICK_RADIUS).then_some((distance, *index))
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, index)| index)
}

/// Подпись маркера по ТЗ §2.4: имя и `место · kbps · кодек`, без флага страны.
fn marker_label(marker: &GlobeMarker) -> String {
    let name = single_line(&marker.station.name);
    let name = if name.is_empty() {
        "Untitled".to_owned()
    } else {
        truncate(&name, 40)
    };
    let head = if marker.count > 1 {
        format!("{name} · +{}", marker.count - 1)
    } else {
        name
    };

    let detail = [
        row_place(&marker.station),
        if marker.bitrate > 0 {
            format!("{} kbps", marker.bitrate)
        } else {
            String::new()
        },
        single_line(&marker.station.codec).to_uppercase(),
    ]
    .into_iter()
    .filter(|part| !part.trim().is_empty())
    .collect::<Vec<_>>()
    .join(" · ");

    if detail.is_empty() {
        head
    } else {
        format!("{head}\n{detail}")
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

    fn geo_station(uuid: &str, lat: f64, lon: f64) -> Station {
        let mut station = sample_station();
        station.stationuuid = uuid.to_owned();
        station.geo_lat = Some(lat);
        station.geo_long = Some(lon);
        station
    }

    #[test]
    fn marker_groups_stations_with_shared_coordinates() {
        let markers = build_markers(vec![
            geo_station("a", 52.5200, 13.4050),
            geo_station("b", 52.5201, 13.4051),
            geo_station("c", 48.8566, 2.3522),
        ]);

        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].count, 2);
        assert_eq!(markers[1].count, 1);
        assert_eq!(markers[0].station.stationuuid, "a");
    }

    #[test]
    fn markers_skip_missing_invalid_and_zero_coordinates() {
        let mut missing = sample_station();
        missing.stationuuid = "missing".to_owned();

        let mut zero = geo_station("zero", 0.0, 0.0);
        zero.stationuuid = "zero".to_owned();

        let markers = build_markers(vec![
            missing,
            zero,
            geo_station("nan", f64::NAN, 13.0),
            geo_station("range", 91.0, 13.0),
            geo_station("valid", 52.52, 13.405),
        ]);

        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].station.stationuuid, "valid");
    }

    #[test]
    fn markers_deduplicate_by_uuid() {
        let markers = build_markers(vec![
            geo_station("same", 52.52, 13.405),
            geo_station("same", 48.85, 2.35),
        ]);

        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].station.stationuuid, "same");
    }

    #[test]
    fn nearest_marker_uses_click_radius() {
        let screens = vec![(0, egui::pos2(10.0, 10.0)), (1, egui::pos2(100.0, 100.0))];

        assert_eq!(nearest_marker(&screens, egui::pos2(14.0, 12.0)), Some(0));
        assert_eq!(nearest_marker(&screens, egui::pos2(50.0, 50.0)), None);
    }

    #[test]
    fn marker_label_reports_group_size() {
        let mut marker = GlobeMarker {
            station: geo_station("a", 52.52, 13.405),
            lat: 52.52,
            lon: 13.405,
            count: 3,
            popularity: 40.0,
            bitrate: 128,
            members: vec!["a".to_owned()],
        };
        marker.station.name = "Groove\nSalad".to_owned();

        assert_eq!(
            marker_label(&marker),
            "Groove Salad · +2\nNew York, United States · 128 kbps · MP3"
        );
    }

    #[test]
    fn marker_label_omits_empty_details() {
        let mut marker = GlobeMarker {
            station: geo_station("a", 52.52, 13.405),
            lat: 52.52,
            lon: 13.405,
            count: 1,
            popularity: 0.0,
            bitrate: 0,
            members: vec!["a".to_owned()],
        };
        marker.station.name = "Night Waves".to_owned();
        marker.station.state.clear();
        marker.station.country.clear();
        marker.station.countrycode.clear();
        marker.station.codec.clear();

        assert_eq!(marker_label(&marker), "Night Waves");
    }

    #[test]
    fn popularity_signal_is_votes_only() {
        let mut silent = sample_station();
        silent.votes = 0;
        silent.bitrate = 320;
        assert_eq!(
            station_popularity(&silent),
            0.0,
            "качество потока не делает маркер крупнее"
        );

        let mut loud = sample_station();
        loud.votes = 4_000;
        loud.bitrate = 32;
        assert_eq!(station_popularity(&loud), 4_000.0);
    }

    #[test]
    fn grouped_markers_keep_the_loudest_station() {
        let mut light = geo_station("light", 52.5200, 13.4050);
        light.votes = 10;
        light.bitrate = 64;
        let mut heavy = geo_station("heavy", 52.5201, 13.4051);
        heavy.votes = 4_000;
        heavy.bitrate = 256;

        let markers = build_markers(vec![light, heavy]);

        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].count, 2);
        assert_eq!(
            markers[0].popularity, 4_000.0,
            "голоса группы — по максимуму"
        );
        assert_eq!(markers[0].bitrate, 256, "цвет группы — по лучшему потоку");
        assert_eq!(
            markers[0].members,
            ["light".to_owned(), "heavy".to_owned()],
            "любая станция группы считается играющей"
        );
    }

    #[test]
    fn country_station_count_sums_grouped_markers() {
        let mut berlin = geo_station("de1", 52.5200, 13.4050);
        berlin.countrycode = "DE".to_owned();
        let mut berlin_twin = geo_station("de2", 52.5201, 13.4051);
        berlin_twin.countrycode = "DE".to_owned();
        let mut paris = geo_station("fr", 48.8566, 2.3522);
        paris.countrycode = "FR".to_owned();

        let markers = build_markers(vec![berlin, berlin_twin, paris]);

        assert_eq!(markers.len(), 2);
        assert_eq!(country_station_count(&markers, "DE"), 2);
        assert_eq!(country_station_count(&markers, "de"), 2);
        assert_eq!(country_station_count(&markers, "FR"), 1);
        assert_eq!(country_station_count(&markers, "US"), 0);
    }

    #[test]
    fn heat_intensity_grows_and_saturates() {
        assert_eq!(heat_intensity(0), 0.0);
        assert!(heat_intensity(1) >= 0.15);
        assert!(heat_intensity(10) > heat_intensity(1));
        assert!((heat_intensity(100) - 1.0).abs() < 1e-6);
        assert!((heat_intensity(10_000) - 1.0).abs() < 1e-6);
        assert_eq!(heat_alpha(100), COUNTRY_HEAT_MAX_ALPHA);
        assert_eq!(heat_alpha(0), COUNTRY_HEAT_MIN_ALPHA);
    }

    fn country(iso: &str, name: &str) -> Country {
        Country {
            name: name.to_owned(),
            iso_a2: iso.to_owned(),
            iso_a3: None,
        }
    }

    #[test]
    fn callout_fades_in_and_out() {
        let germany = country("DE", "Germany");
        let mut state = CalloutState::default();

        update_callout(&mut state, Some((&germany, CALLOUT_HOVER_ALPHA)), 0.05);
        let card = state.current.as_ref().expect("карточка появилась");
        assert!(card.alpha > 0.0 && card.alpha < CALLOUT_HOVER_ALPHA);

        for _ in 0..20 {
            update_callout(&mut state, Some((&germany, CALLOUT_HOVER_ALPHA)), 0.05);
        }
        assert!(
            (state.current.as_ref().unwrap().alpha - CALLOUT_HOVER_ALPHA).abs() < 0.01,
            "за 1 с hover-карточка выходит на 0.85"
        );

        update_callout(&mut state, None, 0.05);
        for _ in 0..20 {
            update_callout(&mut state, None, 0.05);
        }
        assert!(
            state
                .current
                .is_none_or(|card| card.alpha <= CALLOUT_EPSILON)
        );
        assert!(state.previous.is_none());
    }

    #[test]
    fn callout_switch_keeps_the_old_card_for_fade_out() {
        let germany = country("DE", "Germany");
        let france = country("FR", "France");
        let mut state = CalloutState::default();

        for _ in 0..20 {
            update_callout(&mut state, Some((&germany, 1.0)), 0.05);
        }
        update_callout(&mut state, Some((&france, 1.0)), 0.05);

        let current = state.current.as_ref().expect("новая страна");
        let previous = state.previous.as_ref().expect("старая ещё видна");
        assert_eq!(current.iso, "FR");
        assert!(
            current.alpha < previous.alpha,
            "в кадре смены старая карточка ещё ярче новой — есть кроссфэйд"
        );
        assert_eq!(previous.iso, "DE");
        assert_eq!(previous.target, 0.0);
        assert!(
            previous.alpha > 0.5,
            "старая карточка ещё не успела погаснуть"
        );

        for _ in 0..20 {
            update_callout(&mut state, Some((&france, 1.0)), 0.05);
        }
        assert!(state.previous.is_none(), "исчезающая карточка убирается");
        assert!(state.current.as_ref().unwrap().alpha > 0.99);
    }

    #[test]
    fn callout_stays_quiet_without_a_country() {
        let mut state = CalloutState::default();
        for _ in 0..30 {
            update_callout(&mut state, None, 0.05);
        }
        assert!(
            state
                .current
                .is_none_or(|card| card.alpha <= CALLOUT_EPSILON)
        );
        assert!(state.previous.is_none());
    }

    #[test]
    fn hover_hits_country_under_cursor() {
        let geo = GeoService::from_embedded().expect("встроенный датасет");
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));

        let mut camera = GlobeCamera::default();
        assert!(
            country_at_position(&camera, viewport, &geo, viewport.center()).is_none(),
            "в центре по умолчанию океан"
        );

        camera.fly_to(52.52, 13.405, std::time::Duration::from_millis(80));
        while camera.is_animating() {
            camera.update(0.05, &GlobeInput::default());
        }
        let country = country_at_position(&camera, viewport, &geo, viewport.center())
            .expect("после доворота под курсором Германия");
        assert_eq!(country.iso_a2, "DE");
    }
}
