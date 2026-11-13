//! Карточка «+N»: список всех станций кластера со скроллом.

use eframe::egui;

use crate::i18n;
use crate::session::markers::GlobeMarker;
use crate::ui::palette::Palette;
use crate::ui::widgets::{contrast_color, glass_panel};
use crate::ui::{UiAction, truncate};

use super::MarkerCluster;
use super::paint::{place_card, station_detail, station_name};
use super::{
    FAN_CARD_FOOT, FAN_CARD_HEADER, FAN_CARD_PADDING, FAN_CARD_ROW, FAN_CARD_VISIBLE_ROWS,
    FAN_CARD_WIDTH, FanInput, FanOutcome, FanState, cluster_slot_by_key, cluster_station,
};

/// Карточка «+N»: «играть лучшую» сверху и прокручиваемый список всех
/// станций кластера в порядке каталога.
pub(crate) fn card(
    ui: &mut egui::Ui,
    state: &mut FanState,
    markers: &[GlobeMarker],
    clusters: &[MarkerCluster],
    input: FanInput<'_>,
    // Прямоугольники callout страны и станции: карточка обходит их.
    avoid: [Option<egui::Rect>; 2],
    palette: &Palette,
) -> FanOutcome {
    let mut outcome = FanOutcome::default();
    if !state.card_open {
        // Самовосстановление: закрытая карточка не оставляет хит-тест.
        state.card = None;
        return outcome;
    }
    let Some(zone) = input.zone else {
        return outcome;
    };
    let Some(key) = state.open.clone() else {
        state.card_open = false;
        state.card_fresh = false;
        state.card = None;
        return outcome;
    };
    let Some(cluster_slot) = cluster_slot_by_key(clusters, markers, &key) else {
        state.close();
        return outcome;
    };
    let cluster = &clusters[cluster_slot];
    if cluster.stations < 2 {
        state.close();
        return outcome;
    }
    // Клик, открывший карточку, уже израсходован слотом «+N»: в этом кадре
    // строки и кнопка не должны поймать его повторно.
    let swallow_click = std::mem::take(&mut state.card_fresh);
    let total = cluster.stations;
    let visible_rows = total.min(FAN_CARD_VISIBLE_ROWS);
    let width = FAN_CARD_WIDTH.min(zone.width());
    let height =
        (FAN_CARD_HEADER + visible_rows as f32 * FAN_CARD_ROW + FAN_CARD_FOOT).min(zone.height());
    let size = egui::vec2(width, height);
    let avoids: Vec<egui::Rect> = avoid.into_iter().flatten().collect();
    let rect = place_card(cluster.pos, size, zone, &avoids);
    state.card = Some(rect);

    let text = i18n::catalog(input.language);
    glass_panel(ui.painter(), rect, palette, 12.0, 1.0);
    ui.painter().rect_stroke(
        rect,
        12.0,
        egui::Stroke::new(1.0, palette.card_stroke),
        egui::StrokeKind::Middle,
    );

    let header = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), FAN_CARD_HEADER));
    let title = i18n::fill(
        text.panel.cluster_show_all,
        &[("count", &total.to_string())],
    );
    ui.painter().text(
        egui::pos2(header.left() + 12.0, header.center().y),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(13.0),
        palette.text,
    );

    // «Играть лучшую» — первая станция в порядке каталога.
    let play_label = text.panel.cluster_play_best;
    let play_color = contrast_color(palette.accent);
    let play_galley = ui.painter().layout_no_wrap(
        play_label.to_owned(),
        egui::FontId::proportional(11.5),
        play_color,
    );
    let play_rect = egui::Rect::from_min_size(
        egui::pos2(
            header.right() - play_galley.size().x - 24.0,
            header.center().y - 13.0,
        ),
        egui::vec2(play_galley.size().x + 16.0, 26.0),
    );
    let play = ui.interact(
        play_rect,
        egui::Id::unique(("orbitfm_fan_best", key.as_str())),
        egui::Sense::click(),
    );
    ui.painter().rect_filled(
        play_rect,
        8.0,
        if play.hovered() {
            palette.accent
        } else {
            palette.accent.gamma_multiply(0.8)
        },
    );
    ui.painter().galley(
        egui::pos2(
            play_rect.center().x - play_galley.size().x / 2.0,
            play_rect.center().y - play_galley.size().y / 2.0,
        ),
        play_galley,
        play_color,
    );
    if play.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    play.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, play_label));
    if !swallow_click
        && play.clicked()
        && let Some(station) = cluster_station(markers, cluster, 0)
    {
        outcome
            .actions
            .push(UiAction::SelectStation(station.clone()));
        outcome.clicked = true;
        state.close();
    }

    ui.painter().line_segment(
        [
            egui::pos2(rect.left() + 8.0, header.bottom() - 1.0),
            egui::pos2(rect.right() - 8.0, header.bottom() - 1.0),
        ],
        egui::Stroke::new(1.0, palette.card_stroke),
    );

    let list = egui::Rect::from_min_max(
        egui::pos2(rect.left() + FAN_CARD_PADDING, header.bottom() + 4.0),
        egui::pos2(
            rect.right() - FAN_CARD_PADDING,
            rect.bottom() - FAN_CARD_FOOT,
        ),
    );
    let content_height = total as f32 * FAN_CARD_ROW;
    let max_scroll = (content_height - list.height()).max(0.0);
    if input.pointer.is_some_and(|point| rect.contains(point)) {
        let wheel = ui.input_mut(|input| {
            let wheel = input.smooth_scroll_delta.y;
            input.smooth_scroll_delta.y = 0.0;
            wheel
        });
        if wheel != 0.0 {
            state.card_scroll = (state.card_scroll - wheel).clamp(0.0, max_scroll);
        }
    }
    state.card_scroll = state.card_scroll.clamp(0.0, max_scroll);
    if let Some(point) = input.clicked
        && rect.contains(point)
    {
        outcome.clicked = true;
    }

    let first = (state.card_scroll / FAN_CARD_ROW).floor() as usize;
    let offset = state.card_scroll - first as f32 * FAN_CARD_ROW;
    let rows = (list.height() / FAN_CARD_ROW).ceil() as usize + 1;
    let clip = ui.painter().with_clip_rect(list);
    for (visible_index, station_index) in (first..total).take(rows).enumerate() {
        let Some(station) = cluster_station(markers, cluster, station_index) else {
            continue;
        };
        let top = list.top() + visible_index as f32 * FAN_CARD_ROW - offset;
        let row = egui::Rect::from_min_max(
            egui::pos2(list.left(), top),
            egui::pos2(list.right(), top + FAN_CARD_ROW),
        );
        let playing = input.current_uuid == Some(station.stationuuid.as_str());
        let selected = input.selected_uuid == Some(station.stationuuid.as_str());
        let response = ui.interact(
            row.intersect(list),
            egui::Id::unique(("orbitfm_fan_row", key.as_str(), station_index)),
            egui::Sense::click(),
        );
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if ui.is_rect_visible(row) {
            if playing || selected {
                clip.rect_filled(row.shrink2(egui::vec2(2.0, 1.0)), 8.0, palette.chip_fill);
                clip.rect_stroke(
                    row.shrink2(egui::vec2(2.0, 1.0)),
                    8.0,
                    egui::Stroke::new(1.2, palette.accent),
                    egui::StrokeKind::Inside,
                );
            } else if response.hovered() {
                clip.rect_filled(row.shrink2(egui::vec2(2.0, 1.0)), 8.0, palette.chip_fill);
            }
            let name = station_name(station, input.language);
            clip.text(
                egui::pos2(row.left() + 8.0, row.top() + 7.0),
                egui::Align2::LEFT_TOP,
                name,
                egui::FontId::proportional(12.0),
                if playing {
                    palette.accent
                } else {
                    palette.text
                },
            );
            let detail = station_detail(station);
            if !detail.is_empty() {
                clip.text(
                    egui::pos2(row.left() + 8.0, row.top() + 23.0),
                    egui::Align2::LEFT_TOP,
                    truncate(&detail, 44),
                    egui::FontId::proportional(10.0),
                    palette.muted,
                );
            }
        }
        let label = station_name(station, input.language);
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label.as_str()));
        if !swallow_click && response.clicked() {
            outcome
                .actions
                .push(UiAction::SelectStation(station.clone()));
            outcome.clicked = true;
            state.close();
            return outcome;
        }
    }
    // Полоса прокрутки: тонкая, без интерактивности.
    if max_scroll > 0.0 {
        let track = egui::Rect::from_min_max(
            egui::pos2(rect.right() - 5.0, list.top()),
            egui::pos2(rect.right() - 3.0, list.bottom()),
        );
        let thumb_h = (list.height() / content_height * track.height()).max(16.0);
        let t = state.card_scroll / max_scroll;
        let thumb = egui::Rect::from_min_size(
            egui::pos2(track.left(), track.top() + (track.height() - thumb_h) * t),
            egui::vec2(track.width(), thumb_h),
        );
        ui.painter().rect_filled(track, 1.0, palette.chip_fill);
        ui.painter()
            .rect_filled(thumb, 1.0, palette.accent.gamma_multiply(0.55));
    }

    outcome
}
