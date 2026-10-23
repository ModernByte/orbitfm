//! 3D-глобус: ввод камеры, маркеры, спектр и подсказки.

use std::collections::{HashMap, HashSet};

use eframe::egui;

use crate::geo::{Country, GeoService};
use crate::render::{
    CountryStyle, GlobeCamera, GlobeInput, MarkerColors, MarkerVisual, SpectrumOverlay,
    SpectrumTarget, paint_country, paint_globe, paint_markers, sphere_point,
};
use crate::services::Station;

use super::palette::Palette;
use super::{UiAction, single_line, truncate};

const MARKER_CLICK_RADIUS: f32 = 10.0;
const MARKER_GROUP_SCALE: f64 = 100.0;
const SPECTRUM_REPAINT: std::time::Duration = std::time::Duration::from_millis(33);
const COUNTRY_FILL_ALPHA: u8 = 34;
const COUNTRY_HOVER_ALPHA: u8 = 120;
const WEIGHT_VOTES_SATURATION: f32 = 5_000.0;
const WEIGHT_BITRATE_SATURATION: f32 = 320.0;
const WEIGHT_VOTES_SHARE: f32 = 0.7;

/// Станция (или группа станций), отображаемая на глобусе.
#[derive(Debug, Clone)]
pub(crate) struct GlobeMarker {
    pub station: Station,
    pub lat: f32,
    pub lon: f32,
    pub count: usize,
    /// Визуальный вес 0..1 из `votes` и `bitrate`.
    pub weight: f32,
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
    pub show_hint: bool,
}

#[derive(Debug, Default)]
pub(crate) struct GlobeResponse {
    pub actions: Vec<UiAction>,
    /// Точка на сфере, если клик не попал в маркер.
    pub clicked_point: Option<(f64, f64, f64)>,
    pub interacted: bool,
}

pub(crate) fn show(ui: &mut egui::Ui, palette: &Palette, state: GlobeState<'_>) -> GlobeResponse {
    let mut response = GlobeResponse::default();

    palette.globe().show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        let size = ui.available_size();
        let (globe_rect, sense) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        ui.painter()
            .rect_filled(globe_rect, egui::CornerRadius::same(16), palette.globe_fill);

        if !state.ready {
            ui.painter().text(
                globe_rect.center(),
                egui::Align2::CENTER_CENTER,
                "3D-глобус недоступен",
                egui::FontId::proportional(16.0),
                palette.muted,
            );
            return;
        }

        let mut input = GlobeInput::default();
        if sense.dragged_by(egui::PointerButton::Secondary) {
            input.drag_delta = ui.input(|input| input.pointer.delta());
        }
        if sense.hovered() {
            input.scroll_delta = ui.input(|input| input.smooth_scroll_delta.y);
        }
        if sense.clicked_by(egui::PointerButton::Primary) {
            input.primary_clicked = sense.interact_pointer_pos();
        }
        if input.drag_delta != egui::Vec2::ZERO
            || input.scroll_delta != 0.0
            || input.primary_clicked.is_some()
        {
            response.interacted = true;
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

        paint_globe(ui.painter(), globe_rect, state.camera);

        let hovered = sense
            .hover_pos()
            .and_then(|position| nearest_marker(&marker_screens, position));
        let hovered_country =
            if !state.country_enabled || sense.dragged_by(egui::PointerButton::Secondary) {
                None
            } else {
                sense.hover_pos().and_then(|position| {
                    country_at_position(state.camera, globe_rect, state.geo, position)
                })
            };

        if state.country_enabled {
            paint_country_highlights(
                ui.painter(),
                globe_rect,
                state.camera,
                state.geo,
                state.selected_country,
                hovered_country,
                palette,
            );
        }

        if hovered.is_some() || hovered_country.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        } else if sense.dragged_by(egui::PointerButton::Secondary) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }

        if !marker_screens.is_empty() {
            let visuals: Vec<MarkerVisual> = marker_screens
                .iter()
                .map(|(index, position)| {
                    let marker = &state.markers[*index];
                    MarkerVisual {
                        pos: *position,
                        highlighted: state.current_uuid
                            == Some(marker.station.stationuuid.as_str()),
                        count: marker.count,
                        weight: marker.weight,
                    }
                })
                .collect();
            let hovered_label = hovered.map(|index| marker_label(&state.markers[index]));
            paint_markers(
                ui.painter(),
                globe_rect,
                &visuals,
                hovered,
                hovered_label.as_deref(),
                MarkerColors {
                    marker: palette.marker,
                    ..MarkerColors::default()
                },
            );
        }

        state
            .spectrum_overlay
            .update(state.spectrum_target, ui.input(|input| input.stable_dt));
        if state.spectrum_overlay.is_visible() {
            let strip = egui::Rect::from_min_max(
                egui::pos2(globe_rect.left() + 16.0, globe_rect.bottom() - 76.0),
                egui::pos2(globe_rect.right() - 16.0, globe_rect.bottom() - 14.0),
            );
            state
                .spectrum_overlay
                .paint(ui.painter(), strip, ui.input(|input| input.time));
            ui.ctx().request_repaint_after(SPECTRUM_REPAINT);
        }

        if state.camera.is_animating() {
            ui.ctx().request_repaint();
        }

        if state.show_hint {
            paint_hint(ui.painter(), globe_rect, palette);
        }

        sense
            .clone()
            .on_hover_text("ПКМ — вращение · колесо — масштаб · ЛКМ — выбор");
    });

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

fn paint_country_highlights(
    painter: &egui::Painter,
    globe_rect: egui::Rect,
    camera: &GlobeCamera,
    geo: &GeoService,
    selected: Option<&Country>,
    hovered: Option<&Country>,
    palette: &Palette,
) {
    let accent = palette.accent;

    if let Some(country) = selected
        && let Some(shape) = geo.country_shape(&country.iso_a2)
    {
        paint_country(
            painter,
            globe_rect,
            camera,
            &shape.rings,
            CountryStyle {
                fill: Some(egui::Color32::from_rgba_unmultiplied(
                    accent.r(),
                    accent.g(),
                    accent.b(),
                    COUNTRY_FILL_ALPHA,
                )),
                stroke: egui::Stroke::new(2.0, accent),
            },
        );
    }

    if let Some(country) = hovered
        && selected.is_none_or(|selected| selected.iso_a2 != country.iso_a2)
        && let Some(shape) = geo.country_shape(&country.iso_a2)
    {
        paint_country(
            painter,
            globe_rect,
            camera,
            &shape.rings,
            CountryStyle {
                fill: None,
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

fn paint_hint(painter: &egui::Painter, globe_rect: egui::Rect, palette: &Palette) {
    let text = "ПКМ — вращение · колесо — масштаб · ЛКМ — выбор";
    let galley = painter.layout_no_wrap(
        text.to_owned(),
        egui::FontId::proportional(11.0),
        palette.muted,
    );
    let padding = egui::vec2(10.0, 6.0);
    let background = egui::Rect::from_min_size(
        globe_rect.min + egui::vec2(14.0, 12.0),
        galley.size() + padding * 2.0,
    );
    painter.rect_filled(background, egui::CornerRadius::same(8), palette.subtle_fill);
    painter.galley(background.min + padding, galley, palette.muted);
}

/// Визуальный вес маркера: голоса важнее битрейта, оба сжимаются насыщением.
pub(crate) fn marker_weight(station: &Station) -> f32 {
    let votes = (station.votes as f32 / WEIGHT_VOTES_SATURATION)
        .sqrt()
        .clamp(0.0, 1.0);
    let bitrate = (station.bitrate as f32 / WEIGHT_BITRATE_SATURATION).clamp(0.0, 1.0);
    (votes * WEIGHT_VOTES_SHARE + bitrate * (1.0 - WEIGHT_VOTES_SHARE)).clamp(0.0, 1.0)
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
        let weight = marker_weight(&station);
        let key = (
            (lat * MARKER_GROUP_SCALE).round() as i64,
            (lon * MARKER_GROUP_SCALE).round() as i64,
        );
        if let Some(&index) = groups.get(&key) {
            markers[index].count += 1;
            markers[index].weight = markers[index].weight.max(weight);
        } else {
            groups.insert(key, markers.len());
            markers.push(GlobeMarker {
                station,
                lat: lat as f32,
                lon: lon as f32,
                count: 1,
                weight,
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

fn marker_label(marker: &GlobeMarker) -> String {
    let name = single_line(&marker.station.name);
    let name = if name.is_empty() {
        "Без названия".to_owned()
    } else {
        truncate(&name, 40)
    };

    if marker.count > 1 {
        format!("{name} · +{}", marker.count - 1)
    } else {
        name
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
            weight: 0.4,
        };
        marker.station.name = "Groove\nSalad".to_owned();

        assert_eq!(marker_label(&marker), "Groove Salad · +2");
    }

    #[test]
    fn marker_weight_combines_votes_and_bitrate() {
        let mut station = sample_station();
        station.votes = 0;
        station.bitrate = 0;
        assert_eq!(marker_weight(&station), 0.0);

        let mut popular = sample_station();
        popular.votes = 5_000;
        popular.bitrate = 320;
        assert!((marker_weight(&popular) - 1.0).abs() < 1e-5);

        let mut quiet = sample_station();
        quiet.votes = 100;
        quiet.bitrate = 96;
        assert!(marker_weight(&quiet) > 0.0 && marker_weight(&quiet) < marker_weight(&popular));
        assert!(marker_weight(&popular) >= marker_weight(&quiet));
    }

    #[test]
    fn grouped_markers_keep_the_heaviest_weight() {
        let mut light = geo_station("light", 52.5200, 13.4050);
        light.votes = 10;
        light.bitrate = 64;
        let mut heavy = geo_station("heavy", 52.5201, 13.4051);
        heavy.votes = 4_000;
        heavy.bitrate = 256;
        let heaviest = marker_weight(&heavy);

        let markers = build_markers(vec![light, heavy]);

        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].count, 2);
        assert!((markers[0].weight - heaviest).abs() < 1e-6);
        assert!(markers[0].weight > 0.5, "вес группы берётся по максимуму");
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
