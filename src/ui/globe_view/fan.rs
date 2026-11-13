//! Веер станций кластера и карточка «+N» (ADR-0065 § 3).
//!
//! Веер живёт в presentation: ядро отдаёт `GlobeMarker` и экранные кластеры,
//! а раскладку, порог наведения, закрепление и выбор станции считает этот
//! модуль. Единственный путь выбора — `UiAction::SelectStation`, как у строки
//! списка и одиночного маркера (ADR-0036).

use eframe::egui;

use crate::i18n;
use crate::services::Station;
use crate::session::markers::GlobeMarker;
use crate::settings::Language;
use crate::ui::palette::Palette;
use crate::ui::{UiAction, single_line, truncate};

use super::MarkerCluster;

mod card;
mod paint;

pub(super) use card::card;

/// Наведение на кластер дольше этого порога раскрывает веер.
const CLUSTER_DWELL: f64 = 0.200;
/// Отсрочка закрытия, когда курсор ушёл с маркера и веера.
const CLUSTER_GRACE: f64 = 0.300;
/// Дуга веера: радиус, шаг угла и максимум слотов.
const CLUSTER_ARC_RADIUS: f32 = 52.0;
const CLUSTER_ARC_STEP_DEG: f32 = 22.0;
const CLUSTER_FAN_MAX: usize = 7;
/// Круглый чип станции и его зона попадания.
const FAN_CHIP_RADIUS: f32 = 10.0;
const FAN_CHIP_HIT: f32 = 13.0;
/// Раскрытие веера: быстрый вылет без упругой отдачи.
const FAN_OPEN_SECONDS: f32 = 0.120;
const FAN_GROW_MIN: f32 = 0.72;
/// Карточка «+N»: ширина, высота строки и видимое окно списка.
const FAN_CARD_WIDTH: f32 = 300.0;
const FAN_CARD_HEADER: f32 = 48.0;
const FAN_CARD_ROW: f32 = 42.0;
const FAN_CARD_VISIBLE_ROWS: usize = 6;
const FAN_CARD_FOOT: f32 = 8.0;
const FAN_CARD_GAP: f32 = 14.0;
const FAN_CARD_PADDING: f32 = 8.0;
/// Минимальный просвет до карточек страны и станции при выборе места.
const FAN_CARD_AVOID: f32 = 6.0;

/// Состояние веера между кадрами.
#[derive(Debug, Default)]
pub(crate) struct FanState {
    /// Ключ открытого кластера: UUID первой станции в порядке каталога.
    open: Option<String>,
    /// Закрепление кликом: уход курсора не закрывает веер.
    pinned: bool,
    /// Кандидат на раскрытие и время прихода курсора.
    dwell: Option<(String, f64)>,
    /// Начало отсрочки закрытия после ухода курсора.
    grace: Option<f64>,
    /// Время фактического раскрытия: вылет чипов.
    opened_at: f64,
    /// Клавиатурный фокус по слотам.
    focus: usize,
    /// Фокус показан только после первого ←/→: мышь не подсвечивает слот.
    keyboard_focus: bool,
    /// Открыта карточка «+N» со списком всех станций кластера.
    card_open: bool,
    /// Карточка только что открылась: клик по слоту «+N» не должен
    /// провалиться в строку под курсором.
    card_fresh: bool,
    /// Прокрутка карточки, px.
    card_scroll: f32,
    /// Геометрия прошлого кадра: клик и колесо не проваливаются на глобус.
    chips: [Option<egui::Rect>; CLUSTER_FAN_MAX],
    card: Option<egui::Rect>,
}

impl FanState {
    pub(crate) fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// Точка занята веером: глобус не должен её вращать, масштабировать и
    /// трактовать клик как «мимо маркера».
    pub(crate) fn blocks_point(&self, point: egui::Pos2) -> bool {
        self.chips.iter().flatten().any(|rect| rect.contains(point))
            || self.card.is_some_and(|rect| rect.contains(point))
    }

    /// Клик по кластеру закрепляет веер (ADR-0065 § 3).
    pub(crate) fn pin(&mut self, key: &str, now: f64) {
        if self.open.as_deref() != Some(key) {
            self.opened_at = now;
            self.focus = 0;
            self.keyboard_focus = false;
            self.card_open = false;
            self.card_fresh = false;
            self.card_scroll = 0.0;
            self.card = None;
            self.chips = [None; CLUSTER_FAN_MAX];
        }
        self.open = Some(key.to_owned());
        self.pinned = true;
        self.dwell = None;
        self.grace = None;
    }

    pub(crate) fn close(&mut self) {
        self.open = None;
        self.pinned = false;
        self.dwell = None;
        self.grace = None;
        self.focus = 0;
        self.keyboard_focus = false;
        self.card_open = false;
        self.card_fresh = false;
        self.card_scroll = 0.0;
        self.chips = [None; CLUSTER_FAN_MAX];
        self.card = None;
    }

    /// Будильник порога наведения или отсрочки закрытия.
    pub(crate) fn pending_deadline(&self, now: f64) -> Option<f64> {
        if let Some((_, since)) = &self.dwell {
            return Some((CLUSTER_DWELL - (now - since)).max(0.0));
        }
        self.grace
            .map(|since| (CLUSTER_GRACE - (now - since)).max(0.0))
    }

    /// Порог наведения, закрепление и отсрочка закрытия. Открытый кластер
    /// обязан присутствовать в текущем кадре и содержать ≥ 2 станций.
    fn advance(
        &mut self,
        markers: &[GlobeMarker],
        clusters: &[MarkerCluster],
        hovered: Option<usize>,
        fan_hover: bool,
        now: f64,
    ) {
        if let Some(key) = self.open.clone()
            && cluster_slot_by_key(clusters, markers, &key).is_none()
        {
            self.close();
        }

        let hovered = hovered
            .and_then(|slot| clusters.get(slot))
            .filter(|cluster| cluster.stations >= 2);
        let hovered_key = hovered.and_then(|cluster| cluster_key(markers, cluster));

        // Закреплённый веер не реагирует на уход курсора; Esc и клик в пустоту
        // снимают закрепление (ADR-0065 § 3).
        if self.pinned {
            return;
        }

        match (self.open.as_deref(), hovered_key) {
            (Some(open), Some(key)) if open == key => {
                self.grace = None;
            }
            (Some(_), Some(key)) => {
                self.close();
                self.dwell = Some((key.to_owned(), now));
            }
            (Some(_), None) => {
                if fan_hover {
                    self.grace = None;
                } else if self.grace.is_none() {
                    self.grace = Some(now);
                }
            }
            (None, Some(key)) => match &self.dwell {
                Some((dwell_key, since)) if dwell_key == key => {
                    if now - since >= CLUSTER_DWELL {
                        self.open = Some(key.to_owned());
                        self.opened_at = now;
                        self.focus = 0;
                        self.keyboard_focus = false;
                        self.card_open = false;
                        self.card_fresh = false;
                        self.card_scroll = 0.0;
                        self.card = None;
                        self.grace = None;
                        self.dwell = None;
                    }
                }
                _ => self.dwell = Some((key.to_owned(), now)),
            },
            (None, None) => {
                self.dwell = None;
                self.grace = None;
            }
        }

        if let Some(since) = self.grace
            && now - since >= CLUSTER_GRACE
        {
            self.close();
        }
    }
}

/// Вход кадра веера, не зависящий от состояния: курсор, проекция и подписи.
#[derive(Clone, Copy)]
pub(super) struct FanInput<'a> {
    pub hovered: Option<usize>,
    pub pointer: Option<egui::Pos2>,
    pub clicked: Option<egui::Pos2>,
    pub now: f64,
    pub globe_center: egui::Pos2,
    pub bounds: egui::Rect,
    /// Свободная зона между HUD, transport и панелью.
    pub zone: Option<egui::Rect>,
    pub language: Language,
    pub current_uuid: Option<&'a str>,
    pub selected_uuid: Option<&'a str>,
    pub reduce_motion: bool,
}

#[derive(Debug, Default)]
pub(super) struct FanOutcome {
    pub actions: Vec<UiAction>,
    /// Клик пришёлся на веер: глобус его не обрабатывает.
    pub clicked: bool,
}

/// Один слот веера: станция или «+N».
#[derive(Clone, Copy)]
struct FanSlot<'a> {
    station: Option<&'a Station>,
    more: Option<usize>,
    pos: egui::Pos2,
    alpha: f32,
}

struct FanLayout<'a> {
    key: &'a str,
    slots: [Option<FanSlot<'a>>; CLUSTER_FAN_MAX],
    len: usize,
}

impl<'a> FanLayout<'a> {
    fn get(&self, index: usize) -> Option<FanSlot<'a>> {
        self.slots.get(index).copied().flatten()
    }

    fn contains(&self, point: egui::Pos2) -> bool {
        self.slots
            .iter()
            .flatten()
            .any(|slot| slot.pos.distance(point) <= FAN_CHIP_HIT)
    }
}

/// Станция кластера по сквозному индексу в порядке каталога.
pub(super) fn cluster_station<'a>(
    markers: &'a [GlobeMarker],
    cluster: &MarkerCluster,
    mut index: usize,
) -> Option<&'a Station> {
    for &marker_index in &cluster.members {
        let stations = &markers.get(marker_index)?.stations;
        if index < stations.len() {
            return stations.get(index);
        }
        index -= stations.len();
    }
    None
}

/// Ключ кластера: UUID его первой станции. Состав кластера меняется с зумом,
/// но первая станция в порядке каталога — стабильный якорь между кадрами.
pub(super) fn cluster_key<'a>(
    markers: &'a [GlobeMarker],
    cluster: &MarkerCluster,
) -> Option<&'a str> {
    markers
        .get(*cluster.members.first()?)?
        .stations
        .first()
        .map(|station| station.stationuuid.as_str())
}

fn cluster_slot_by_key(
    clusters: &[MarkerCluster],
    markers: &[GlobeMarker],
    key: &str,
) -> Option<usize> {
    clusters
        .iter()
        .position(|cluster| cluster.stations >= 2 && cluster_key(markers, cluster) == Some(key))
}

/// Дуга слотов в сторону от центра планеты (ADR-0065 § 3).
fn fan_layout<'a>(
    markers: &'a [GlobeMarker],
    cluster: &MarkerCluster,
    globe_center: egui::Pos2,
    bounds: egui::Rect,
    grow: f32,
) -> Option<FanLayout<'a>> {
    let total = cluster.stations;
    if total < 2 {
        return None;
    }
    let visible = total.min(CLUSTER_FAN_MAX - usize::from(total > CLUSTER_FAN_MAX));
    let has_more = total > CLUSTER_FAN_MAX;
    let len = visible + usize::from(has_more);

    let direction = cluster.pos - globe_center;
    let base = if direction.length_sq() > 1.0 {
        direction.y.atan2(direction.x)
    } else {
        0.0
    };
    let step = CLUSTER_ARC_STEP_DEG.to_radians();
    let radius = CLUSTER_ARC_RADIUS * (FAN_GROW_MIN + (1.0 - FAN_GROW_MIN) * grow);
    let margin = FAN_CHIP_HIT + 4.0;

    let mut slots = [None; CLUSTER_FAN_MAX];
    for (index, slot) in slots.iter_mut().enumerate().take(len) {
        let offset = index as f32 - (len as f32 - 1.0) * 0.5;
        let angle = base + offset * step;
        let pos = cluster.pos + egui::vec2(angle.cos(), angle.sin()) * radius;
        let pos = egui::pos2(
            pos.x.clamp(bounds.left() + margin, bounds.right() - margin),
            pos.y.clamp(bounds.top() + margin, bounds.bottom() - margin),
        );
        *slot = Some(FanSlot {
            station: (index < visible)
                .then(|| cluster_station(markers, cluster, index))
                .flatten(),
            more: (has_more && index + 1 == len).then_some(total - visible),
            pos,
            alpha: grow,
        });
    }

    Some(FanLayout {
        key: cluster_key(markers, cluster)?,
        slots,
        len,
    })
}

/// Обновляет порог наведения и закрепление до отрисовки маркеров: подпись
/// маркера не должна мигать под уже раскрывшимся веером.
pub(super) fn advance(
    state: &mut FanState,
    markers: &[GlobeMarker],
    clusters: &[MarkerCluster],
    input: &FanInput<'_>,
) {
    // Веер — popup Focus-стола, как callout: в капсулах и панели свободной
    // зоны нет, клик по кластеру остаётся выбором представителя.
    if input.zone.is_none() {
        state.close();
        return;
    }
    // Геометрия до решения: нужна только для «курсор над веером».
    let before = state.open.as_deref().and_then(|key| {
        let slot = cluster_slot_by_key(clusters, markers, key)?;
        fan_layout(
            markers,
            &clusters[slot],
            input.globe_center,
            input.bounds,
            1.0,
        )
    });
    let fan_hover = input.pointer.is_some_and(|point| {
        state.blocks_point(point) || before.as_ref().is_some_and(|layout| layout.contains(point))
    });
    state.advance(markers, clusters, input.hovered, fan_hover, input.now);
}

/// Чипы веера. Возвращает действия выбора; клик по чипу закрывает веер.
/// Карточка «+N» рисуется отдельно, после callout страны и станции, чтобы
/// соблюсти порядок обхода прямоугольников (ADR-0065 § 3).
pub(super) fn chips(
    ui: &mut egui::Ui,
    state: &mut FanState,
    markers: &[GlobeMarker],
    clusters: &[MarkerCluster],
    input: FanInput<'_>,
    palette: &Palette,
) -> FanOutcome {
    let mut outcome = FanOutcome::default();

    let Some(cluster_slot) = state
        .open
        .as_deref()
        .and_then(|key| cluster_slot_by_key(clusters, markers, key))
    else {
        state.close();
        return outcome;
    };
    let cluster = &clusters[cluster_slot];
    let grow = if input.reduce_motion {
        1.0
    } else {
        let raw = ((input.now - state.opened_at) as f32 / FAN_OPEN_SECONDS).clamp(0.0, 1.0);
        1.0 - (1.0 - raw).powi(3)
    };
    let Some(layout) = fan_layout(markers, cluster, input.globe_center, input.bounds, grow) else {
        state.close();
        return outcome;
    };

    if layout.slots.iter().flatten().count() < 2 {
        state.close();
        return outcome;
    }
    state.focus = state.focus.min(layout.len - 1);

    // Клавиатура: ←/→ по слотам, Enter — играть, Esc — закрыть (ADR-0065 § 3).
    // Поля ввода не перехватываются: пока egui ждёт текст, веер молчит.
    if !ui.ctx().egui_wants_keyboard_input() {
        let (left, right) = ui.input_mut(|input| {
            (
                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft),
                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight),
            )
        });
        if left || right {
            state.pinned = true;
            state.keyboard_focus = true;
            let step = if left { layout.len - 1 } else { 1 };
            state.focus = (state.focus + step) % layout.len;
        }
        let escape =
            ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if escape {
            state.close();
            return outcome;
        }
        let enter =
            ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        if enter && let Some(slot) = layout.get(state.focus) {
            state.pinned = true;
            if let Some(station) = slot.station {
                outcome
                    .actions
                    .push(UiAction::SelectStation(station.clone()));
                state.close();
                return outcome;
            }
            if slot.more.is_some() {
                state.card_open = true;
                state.card_fresh = true;
            }
        }
    }

    let hovered_slot = input.pointer.and_then(|point| {
        (0..layout.len).find(|index| {
            layout
                .get(*index)
                .is_some_and(|slot| slot.pos.distance(point) <= FAN_CHIP_HIT)
        })
    });
    if hovered_slot.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let text = i18n::catalog(input.language);

    state.chips = [None; CLUSTER_FAN_MAX];
    for index in 0..layout.len {
        let Some(slot) = layout.get(index) else {
            continue;
        };
        let focused = state.keyboard_focus && state.focus == index;
        let active = hovered_slot == Some(index) || focused;
        paint::paint_chip(
            ui.painter(),
            &slot,
            active,
            focused,
            palette,
            input.language,
        );

        let hit = egui::Rect::from_center_size(
            slot.pos,
            egui::vec2(FAN_CHIP_HIT * 2.0, FAN_CHIP_HIT * 2.0),
        );
        let response = ui.interact(
            hit,
            egui::Id::unique(("orbitfm_fan_chip", layout.key, index)),
            egui::Sense::click(),
        );
        let label = slot.station.map_or_else(
            || {
                i18n::fill(
                    text.panel.cluster_more,
                    &[("count", &slot.more.unwrap_or_default().to_string())],
                )
            },
            |station| paint::station_name(station, input.language),
        );
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label.as_str()));
        if response.clicked() {
            outcome.clicked = true;
            state.pinned = true;
            if let Some(station) = slot.station {
                outcome
                    .actions
                    .push(UiAction::SelectStation(station.clone()));
                state.close();
                return outcome;
            }
            if slot.more.is_some() {
                state.card_open = true;
                state.card_fresh = true;
            }
        }
        if state.is_open() {
            state.chips[index] = Some(hit);
        }
    }

    if let Some(slot) = hovered_slot.and_then(|index| layout.get(index))
        && let Some(station) = slot.station
    {
        let title = single_line(&station.name);
        let title = if title.is_empty() {
            text.common.untitled.to_owned()
        } else {
            truncate(&title, 34)
        };
        let detail = paint::station_detail(station);
        let detail = if detail.is_empty() {
            None
        } else {
            Some(detail)
        };
        paint::paint_tooltip(
            ui.painter(),
            input.bounds,
            slot.pos,
            &title,
            detail.as_deref(),
            palette,
            slot.alpha,
        );
    }

    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::markers::{build_markers, geo_station};

    fn stacked(count: usize) -> Vec<GlobeMarker> {
        build_markers(
            (0..count)
                .map(|index| geo_station(&format!("m{index}"), 10.0, 10.0))
                .collect(),
        )
    }

    fn cluster_of(markers: &[GlobeMarker]) -> MarkerCluster {
        MarkerCluster {
            members: vec![0],
            pos: egui::pos2(200.0, 200.0),
            stations: markers[0].count(),
        }
    }

    fn bounds() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0))
    }

    #[test]
    fn fan_opens_only_after_dwell() {
        let markers = stacked(3);
        let clusters = vec![cluster_of(&markers)];
        let mut state = FanState::default();

        state.advance(&markers, &clusters, Some(0), false, 0.0);
        assert!(!state.is_open(), "порог ещё не прошёл");
        state.advance(&markers, &clusters, Some(0), false, 0.19);
        assert!(!state.is_open());
        state.advance(&markers, &clusters, Some(0), false, 0.21);
        assert!(state.is_open(), "200 мс наведения раскрывают веер");
    }

    #[test]
    fn cluster_of_one_opens_no_fan() {
        let markers = stacked(1);
        // Кластер из одной станции даже при долгом наведении молчит.
        let mut clusters = vec![cluster_of(&markers)];
        clusters[0].stations = 1;
        let mut state = FanState::default();

        state.advance(&markers, &clusters, Some(0), false, 0.0);
        state.advance(&markers, &clusters, Some(0), false, 10.0);
        assert!(!state.is_open());
    }

    #[test]
    fn fan_closes_after_grace_but_pin_survives() {
        let markers = stacked(3);
        let clusters = vec![cluster_of(&markers)];
        let mut state = FanState::default();
        state.advance(&markers, &clusters, Some(0), false, 0.0);
        state.advance(&markers, &clusters, Some(0), false, 0.2);
        assert!(state.is_open());

        state.advance(&markers, &clusters, None, false, 0.5);
        assert!(state.is_open(), "отсрочка закрытия ещё идёт");
        state.advance(&markers, &clusters, None, false, 0.81);
        assert!(!state.is_open(), "300 мс без курсора закрывают веер");

        state.pin("m0", 1.0);
        state.advance(&markers, &clusters, None, false, 10.0);
        assert!(state.is_open(), "закреплённый веер не боится ухода курсора");
    }

    #[test]
    fn pin_to_another_cluster_drops_stale_card_geometry() {
        let mut state = FanState::default();
        state.pin("first", 0.0);
        state.card = Some(bounds());
        state.pin("second", 1.0);

        assert!(
            state.card.is_none(),
            "карточка прошлого кластера не должна блокировать клики"
        );
    }

    #[test]
    fn fan_keeps_six_stations_and_more_chip() {
        let markers = stacked(20);
        let cluster = cluster_of(&markers);
        let layout = fan_layout(&markers, &cluster, egui::pos2(100.0, 100.0), bounds(), 1.0)
            .expect("веер есть");

        assert_eq!(layout.len, CLUSTER_FAN_MAX, "не больше семи слотов");
        let last = layout.get(CLUSTER_FAN_MAX - 1).expect("слот «+N»");
        assert_eq!(last.more, Some(14));
        assert!(last.station.is_none());
        assert_eq!(
            layout
                .get(0)
                .and_then(|slot| slot.station)
                .map(|s| s.stationuuid.as_str()),
            Some("m0"),
            "порядок слотов — порядок каталога"
        );
        assert_eq!(
            layout
                .get(5)
                .and_then(|slot| slot.station)
                .map(|s| s.stationuuid.as_str()),
            Some("m5")
        );
    }

    #[test]
    fn cluster_station_indexes_across_geo_groups() {
        let mut first = geo_station("a", 10.0, 10.0);
        first.name = "A".to_owned();
        let second = geo_station("b", 11.0, 11.0);
        let third = geo_station("c", 11.0, 11.0);
        let markers = build_markers(vec![first, second, third]);
        assert_eq!(markers.len(), 2);
        let cluster = MarkerCluster {
            members: vec![0, 1],
            pos: egui::pos2(200.0, 200.0),
            stations: 3,
        };

        let order: Vec<&str> = (0..3)
            .filter_map(|index| cluster_station(&markers, &cluster, index))
            .map(|station| station.stationuuid.as_str())
            .collect();
        assert_eq!(order, ["a", "b", "c"]);
    }

    #[test]
    fn card_avoids_country_and_station_rects() {
        let zone = bounds();
        let country = egui::Rect::from_min_size(egui::pos2(216.0, 170.0), egui::vec2(180.0, 60.0));
        let size = egui::vec2(280.0, 260.0);
        let rect = paint::place_card(egui::pos2(200.0, 200.0), size, zone, &[country]);

        assert!(zone.contains(rect.min) && zone.contains(rect.max));
        assert!(!rect.intersects(country.expand(FAN_CARD_AVOID)));
    }
}
