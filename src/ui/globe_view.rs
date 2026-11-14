//! 3D-глобус: ввод камеры, маркеры, спектр и подсказки.

mod fan;

use std::collections::HashMap;
use std::sync::OnceLock;

use eframe::egui;

use crate::geo::{Country, GeoService};
use crate::render::{
    CountryStyle, GlobeCamera, GlobeDeform, GlobeInput, GlobeMotion, GlobeProjector, MarkerLabel,
    MarkerVisual, frame_stats, paint_country, paint_globe, paint_markers, sphere_point,
};
use crate::services::Station;
use crate::session::markers::GlobeMarker;
use crate::settings::Language;

use super::animation;
use super::layout::DeskLayout;
use super::orbit_ring::OrbitRing;
use super::palette::{Palette, heat_color};
use super::repaint::{self, RepaintReason};
use super::station_row::row_place;
use super::widgets::{faded, flag_emoji, glass_panel};
use super::{UiAction, single_line, truncate};

pub(crate) use fan::FanState;

const MARKER_CLICK_RADIUS: f32 = 10.0;
const COUNTRY_HEAT_MIN_ALPHA: u8 = 26;
const COUNTRY_HEAT_MAX_ALPHA: u8 = 96;
const COUNTRY_HOVER_ALPHA: u8 = 120;
const COUNTRY_HOVER_FILL_ALPHA: u8 = 30;
const STAR_COUNT: usize = 160;
/// Полосы вертикального градиента космоса и глубина виньетки (ТЗ §3.1, §5.1 п. 5).
const SPACE_BANDS: usize = 12;
const VIGNETTE_BANDS: usize = 6;
/// Длительность fade-in/out callout: 180 мс из контракта ADR-0032.
const CALLOUT_FADE_SECONDS: f32 = animation::CALLOUT;
const CALLOUT_HOVER_ALPHA: f32 = 0.85;
/// Расстояние от якоря до карточки — длина leader line.
const CALLOUT_LEADER: f32 = 26.0;
/// Смещение карточки от якоря на время появления.
const CALLOUT_SLIDE: f32 = 6.0;
const CALLOUT_ROUNDING: f32 = 11.0;
/// Минимальный просвет между плашкой страны и плашкой станции, px.
const CALLOUT_STACK_GAP: f32 = 6.0;
const CALLOUT_EPSILON: f32 = 0.004;

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

/// Плашка, переживающая кадры: страна или активная станция. Содержимое
/// вычисляется один раз при появлении, дальше анимируется только прозрачность.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CalloutCard {
    /// Ключ идентичности: ISO страны или UUID станции.
    key: String,
    title: String,
    subtitle: String,
    /// Якорь на сфере, градусы.
    lat: f32,
    lon: f32,
    /// Станция подсвечивается акцентом, страна — обычным текстом.
    accent: bool,
    /// Текущая прозрачность 0..1 (умножается на цвета карточки).
    alpha: f32,
    target: f32,
}

impl CalloutCard {
    fn new(
        key: String,
        title: String,
        subtitle: String,
        lat: f32,
        lon: f32,
        accent: bool,
        target: f32,
    ) -> Self {
        Self {
            key,
            title,
            subtitle,
            lat,
            lon,
            accent,
            alpha: 0.0,
            target,
        }
    }
}

/// Состояние одного callout-канала: видимая и исчезающая карточки (ТЗ §1.2).
#[derive(Debug, Default, Clone)]
pub(crate) struct CalloutState {
    current: Option<CalloutCard>,
    previous: Option<CalloutCard>,
}

pub(crate) struct GlobeState<'a> {
    pub camera: &'a mut GlobeCamera,
    /// Аудио-деформа планеты: та же, что у меша, маркеров и hit-test (ADR-0028).
    pub motion: &'a GlobeMotion,
    pub ready: bool,
    pub markers: &'a [GlobeMarker],
    /// Активная станция: маркер пульсирует, у точки — плашка, как у страны.
    pub current_station: Option<&'a Station>,
    /// Выбранная станция: её маркер обведён постоянным кольцом (ADR-0036).
    pub selected_station: Option<&'a str>,
    pub geo: &'a GeoService,
    /// Счётчики всех стран Radio Browser из прогретого `session::world`
    /// (ADR-0035): подсказка и заливка страны показывают размер базы, а не
    /// только видимую выборку. Пустая карта — снимок ещё не пришёл.
    pub country_counts: Option<&'a HashMap<String, u32>>,
    pub selected_country: Option<&'a Country>,
    /// В режиме жанра страна не выбирается и не подсвечивается.
    pub country_enabled: bool,
    /// Full-bleed космос Focus; у мини-глобуса капсулы фон прозрачный.
    pub backdrop: bool,
    /// Орбитальное аудиокольцо: рисуется до wgpu-callback'а, поэтому планета
    /// перекрывает его внутреннюю часть (ADR-0029).
    pub ring: &'a OrbitRing,
    /// Плавающие зоны HUD: ввод под ними не доходит до глобуса.
    pub layout: DeskLayout,
    /// Перевороты разрешены: Focus, движение включено и настройка активна.
    pub allow_flips: bool,
    /// Режим вращения: аудио-реактивность выключена, глобус крутится сам.
    pub auto_spin: bool,
    /// Тепловая карта стран: текстура уже собрана под текущую выборку.
    pub heat: Option<&'a std::sync::Arc<crate::render::HeatFrame>>,
    /// Callout страны: переживает кадр ради fade-in/out (ТЗ §1.2).
    pub country_callout: &'a mut CalloutState,
    /// Callout активной станции: отдельный канал, чтобы плашки не спорили
    /// за одну карточку и могли разъезжаться.
    pub station_callout: &'a mut CalloutState,
    /// Веер станций кластера: состояние между кадрами и геометрия для
    /// блокировки клика/колеса (ADR-0065 § 3).
    pub fan: &'a mut FanState,
    /// Язык подписей веера и карточки «+N» (ADR-0030).
    pub language: Language,
    /// Reduce motion: карточка появляется и гаснет без перехода (ADR-0032).
    pub reduce_motion: bool,
}

#[derive(Debug, Default)]
pub(crate) struct GlobeResponse {
    pub actions: Vec<UiAction>,
    /// Точка на сфере, если клик не попал в маркер.
    pub clicked_point: Option<(f64, f64, f64)>,
    /// Первичный клик не попал ни в маркер, ни в страну: выделение страны
    /// снимается, список станций при этом остаётся.
    pub clicked_empty: bool,
    /// Сколько чипов/кластеров нарисовано в этом кадре: счётчик «B точек»
    /// в панели (ADR-0065 § 4).
    pub clusters: usize,
}

#[allow(clippy::needless_pass_by_value)] // пучок аргументов с `&mut`-полем
pub(crate) fn show(ui: &mut egui::Ui, palette: &Palette, state: GlobeState<'_>) -> GlobeResponse {
    // Граница кадра глобуса: замер включается только `ORBITFM_FRAME_STATS=1`.
    let show_started = frame_stats::begin();
    let mut response = GlobeResponse::default();

    ui.set_min_size(ui.available_size());
    let (globe_rect, sense) =
        ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
    if state.backdrop {
        paint_space(ui.painter(), globe_rect, palette);
        paint_starfield(ui.painter(), globe_rect);
    }

    if !state.ready {
        if state.backdrop {
            paint_vignette(ui.painter(), globe_rect);
        }
        ui.painter().text(
            globe_rect.center(),
            egui::Align2::CENTER_CENTER,
            "3D globe unavailable",
            egui::FontId::proportional(16.0),
            palette.muted,
        );
        frame_stats::end(frame_stats::Metric::GlobeShow, show_started);
        return response;
    }

    let pointer = ui.input(|input| input.pointer.hover_pos());
    // Веер прошлого кадра забирает курсор: клик, колесо и наведение не должны
    // одновременно работать по глобусу (hit-test карточки «+N», ADR-0065 § 3).
    let fan_capture = pointer.is_some_and(|position| state.fan.blocks_point(position));
    let hover_pos = sense
        .hover_pos()
        .filter(|position| !state.layout.blocks_point(*position) && !fan_capture);

    // Жёсткий фильтр по точке нажатия, а не по текущему курсору: жест, начатый
    // над панелью или при открытом окне настроек, не должен вращать глобус (ADR-0031).
    let press_blocked = ui.input(|input| {
        input.pointer.press_origin().is_some_and(|origin| {
            state.layout.blocks_point(origin) || state.fan.blocks_point(origin)
        })
    });
    // Окно настроек теперь отдельное вьюпорт-окно (ADR-0053): глобус стола
    // не блокируется настройками.
    let mut input = GlobeInput {
        // Перетаскивание переводится в поворот через фокусное расстояние
        // кадра: поверхность следует за курсором (ADR-0060 § 3).
        viewport_height: globe_rect.height(),
        ..GlobeInput::default()
    };
    if sense.dragged_by(egui::PointerButton::Secondary) && !press_blocked {
        input.drag_delta = ui.input(|input| input.pointer.delta());
    }
    if hover_pos.is_some() {
        input.scroll_delta = ui.input(|input| input.smooth_scroll_delta.y);
        input.hovered = true;
    }
    if let Some(position) = sense
        .clicked_by(egui::PointerButton::Primary)
        .then(|| sense.interact_pointer_pos())
        .flatten()
        .filter(|position| !state.layout.blocks_point(*position))
    {
        input.primary_clicked = Some(position);
    }
    let (dt, elapsed) = ui.input(|input| (input.stable_dt, input.unstable_dt));
    state.camera.advance(
        dt,
        elapsed,
        &input,
        state.motion,
        state.allow_flips,
        state.auto_spin,
    );
    // Пока камера под управлением пользователя, squash и рябь ослабляются
    // до 35 %; качка гасится самим камерным приоритетом (ADR-0028). Курсор
    // над глобусом плавно успокаивает движение до нуля.
    let mut motion = state
        .motion
        .with_camera_priority(state.camera.camera_priority())
        .with_calm(state.camera.hover_calm());
    // Переворот живёт в камере: снимок отражает фактический roll кадра.
    motion.roll = state.camera.roll();
    let deform = motion.deform;

    let marker_screens = {
        let started = frame_stats::begin();
        let screens = marker_screens(state.camera, state.markers, globe_rect, &deform);
        frame_stats::end(frame_stats::Metric::MarkersProject, started);
        screens
    };
    // Вторая ступень: то, что слипается на экране, — один чип (ADR-0065).
    let clusters = cluster_screens(state.markers, &marker_screens);
    response.clusters = clusters.len();

    if state.backdrop {
        state.ring.paint(
            ui.painter(),
            globe_rect.center(),
            state.camera.projected_radius(globe_rect),
            palette,
        );
        paint_vignette(ui.painter(), globe_rect);
    }

    paint_globe(ui.painter(), globe_rect, state.camera, &motion, state.heat);

    let hovered = hover_pos.and_then(|position| nearest_cluster(&clusters, position));
    let hovered_country =
        if !state.country_enabled || sense.dragged_by(egui::PointerButton::Secondary) {
            None
        } else {
            hover_pos.and_then(|position| {
                country_at_position(state.camera, globe_rect, state.geo, position, &deform)
            })
        };

    if state.country_enabled {
        let started = frame_stats::begin();
        paint_country_highlights(
            ui.painter(),
            globe_rect,
            state.camera,
            &deform,
            state.geo,
            CountryHighlight {
                markers: state.markers,
                counts: state.country_counts,
                selected: state.selected_country,
                hovered: hovered_country,
            },
            palette,
        );
        frame_stats::end(frame_stats::Metric::CountriesPaint, started);
    }

    if hovered.is_some() || hovered_country.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    } else if sense.dragged_by(egui::PointerButton::Secondary) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }

    let mut any_playing = false;
    let current_uuid = state
        .current_station
        .map(|station| station.stationuuid.as_str());
    let now = ui.input(|input| input.time);
    let zone = callout_zone(state.layout);
    let fan_input = fan::FanInput {
        hovered,
        pointer,
        clicked: input.primary_clicked,
        now,
        globe_center: globe_rect.center(),
        // Чипы и подсказка держатся свободной зоны, а не заезжают под HUD
        // и панель; без зоны (капсулы) веер всё равно не открывается.
        bounds: zone.unwrap_or(globe_rect),
        zone,
        language: state.language,
        current_uuid,
        selected_uuid: state.selected_station,
        reduce_motion: state.reduce_motion,
    };
    // Порог наведения считается до отрисовки подписи маркера: раскрывшийся
    // веер сразу убирает плашку, а не мигает ею один кадр.
    fan::advance(state.fan, state.markers, &clusters, &fan_input);

    if !clusters.is_empty() {
        let visuals: Vec<MarkerVisual> = clusters
            .iter()
            .map(|cluster| {
                let playing = current_uuid.is_some_and(|uuid| {
                    cluster.members.iter().any(|index| {
                        state.markers[*index]
                            .stations
                            .iter()
                            .any(|station| station.stationuuid == uuid)
                    })
                });
                let selected = state.selected_station.is_some_and(|uuid| {
                    cluster.members.iter().any(|index| {
                        state.markers[*index]
                            .stations
                            .iter()
                            .any(|station| station.stationuuid == uuid)
                    })
                });
                MarkerVisual {
                    pos: cluster.pos,
                    count: cluster.stations,
                    playing,
                    selected,
                }
            })
            .collect();
        any_playing = visuals.iter().any(|visual| visual.playing);
        let hovered_label = (!state.fan.is_open())
            .then(|| hovered.map(|slot| cluster_label(state.markers, &clusters[slot])))
            .flatten();
        let started = frame_stats::begin();
        paint_markers(
            ui.painter(),
            globe_rect,
            &visuals,
            hovered,
            hovered_label.as_ref(),
            palette.marker,
            now,
        );
        frame_stats::end(frame_stats::Metric::MarkersPaint, started);
    }

    // Открытый веер — активное меню: плашка страны уступает ему место, иначе
    // выбранная страна накрывает чипы рядом с кластером (ADR-0065 § 3).
    let fan_open = state.fan.is_open();
    let active_country = if fan_open {
        None
    } else {
        state
            .country_enabled
            .then_some(match state.selected_country {
                Some(country) => {
                    country_card(country, state.markers, state.country_counts, state.geo, 1.0)
                }
                None => hovered_country.and_then(|country| {
                    country_card(
                        country,
                        state.markers,
                        state.country_counts,
                        state.geo,
                        CALLOUT_HOVER_ALPHA,
                    )
                }),
            })
            .flatten()
    };
    update_callout(
        state.country_callout,
        active_country,
        dt,
        state.reduce_motion,
    );
    let active_station = state
        .current_station
        .and_then(|station| station_card(station, 1.0));
    update_callout(
        state.station_callout,
        active_station,
        dt,
        state.reduce_motion,
    );
    let scene = CalloutScene {
        camera: state.camera,
        deform,
        globe_rect,
        layout: state.layout,
        palette,
    };
    let mut callout_fading = false;
    // Порядок важен: страна занимает место первой, плашка станции обходит её
    // прямоугольник — две карточки никогда не накладываются.
    let country_rect = paint_callout_channel(
        ui.painter(),
        &scene,
        state.country_callout,
        None,
        &mut callout_fading,
    );
    let station_rect = paint_callout_channel(
        ui.painter(),
        &scene,
        state.station_callout,
        country_rect,
        &mut callout_fading,
    );
    if callout_fading {
        repaint::schedule(ui.ctx(), RepaintReason::Callout);
    }

    // Веер рисуется поверх маркеров и callout'ов: чипы — активное меню,
    // и плашка страны не должна их накрывать. Карточка «+N» по-прежнему
    // обходит прямоугольники callout'ов (ADR-0065 § 3).
    let chips = fan::chips(ui, state.fan, state.markers, &clusters, fan_input, palette);
    let mut fan_clicked = chips.clicked;
    response.actions.extend(chips.actions);

    // Карточка «+N» — последний канал: она уступает место стране и станции.
    let card = fan::card(
        ui,
        state.fan,
        state.markers,
        &clusters,
        fan_input,
        [country_rect, station_rect],
        palette,
    );
    fan_clicked |= card.clicked;
    response.actions.extend(card.actions);

    // Клик обрабатывается после веера: чип и карточка забирают его раньше
    // глобуса, иначе выбор станции дублировался бы кликом по маркеру.
    if let Some(position) = input.primary_clicked
        && !fan_clicked
    {
        if let Some(slot) = nearest_cluster(&clusters, position) {
            let cluster = &clusters[slot];
            if cluster.stations >= 2 && zone.is_some() {
                // Клик по кластеру закрепляет веер, а не запускает станцию.
                if let Some(key) = fan::cluster_key(state.markers, cluster) {
                    state.fan.pin(key, now);
                }
            } else if let Some(station) = fan::cluster_station(state.markers, cluster, 0) {
                response
                    .actions
                    .push(UiAction::SelectStation(station.clone()));
                state.fan.close();
            }
        } else {
            state.fan.close();
            response.clicked_empty = true;
            if let Some(point) = state
                .camera
                .screen_to_sphere_deformed(position, globe_rect, &deform)
            {
                response.clicked_point =
                    Some((f64::from(point.x), f64::from(point.y), f64::from(point.z)));
            }
        }
    }

    if state.fan.is_open() {
        // Пока веер раскрыт, кадры нужны для вылета и подсветки слотов.
        repaint::schedule(ui.ctx(), RepaintReason::Interaction);
    } else if let Some(deadline) = state.fan.pending_deadline(now) {
        repaint::schedule(
            ui.ctx(),
            RepaintReason::Deadline(std::time::Duration::from_secs_f64(deadline.max(0.01))),
        );
    }

    if any_playing || (state.backdrop && state.ring.is_animating()) {
        // Пульс маркера и орбитальное кольцо живут на кадрах — держим ~30 fps.
        repaint::schedule(ui.ctx(), RepaintReason::Playback);
    }

    if state.camera.is_idle_spinning() {
        // Медленному авто-вращению хватает 30 fps и без эфира.
        repaint::schedule(ui.ctx(), RepaintReason::IdleSpin);
    } else if state.camera.is_animating() {
        repaint::schedule(ui.ctx(), RepaintReason::Interaction);
    } else if let Some(remaining) = state.camera.idle_countdown() {
        // Без эфира окно может спать: будим его к началу авто-вращения.
        repaint::schedule(
            ui.ctx(),
            RepaintReason::Deadline(std::time::Duration::from_secs_f32(remaining.max(0.05))),
        );
    }

    frame_stats::end(frame_stats::Metric::GlobeShow, show_started);
    response
}

fn country_at_position<'a>(
    camera: &GlobeCamera,
    viewport: egui::Rect,
    geo: &'a GeoService,
    position: egui::Pos2,
    deform: &GlobeDeform,
) -> Option<&'a Country> {
    let point = camera.screen_to_sphere_deformed(position, viewport, deform)?;
    geo.country_at_sphere_point(f64::from(point.x), f64::from(point.y), f64::from(point.z))
}

#[derive(Clone, Copy)]
struct CountryHighlight<'a> {
    markers: &'a [GlobeMarker],
    counts: Option<&'a HashMap<String, u32>>,
    selected: Option<&'a Country>,
    hovered: Option<&'a Country>,
}

fn paint_country_highlights(
    painter: &egui::Painter,
    globe_rect: egui::Rect,
    camera: &GlobeCamera,
    deform: &GlobeDeform,
    geo: &GeoService,
    highlight: CountryHighlight<'_>,
    palette: &Palette,
) {
    let accent = palette.accent;

    if let Some(country) = highlight.selected
        && let Some(shape) = geo.country_shape(&country.iso_a2)
    {
        let count = country_station_count(highlight.markers, highlight.counts, &country.iso_a2)
            .unwrap_or(0);
        // Заливка — по рампе плотности (ТЗ §3.2), контур остаётся акцентным.
        let heat = heat_color(heat_intensity(count));
        paint_country(
            painter,
            globe_rect,
            camera,
            deform,
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
        let count = country_station_count(highlight.markers, highlight.counts, &country.iso_a2)
            .unwrap_or(0);
        let heat = heat_intensity(count);
        let color = heat_color(heat);
        let fill_alpha = (f32::from(COUNTRY_HOVER_FILL_ALPHA) * (0.35 + 0.65 * heat)).round() as u8;
        paint_country(
            painter,
            globe_rect,
            camera,
            deform,
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

/// Число станций страны: сначала прогретый снимок всей базы Radio Browser,
/// затем — сумма видимых маркеров. `None` — счётчик неизвестен (снимок ещё не
/// пришёл, а маркеров страны нет): подсказка не пишет выдуманный ноль.
fn country_station_count(
    markers: &[GlobeMarker],
    counts: Option<&HashMap<String, u32>>,
    iso_a2: &str,
) -> Option<usize> {
    if let Some(counts) = counts.filter(|counts| !counts.is_empty()) {
        return counts
            .get(&iso_a2.to_ascii_uppercase())
            .map(|count| *count as usize);
    }
    let visible: usize = markers
        .iter()
        .filter(|marker| marker.station().countrycode.eq_ignore_ascii_case(iso_a2))
        .map(GlobeMarker::count)
        .sum();
    (visible > 0).then_some(visible)
}

/// Плавное появление и исчезновение callout (ТЗ §1.2): карточки идут к своей цели.
/// Reduce motion (ADR-0032) убирает переход: карточка сразу в конечном состоянии.
fn update_callout(state: &mut CalloutState, active: Option<CalloutCard>, dt: f32, reduce: bool) {
    match active {
        Some(card) => {
            let same = state
                .current
                .as_ref()
                .is_some_and(|current| current.key == card.key);
            if same && let Some(current) = state.current.as_mut() {
                current.target = card.target;
            }
            if !same {
                state.previous = state.current.take().map(|mut card| {
                    card.target = 0.0;
                    card
                });
                state.current = Some(card);
            }
        }
        None => {
            if let Some(card) = state.current.as_mut() {
                card.target = 0.0;
            }
        }
    }

    let step = if reduce || dt <= 0.0 {
        1.0
    } else {
        1.0 - (-dt / CALLOUT_FADE_SECONDS).exp()
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

/// Контекст кадра для карточек: камера, раскладка, палитра и данные.
struct CalloutScene<'a> {
    camera: &'a GlobeCamera,
    deform: GlobeDeform,
    globe_rect: egui::Rect,
    layout: DeskLayout,
    palette: &'a Palette,
}

/// Карточка страны: флаг, имя и число станций базы (или видимых маркеров,
/// пока снимок мира не пришёл).
fn country_card(
    country: &Country,
    markers: &[GlobeMarker],
    counts: Option<&HashMap<String, u32>>,
    geo: &GeoService,
    target: f32,
) -> Option<CalloutCard> {
    // Якорь — focus_point страны, тот же путь, что у доворота камеры.
    let (lat, lon) = geo.country_shape(&country.iso_a2)?.focus_point()?;
    let flag = flag_emoji(&country.iso_a2);
    let name = single_line(&country.name);
    let title = if flag.is_empty() {
        name
    } else {
        format!("{flag} {name}")
    };
    let subtitle = match country_station_count(markers, counts, &country.iso_a2) {
        Some(1) => "1 station".to_owned(),
        Some(count) => format!("{count} stations"),
        None => String::new(),
    };
    Some(CalloutCard::new(
        country.iso_a2.clone(),
        title,
        subtitle,
        lat as f32,
        lon as f32,
        false,
        target,
    ))
}

/// Карточка активной станции: имя, место, качество и кодек.
fn station_card(station: &Station, target: f32) -> Option<CalloutCard> {
    let (lat, lon) = station.geo_lat.zip(station.geo_long)?;
    let name = single_line(&station.name);
    let title = if name.is_empty() {
        "Untitled".to_owned()
    } else {
        truncate(&name, 32)
    };
    let subtitle = [
        row_place(station),
        if station.bitrate > 0 {
            format!("{} kbps", station.bitrate)
        } else {
            String::new()
        },
        single_line(&station.codec).to_uppercase(),
    ]
    .into_iter()
    .filter(|part| !part.trim().is_empty())
    .collect::<Vec<_>>()
    .join(" · ");
    Some(CalloutCard::new(
        station.stationuuid.clone(),
        title,
        subtitle,
        lat as f32,
        lon as f32,
        true,
        target,
    ))
}

/// Рисует карточки одного канала и возвращает прямоугольник видимой текущей:
/// второй канал обходит его, чтобы плашки никогда не перекрывались.
fn paint_callout_channel(
    painter: &egui::Painter,
    scene: &CalloutScene<'_>,
    state: &CalloutState,
    avoid: Option<egui::Rect>,
    fading: &mut bool,
) -> Option<egui::Rect> {
    let current_key = state.current.as_ref().map(|card| card.key.as_str());
    let mut current_rect = None;
    for card in state
        .previous
        .iter()
        .chain(state.current.iter())
        .filter(|card| card.alpha > CALLOUT_EPSILON)
    {
        *fading |= (card.alpha - card.target).abs() > CALLOUT_EPSILON;
        let rect = paint_callout(painter, scene, card, avoid);
        if current_key == Some(card.key.as_str()) {
            current_rect = rect;
        }
    }
    current_rect
}

/// Свободная зона Focus-стола для карточек: между HUD, transport и панелью.
/// Возвращает `None` в капсулах, где этих зон нет.
fn callout_zone(layout: DeskLayout) -> Option<egui::Rect> {
    let (hud, transport, globe) = (layout.hud?, layout.transport?, layout.globe?);
    let left = globe.left() + 6.0;
    let right = layout
        .panel
        .map_or(globe.right() - 6.0, |panel| panel.left() - 8.0);
    let top = hud.bottom() + 6.0;
    let bottom = transport.top() - 6.0;
    (right > left && bottom > top)
        .then(|| egui::Rect::from_min_max(egui::pos2(left, top), egui::pos2(right, bottom)))
}

/// Карточка у якоря: заголовок, подпись, leader line и точка. `avoid` —
/// прямоугольник соседней карточки, который нельзя перекрывать.
fn paint_callout(
    painter: &egui::Painter,
    scene: &CalloutScene<'_>,
    card: &CalloutCard,
    avoid: Option<egui::Rect>,
) -> Option<egui::Rect> {
    let CalloutScene {
        camera,
        deform,
        globe_rect,
        layout,
        palette,
        ..
    } = *scene;
    let anchor = camera.project_on_sphere(
        deform.to_world(sphere_point(card.lat, card.lon)),
        globe_rect,
    )?;

    let alpha = card.alpha;
    let title_galley = painter.layout_no_wrap(
        card.title.clone(),
        egui::FontId::proportional(13.5),
        faded(
            if card.accent {
                palette.accent
            } else {
                palette.text
            },
            alpha,
        ),
    );
    let subtitle_galley = (!card.subtitle.trim().is_empty()).then(|| {
        painter.layout_no_wrap(
            card.subtitle.clone(),
            egui::FontId::proportional(11.0),
            faded(palette.muted, alpha),
        )
    });
    let padding = egui::vec2(11.0, 8.0);
    let gap = 3.0;
    let text_width = title_galley.size().x.max(
        subtitle_galley
            .as_ref()
            .map_or(0.0, |galley| galley.size().x),
    );
    let text_height = title_galley.size().y
        + subtitle_galley
            .as_ref()
            .map_or(0.0, |galley| galley.size().y + gap);
    let size = egui::vec2(text_width + padding.x * 2.0, text_height + padding.y * 2.0);

    // Свободная зона между HUD, transport и панелью станций. Callout живёт
    // только в Focus: в капсулах этих зон нет.
    let zone = callout_zone(layout)?;
    let (top, bottom, left, right) = (zone.top(), zone.bottom(), zone.left(), zone.right());

    // По умолчанию карточка над якорем, при нехватке места — под ним; если
    // рядом стоит вторая плашка, пробуем встать над ней или под ней.
    let slide = CALLOUT_SLIDE * (1.0 - alpha);
    let mut candidates = [
        egui::Rect::from_min_size(
            egui::pos2(
                anchor.x - size.x / 2.0,
                anchor.y - CALLOUT_LEADER - size.y - slide,
            ),
            size,
        ),
        egui::Rect::from_min_size(
            egui::pos2(anchor.x - size.x / 2.0, anchor.y + CALLOUT_LEADER + slide),
            size,
        ),
        egui::Rect::ZERO,
        egui::Rect::ZERO,
    ];
    if let Some(avoid) = avoid {
        candidates[2] = egui::Rect::from_min_size(
            egui::pos2(
                anchor.x - size.x / 2.0,
                avoid.top() - CALLOUT_LEADER * 0.5 - size.y,
            ),
            size,
        );
        candidates[3] = egui::Rect::from_min_size(
            egui::pos2(
                anchor.x - size.x / 2.0,
                avoid.bottom() + CALLOUT_LEADER * 0.5,
            ),
            size,
        );
    }
    let rect = candidates
        .into_iter()
        .filter_map(|candidate| clamp_callout(candidate, top, bottom, left, right))
        .find(|rect| {
            !avoid.is_some_and(|avoid| rect.intersects(avoid.expand(CALLOUT_STACK_GAP)))
        })?;

    let below = rect.center().y > anchor.y;
    let tail = egui::pos2(
        rect.center().x,
        if below { rect.top() } else { rect.bottom() },
    );
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
    if let Some(subtitle_galley) = subtitle_galley {
        painter.galley(
            egui::pos2(
                rect.left() + padding.x,
                rect.top() + padding.y + title_height + gap,
            ),
            subtitle_galley,
            egui::Color32::WHITE,
        );
    }
    Some(rect)
}

/// Сдвигает карточку внутрь свободной зоны; `None`, если она туда не влезает.
fn clamp_callout(
    mut rect: egui::Rect,
    top: f32,
    bottom: f32,
    left: f32,
    right: f32,
) -> Option<egui::Rect> {
    if right - left < rect.width() || bottom - top < rect.height() {
        return None;
    }
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
    Some(rect)
}

fn marker_screens(
    camera: &GlobeCamera,
    markers: &[GlobeMarker],
    viewport: egui::Rect,
    deform: &GlobeDeform,
) -> Vec<(usize, egui::Pos2)> {
    // Один `view_proj` на кадр: сборка матрицы на точку была почти всей ценой
    // проекции (план § 11). Семантика отбора не меняется (тест паритета).
    let Some(projector) = GlobeProjector::new(camera, viewport) else {
        return Vec::new();
    };
    markers
        .iter()
        .enumerate()
        .filter_map(|(index, marker)| {
            projector
                .project(deform.to_world(sphere_point(marker.lat, marker.lon)))
                .map(|position| (index, position))
        })
        .collect()
}

/// Радиус экранной склейки: маркеры ближе 16 px — один чип (ADR-0065 § 2).
const CLUSTER_PIXEL_RADIUS: f32 = 16.0;

/// Склеенные на экране маркеры (вторая ступень агрегации, ADR-0065 § 2).
struct MarkerCluster {
    /// Исходные индексы `GlobeMarker` в кластере; порядок — слой карты.
    members: Vec<usize>,
    /// Экранная позиция чипа; первый член — стабильный якорь.
    pos: egui::Pos2,
    /// Станций всего: сумма `GlobeMarker.count`.
    stations: usize,
}

/// Склеивает видимые маркеры в кластеры по экранному расстоянию. Первая
/// ступень (гео 0.01°) уже отработала в `build_markers`; здесь — то, что
/// слипается на текущем зуме. Пересобирается каждый кадр, пока камера едет.
fn cluster_screens(markers: &[GlobeMarker], screens: &[(usize, egui::Pos2)]) -> Vec<MarkerCluster> {
    let cell = CLUSTER_PIXEL_RADIUS.max(1.0);
    let mut clusters: Vec<MarkerCluster> = Vec::new();
    let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for &(index, pos) in screens {
        let key = ((pos.x / cell).floor() as i32, (pos.y / cell).floor() as i32);
        let mut hit = None;
        'search: for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(ids) = grid.get(&(key.0 + dx, key.1 + dy)) {
                    for &id in ids {
                        if clusters[id].pos.distance(pos) <= CLUSTER_PIXEL_RADIUS {
                            hit = Some(id);
                            break 'search;
                        }
                    }
                }
            }
        }
        if let Some(id) = hit {
            let cluster = &mut clusters[id];
            cluster.members.push(index);
            cluster.stations += markers[index].count();
        } else {
            let id = clusters.len();
            clusters.push(MarkerCluster {
                members: vec![index],
                pos,
                stations: markers[index].count(),
            });
            grid.entry(key).or_default().push(id);
        }
    }
    clusters
}

/// Кластер под курсором: слот в массиве кластеров (порядок — слой карты).
fn nearest_cluster(clusters: &[MarkerCluster], position: egui::Pos2) -> Option<usize> {
    let radius_sq = MARKER_CLICK_RADIUS * MARKER_CLICK_RADIUS;
    clusters
        .iter()
        .enumerate()
        .filter_map(|(slot, cluster)| {
            let distance_sq = (cluster.pos - position).length_sq();
            (distance_sq <= radius_sq).then_some((distance_sq, slot))
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, slot)| slot)
}

/// Подпись маркера: имя, а при наведении — место, качество потока и рейтинг.
/// Размер и цвет маркера от этих данных больше не зависят.
fn marker_label(marker: &GlobeMarker) -> MarkerLabel {
    let station = marker.station();
    let name = single_line(&station.name);
    let name = if name.is_empty() {
        "Untitled".to_owned()
    } else {
        truncate(&name, 40)
    };
    let title = if marker.count() > 1 {
        format!("{name} · +{}", marker.count() - 1)
    } else {
        name
    };

    let votes = marker.popularity.round().max(0.0) as u64;
    let rating = match votes {
        0 => String::new(),
        1 => "1 vote".to_owned(),
        votes => format!("{votes} votes"),
    };
    let detail = [
        row_place(station),
        if marker.bitrate > 0 {
            format!("{} kbps", marker.bitrate)
        } else {
            String::new()
        },
        single_line(&station.codec).to_uppercase(),
        rating,
    ]
    .into_iter()
    .filter(|part| !part.trim().is_empty())
    .collect::<Vec<_>>()
    .join(" · ");

    MarkerLabel {
        title,
        details: (!detail.is_empty()).then_some(detail),
    }
}

/// Подпись кластера: имя представителя и общее число станций, склеенных на
/// экране (ADR-0065 § 2). Когда кластер — одна гео-группа, остаётся `marker_label`.
fn cluster_label(markers: &[GlobeMarker], cluster: &MarkerCluster) -> MarkerLabel {
    let rep = markers[cluster.members[0]].station();
    let mut label = marker_label(&markers[cluster.members[0]]);
    if cluster.stations > markers[cluster.members[0]].count() {
        let name = single_line(&rep.name);
        let name = if name.is_empty() {
            "Untitled".to_owned()
        } else {
            truncate(&name, 40)
        };
        label.title = format!("{name} · +{}", cluster.stations - 1);
    }
    label
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::markers::{build_markers, geo_station};

    /// Маркеры одной серии для тестов кластеризации.
    fn stacked_markers(count: usize) -> Vec<GlobeMarker> {
        build_markers(
            (0..count)
                .map(|index| geo_station(&format!("m{index}"), 10.0 + index as f64, 10.0))
                .collect(),
        )
    }

    #[test]
    fn nearest_cluster_uses_click_radius() {
        let markers = stacked_markers(2);
        let screens = vec![(0, egui::pos2(10.0, 10.0)), (1, egui::pos2(100.0, 100.0))];
        let clusters = cluster_screens(&markers, &screens);

        assert_eq!(nearest_cluster(&clusters, egui::pos2(14.0, 12.0)), Some(0));
        assert_eq!(nearest_cluster(&clusters, egui::pos2(50.0, 50.0)), None);
    }

    /// Регрессия индекса наведения: слот в массиве кластеров не должен
    /// подменяться исходным индексом `GlobeMarker`.
    #[test]
    fn nearest_cluster_returns_visible_slot_not_source_index() {
        let markers = stacked_markers(9);
        // Первый видимый слот — станция No7 (далеко), второй — No8 (под курсором).
        let screens = vec![(7, egui::pos2(300.0, 300.0)), (8, egui::pos2(12.0, 10.0))];
        let clusters = cluster_screens(&markers, &screens);

        let slot = nearest_cluster(&clusters, egui::pos2(13.0, 11.0)).expect("под курсором");
        // Представитель и члены достаются через `clusters[slot].members`.
        assert_eq!(clusters[slot].members[0], 8);
    }

    #[test]
    fn cluster_screens_merges_neighbours_and_sums_stations() {
        // Две станции в одной гео-точке (count = 2) и одна в стороне.
        let markers = build_markers(vec![
            geo_station("a", 10.0, 10.0),
            geo_station("b", 10.0, 10.0),
            geo_station("c", 30.0, 30.0),
        ]);
        assert_eq!(markers.len(), 2, "гео-склейка уже сработала");
        let screens = vec![(0, egui::pos2(10.0, 10.0)), (1, egui::pos2(20.0, 12.0))];

        let clusters = cluster_screens(&markers, &screens);
        assert_eq!(clusters.len(), 1, "10 px — ближе радиуса склейки");
        assert_eq!(clusters[0].stations, 3, "2 + 1 станции");
        assert_eq!(clusters[0].members, vec![0, 1]);
    }

    #[test]
    fn marker_label_reports_group_size() {
        let station = geo_station("a", 52.52, 13.405);
        let mut marker = GlobeMarker {
            stations: vec![station.clone(), station.clone(), station],
            lat: 52.52,
            lon: 13.405,
            popularity: 40.0,
            bitrate: 128,
        };
        marker.stations[0].name = "Groove\nSalad".to_owned();

        let label = marker_label(&marker);
        assert_eq!(label.title, "Groove Salad · +2");
        assert_eq!(
            label.details.as_deref(),
            Some("New York, United States · 128 kbps · MP3 · 40 votes")
        );
    }

    #[test]
    fn marker_label_omits_empty_details() {
        let mut station = geo_station("a", 52.52, 13.405);
        station.name = "Night Waves".to_owned();
        station.state.clear();
        station.country.clear();
        station.countrycode.clear();
        station.codec.clear();
        let marker = GlobeMarker {
            stations: vec![station],
            lat: 52.52,
            lon: 13.405,
            popularity: 0.0,
            bitrate: 0,
        };

        let label = marker_label(&marker);
        assert_eq!(label.title, "Night Waves");
        assert_eq!(label.details, None);
    }

    #[test]
    fn marker_label_reports_a_single_vote() {
        let mut station = geo_station("a", 52.52, 13.405);
        station.state.clear();
        station.country.clear();
        station.countrycode.clear();
        station.codec.clear();
        let marker = GlobeMarker {
            stations: vec![station],
            lat: 52.52,
            lon: 13.405,
            popularity: 1.0,
            bitrate: 0,
        };

        let label = marker_label(&marker);
        assert_eq!(label.details.as_deref(), Some("1 vote"));
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
        assert_eq!(country_station_count(&markers, None, "DE"), Some(2));
        assert_eq!(country_station_count(&markers, None, "de"), Some(2));
        assert_eq!(country_station_count(&markers, None, "FR"), Some(1));
        assert_eq!(
            country_station_count(&markers, None, "US"),
            None,
            "без маркеров и без снимка мира счётчик неизвестен, а не ноль"
        );
    }

    #[test]
    fn country_station_count_prefers_world_counts() {
        let mut berlin = geo_station("de1", 52.5200, 13.4050);
        berlin.countrycode = "DE".to_owned();
        let markers = build_markers(vec![berlin]);

        let mut counts = HashMap::new();
        counts.insert("DE".to_owned(), 2_453u32);
        assert_eq!(
            country_station_count(&markers, Some(&counts), "de"),
            Some(2_453)
        );
        assert_eq!(
            country_station_count(&markers, Some(&counts), "FR"),
            None,
            "снимок всей базы авторитетнее видимой выборки"
        );

        let empty = HashMap::new();
        assert_eq!(
            country_station_count(&markers, Some(&empty), "DE"),
            Some(1),
            "пустая карта — снимок ещё не пришёл, считаем маркеры"
        );
    }

    #[test]
    fn country_card_reports_world_counts_and_never_a_bogus_zero() {
        let geo = GeoService::from_embedded().expect("встроенный датасет");
        let country = Country {
            name: "Germany".to_owned(),
            iso_a2: "DE".to_owned(),
            iso_a3: Some("DEU".to_owned()),
        };

        let mut counts = HashMap::new();
        counts.insert("DE".to_owned(), 4_249u32);
        let card = country_card(&country, &[], Some(&counts), &geo, 1.0).expect("карточка страны");
        assert_eq!(card.subtitle, "4249 stations");

        let unknown = country_card(&country, &[], None, &geo, 1.0).expect("карточка страны");
        assert_eq!(
            unknown.subtitle, "",
            "без счётчика подпись пустая, а не «0 stations»"
        );
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

    fn card(key: &str, target: f32) -> CalloutCard {
        CalloutCard::new(
            key.to_owned(),
            key.to_owned(),
            "1 station".to_owned(),
            51.0,
            10.0,
            false,
            target,
        )
    }

    #[test]
    fn callout_fades_in_and_out() {
        let mut state = CalloutState::default();

        update_callout(
            &mut state,
            Some(card("DE", CALLOUT_HOVER_ALPHA)),
            0.05,
            false,
        );
        let shown = state.current.as_ref().expect("карточка появилась");
        assert!(shown.alpha > 0.0 && shown.alpha < CALLOUT_HOVER_ALPHA);

        for _ in 0..20 {
            update_callout(
                &mut state,
                Some(card("DE", CALLOUT_HOVER_ALPHA)),
                0.05,
                false,
            );
        }
        assert!(
            (state.current.as_ref().unwrap().alpha - CALLOUT_HOVER_ALPHA).abs() < 0.01,
            "за 1 с hover-карточка выходит на 0.85"
        );

        update_callout(&mut state, None, 0.05, false);
        for _ in 0..20 {
            update_callout(&mut state, None, 0.05, false);
        }
        assert!(
            state
                .current
                .is_none_or(|card| card.alpha <= CALLOUT_EPSILON)
        );
        assert!(state.previous.is_none());
    }

    #[test]
    fn reduce_motion_skips_the_callout_transition() {
        let mut state = CalloutState::default();

        update_callout(&mut state, Some(card("DE", 1.0)), 0.016, true);
        assert_eq!(state.current.as_ref().expect("карточка").alpha, 1.0);

        update_callout(&mut state, None, 0.016, true);
        assert!(
            state
                .current
                .is_none_or(|card| card.alpha <= CALLOUT_EPSILON)
        );
    }

    #[test]
    fn callout_switch_keeps_the_old_card_for_fade_out() {
        let mut state = CalloutState::default();

        for _ in 0..20 {
            update_callout(&mut state, Some(card("DE", 1.0)), 0.05, false);
        }
        update_callout(&mut state, Some(card("FR", 1.0)), 0.05, false);

        let current = state.current.as_ref().expect("новая страна");
        let previous = state.previous.as_ref().expect("старая ещё видна");
        assert_eq!(current.key, "FR");
        assert!(
            current.alpha < previous.alpha,
            "в кадре смены старая карточка ещё ярче новой — есть кроссфэйд"
        );
        assert_eq!(previous.key, "DE");
        assert_eq!(previous.target, 0.0);
        assert!(
            previous.alpha > 0.5,
            "старая карточка ещё не успела погаснуть"
        );

        for _ in 0..20 {
            update_callout(&mut state, Some(card("FR", 1.0)), 0.05, false);
        }
        assert!(state.previous.is_none(), "исчезающая карточка убирается");
        assert!(state.current.as_ref().unwrap().alpha > 0.99);
    }

    #[test]
    fn callout_stays_quiet_without_a_country() {
        let mut state = CalloutState::default();
        for _ in 0..30 {
            update_callout(&mut state, None, 0.05, false);
        }
        assert!(
            state
                .current
                .is_none_or(|card| card.alpha <= CALLOUT_EPSILON)
        );
        assert!(state.previous.is_none());
    }

    #[test]
    fn clamp_callout_keeps_the_card_inside_the_zone() {
        let rect = egui::Rect::from_min_size(egui::pos2(700.0, 460.0), egui::vec2(140.0, 44.0));
        let clamped = clamp_callout(rect, 10.0, 500.0, 0.0, 800.0).expect("карточка влезает");
        assert!(clamped.right() <= 800.0);
        assert!(clamped.bottom() <= 500.0);
        assert!(clamped.left() >= 0.0 && clamped.top() >= 10.0);

        assert!(
            clamp_callout(rect, 10.0, 30.0, 0.0, 800.0).is_none(),
            "по высоте карточка не помещается — лучше не рисовать"
        );
    }

    #[test]
    fn hover_hits_country_under_cursor() {
        let geo = GeoService::from_embedded().expect("встроенный датасет");
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));

        let mut camera = GlobeCamera::default();
        let deform = GlobeDeform::IDENTITY;
        assert!(
            country_at_position(&camera, viewport, &geo, viewport.center(), &deform).is_none(),
            "в центре по умолчанию океан"
        );

        camera.fly_to(52.52, 13.405, std::time::Duration::from_millis(80));
        while camera.is_animating() {
            camera.update(0.05, &GlobeInput::default());
        }
        let country = country_at_position(&camera, viewport, &geo, viewport.center(), &deform)
            .expect("после доворота под курсором Германия");
        assert_eq!(country.iso_a2, "DE");
    }

    #[test]
    fn marker_screens_follow_the_deformation() {
        let camera = GlobeCamera::default();
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let marker = GlobeMarker {
            stations: vec![geo_station("a", 45.0, 0.0)],
            lat: 45.0,
            lon: 0.0,
            popularity: 0.0,
            bitrate: 0,
        };

        let identity = marker_screens(
            &camera,
            std::slice::from_ref(&marker),
            viewport,
            &GlobeDeform::IDENTITY,
        );
        let squashed = marker_screens(
            &camera,
            std::slice::from_ref(&marker),
            viewport,
            &GlobeDeform::new(0.95),
        );

        assert_eq!(identity.len(), 1);
        assert_eq!(squashed.len(), 1);
        assert!(
            squashed[0].1.y > identity[0].1.y,
            "при вертикальном сжатии маркер опускается к центру"
        );
    }

    /// Синтетический список станций для замера: детерминированное распределение
    /// по обеим полусферам, чтобы отбор задней стороны входил в цену кадра.
    fn synthetic_markers(count: usize) -> Vec<GlobeMarker> {
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        (0..count)
            .map(|index| {
                let value = next();
                let lat = (value % 12_000) as f32 / 100.0 - 60.0;
                let lon = ((value >> 24) % 36_000) as f32 / 100.0 - 180.0;
                GlobeMarker {
                    stations: vec![geo_station(
                        &format!("s{index}"),
                        f64::from(lat),
                        f64::from(lon),
                    )],
                    lat,
                    lon,
                    popularity: 0.0,
                    bitrate: 0,
                }
            })
            .collect()
    }

    /// Ручной замер фона кадра (волна 4.2 плана): звёздное поле, градиент космоса
    /// и виньетка. Решение «ппечь звёзды в mesh» принимается по этому числу.
    /// `cargo test --release backdrop_paint_budget -- --ignored --nocapture`.
    #[test]
    #[ignore = "ручной замер: запускать в release"]
    fn backdrop_paint_budget_probe() {
        use crate::settings::AccentColor;
        let ctx = egui::Context::default();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1100.0, 800.0));
        let palette = Palette::for_ui(egui::Theme::Dark, AccentColor::Blue);
        for mode in ["starfield", "space+vignette", "backdrop"] {
            let mut shapes = 0;
            let mut vertices = 0;
            let mut paint_samples = Vec::with_capacity(40);
            let mut tess_samples = Vec::with_capacity(40);
            for _ in 0..40 {
                let started = std::time::Instant::now();
                ctx.begin_pass(egui::RawInput::default());
                let painter = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Background,
                    egui::Id::unique("backdrop-paint-probe"),
                ));
                if mode != "space+vignette" {
                    paint_starfield(&painter, rect);
                }
                if mode != "starfield" {
                    paint_space(&painter, rect, &palette);
                    paint_vignette(&painter, rect);
                }
                let mut output = ctx.end_pass();
                output.textures_delta.clear();
                paint_samples.push(started.elapsed().as_secs_f64() * 1e3);
                shapes = output.shapes.len();

                let started = std::time::Instant::now();
                let tessellated = ctx.tessellate(output.shapes, output.pixels_per_point);
                tess_samples.push(started.elapsed().as_secs_f64() * 1e3);
                vertices = tessellated
                    .iter()
                    .map(|primitive| match primitive.primitive {
                        egui::epaint::Primitive::Mesh(ref mesh) => mesh.vertices.len(),
                        egui::epaint::Primitive::Callback(_) => 0,
                    })
                    .sum();
                std::hint::black_box(&tessellated);
            }
            paint_samples.sort_by(|a, b| a.total_cmp(b));
            tess_samples.sort_by(|a, b| a.total_cmp(b));
            let quantile = |samples: &[f64], q: f64| {
                samples[((samples.len() - 1) as f64 * q).round() as usize]
            };
            println!(
                "backdrop {mode}: paint p50={:.3} p95={:.3} мс, tessellate p50={:.3} p95={:.3} мс; shapes={shapes}, vertices={vertices}",
                quantile(&paint_samples, 0.5),
                quantile(&paint_samples, 0.95),
                quantile(&tess_samples, 0.5),
                quantile(&tess_samples, 0.95),
            );
        }
    }

    /// Ручной baseline проекции маркеров (план § 3.2, волны 0 и 2). Нужен release:
    /// `cargo test --release marker_projection_budget -- --ignored --nocapture`.
    #[test]
    #[ignore = "ручной замер: запускать в release"]
    fn marker_projection_budget_probe() {
        let camera = GlobeCamera::default();
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1100.0, 800.0));
        let deform = GlobeDeform::IDENTITY;
        for count in [100usize, 200, 500, 1000] {
            let markers = synthetic_markers(count);
            // Прогрев: страницы кода в кэше, а не в бюджете.
            for _ in 0..20 {
                std::hint::black_box(marker_screens(&camera, &markers, viewport, &deform));
            }
            let mut samples = Vec::with_capacity(200);
            for _ in 0..200 {
                let started = std::time::Instant::now();
                let screens = marker_screens(&camera, &markers, viewport, &deform);
                samples.push(started.elapsed().as_secs_f64() * 1e3);
                std::hint::black_box(&screens);
            }
            samples.sort_by(|left, right| left.total_cmp(right));
            let quantile = |q: f64| samples[((samples.len() - 1) as f64 * q).round() as usize];
            // Сколько из этой цены — повторная сборка `view_proj` на точку: ровно
            // то, что убирает батч-проекция волны 2.
            let mut matrix_samples = Vec::with_capacity(200);
            let aspect = viewport.aspect_ratio();
            for _ in 0..200 {
                let started = std::time::Instant::now();
                for _ in 0..count {
                    // `black_box` на аргументе: иначе LLVM выносит сборку матрицы
                    // из цикла и колонка становится нулевой.
                    std::hint::black_box(camera.view_proj(std::hint::black_box(aspect)));
                }
                matrix_samples.push(started.elapsed().as_secs_f64() * 1e3);
            }
            matrix_samples.sort_by(|left, right| left.total_cmp(right));
            let matrix_quantile =
                |q: f64| matrix_samples[((matrix_samples.len() - 1) as f64 * q).round() as usize];
            println!(
                "markers_project N={count}: видимых {}, p50={:.3} мс, p95={:.3} мс, макс={:.3} мс \
                 (view_proj×N: p50={:.3} мс, p95={:.3} мс)",
                marker_screens(&camera, &markers, viewport, &deform).len(),
                quantile(0.5),
                quantile(0.95),
                samples[samples.len() - 1],
                matrix_quantile(0.5),
                matrix_quantile(0.95),
            );
        }
    }
}
