//! Отрисовка чипов, подсказки и размещение карточки «+N».

use eframe::egui;

use crate::i18n;
use crate::services::Station;
use crate::settings::Language;
use crate::ui::palette::Palette;
use crate::ui::station_row::row_place;
use crate::ui::widgets::contrast_color;
use crate::ui::{single_line, truncate};

use super::{FAN_CARD_AVOID, FAN_CARD_GAP, FAN_CHIP_HIT, FAN_CHIP_RADIUS, FanSlot};

/// Инициалы для чипа: первые буквы двух слов, иначе первая буква.
fn initials(name: &str) -> String {
    let mut out = String::new();
    for word in name.split_whitespace() {
        if let Some(letter) = word.chars().find(|letter| letter.is_alphanumeric()) {
            out.extend(letter.to_uppercase());
            if out.chars().count() >= 2 {
                break;
            }
        }
    }
    if out.is_empty() {
        "•".to_owned()
    } else {
        out
    }
}

pub(super) fn station_name(station: &Station, language: Language) -> String {
    let name = single_line(&station.name);
    if name.is_empty() {
        i18n::catalog(language).common.untitled.to_owned()
    } else {
        truncate(&name, 30)
    }
}

pub(super) fn station_detail(station: &Station) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(3);
    let place = row_place(station);
    if !place.is_empty() {
        parts.push(place);
    }
    if station.bitrate > 0 {
        parts.push(format!("{} kbps", station.bitrate));
    }
    if station.votes > 0 {
        parts.push(format!("{} votes", station.votes));
    }
    parts.join(" · ")
}

pub(super) fn paint_chip(
    painter: &egui::Painter,
    slot: &FanSlot<'_>,
    active: bool,
    focused: bool,
    palette: &Palette,
    language: Language,
) {
    let alpha = slot.alpha.clamp(0.0, 1.0);
    let fill = if active {
        palette.accent.gamma_multiply(0.85 * alpha)
    } else {
        palette.card_fill.gamma_multiply(alpha)
    };
    painter.circle_filled(slot.pos, FAN_CHIP_RADIUS, fill);
    painter.circle_stroke(
        slot.pos,
        FAN_CHIP_RADIUS,
        egui::Stroke::new(
            if active { 1.6 } else { 1.0 },
            if active {
                palette.text.gamma_multiply(alpha)
            } else {
                palette.card_stroke.gamma_multiply(alpha)
            },
        ),
    );
    let label = if let Some(station) = slot.station {
        initials(&station.name)
    } else if let Some(more) = slot.more {
        let text = i18n::catalog(language);
        i18n::fill(text.panel.cluster_more, &[("count", &more.to_string())])
    } else {
        "•".to_owned()
    };
    let color = if active {
        contrast_color(palette.accent)
    } else {
        palette.text
    }
    .gamma_multiply(alpha);
    // «+100» и длиннее не влезает в чип радиусом 10 px тем же кеглем.
    let font_size = if label.chars().count() > 3 { 8.0 } else { 9.5 };
    painter.text(
        slot.pos,
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(font_size),
        color,
    );
    if focused {
        painter.circle_stroke(
            slot.pos,
            FAN_CHIP_RADIUS + 3.0,
            egui::Stroke::new(1.8, palette.text.gamma_multiply(alpha)),
        );
    }
}

pub(super) fn paint_tooltip(
    painter: &egui::Painter,
    bounds: egui::Rect,
    anchor: egui::Pos2,
    title: &str,
    detail: Option<&str>,
    palette: &Palette,
    alpha: f32,
) {
    let alpha = alpha.clamp(0.0, 1.0);
    let title_galley = painter.layout_no_wrap(
        title.to_owned(),
        egui::FontId::proportional(12.0),
        palette.text.gamma_multiply(alpha),
    );
    let detail_galley = detail.map(|detail| {
        painter.layout_no_wrap(
            detail.to_owned(),
            egui::FontId::proportional(10.0),
            palette.muted.gamma_multiply(alpha),
        )
    });
    let padding = egui::vec2(9.0, 6.0);
    let gap = 2.0;
    let text_width = title_galley
        .size()
        .x
        .max(detail_galley.as_ref().map_or(0.0, |galley| galley.size().x));
    let text_height = title_galley.size().y
        + detail_galley
            .as_ref()
            .map_or(0.0, |galley| galley.size().y + gap);
    let size = egui::vec2(text_width + padding.x * 2.0, text_height + padding.y * 2.0);
    let mut min = egui::pos2(
        anchor.x - size.x / 2.0,
        anchor.y - FAN_CHIP_HIT - size.y - 8.0,
    );
    if min.y < bounds.top() + 4.0 {
        min.y = anchor.y + FAN_CHIP_HIT + 8.0;
    }
    min.x = min.x.clamp(
        bounds.left() + 4.0,
        (bounds.right() - size.x - 4.0).max(bounds.left() + 4.0),
    );
    min.y = min.y.clamp(
        bounds.top() + 4.0,
        (bounds.bottom() - size.y - 4.0).max(bounds.top() + 4.0),
    );
    let rect = egui::Rect::from_min_size(min, size);
    painter.rect(
        rect,
        egui::CornerRadius::same(7),
        palette
            .card_fill
            .gamma_multiply((0.94 * alpha).clamp(0.0, 1.0)),
        egui::Stroke::new(1.0, palette.accent.gamma_multiply(0.45 * alpha)),
        egui::StrokeKind::Middle,
    );
    painter.galley(rect.min + padding, title_galley, egui::Color32::WHITE);
    if let Some(detail_galley) = detail_galley {
        painter.galley(
            egui::pos2(
                rect.left() + padding.x,
                rect.top() + padding.y + text_height - detail_galley.size().y,
            ),
            detail_galley,
            egui::Color32::WHITE,
        );
    }
}

/// Место карточки: справа, слева, снизу или сверху от кластера; первый
/// вариант, который влезает в свободную зону и не накрывает callout страны
/// и станции (порядок обхода страна → станция → кластер).
pub(super) fn place_card(
    anchor: egui::Pos2,
    size: egui::Vec2,
    zone: egui::Rect,
    avoids: &[egui::Rect],
) -> egui::Rect {
    // Карточка шире плашки страны, поэтому «снизу/сверху по центру» может
    // задеть callout, даже если кластер стоит в его середине. Сдвигаем
    // вертикальные варианты за ближайший callout по оси X.
    let mut below_y = anchor.y + FAN_CARD_GAP;
    let mut above_y = anchor.y - FAN_CARD_GAP - size.y;
    for avoid in avoids {
        let same_column =
            avoid.right() > anchor.x - size.x / 2.0 && avoid.left() < anchor.x + size.x / 2.0;
        if same_column {
            // +1 px: `Rect::intersects` включает касание краёв.
            below_y = below_y.max(avoid.bottom() + FAN_CARD_AVOID + 1.0);
            above_y = above_y.min(avoid.top() - FAN_CARD_AVOID - 1.0 - size.y);
        }
    }
    let candidates = [
        egui::Rect::from_min_size(
            egui::pos2(anchor.x + FAN_CARD_GAP, anchor.y - size.y / 2.0),
            size,
        ),
        egui::Rect::from_min_size(
            egui::pos2(anchor.x - FAN_CARD_GAP - size.x, anchor.y - size.y / 2.0),
            size,
        ),
        egui::Rect::from_min_size(egui::pos2(anchor.x - size.x / 2.0, below_y), size),
        egui::Rect::from_min_size(egui::pos2(anchor.x - size.x / 2.0, above_y), size),
    ];
    let fits = |rect: &egui::Rect| {
        zone.contains(rect.min)
            && zone.contains(rect.max)
            && !avoids
                .iter()
                .any(|avoid| rect.intersects(avoid.expand(FAN_CARD_AVOID)))
    };
    candidates
        .into_iter()
        .find(fits)
        .unwrap_or_else(|| clamp_into(candidates[0], zone))
}

fn clamp_into(mut rect: egui::Rect, zone: egui::Rect) -> egui::Rect {
    let width = rect.width().min(zone.width());
    let height = rect.height().min(zone.height());
    rect.set_width(width);
    rect.set_height(height);
    if rect.left() < zone.left() {
        rect = rect.translate(egui::vec2(zone.left() - rect.left(), 0.0));
    }
    if rect.right() > zone.right() {
        rect = rect.translate(egui::vec2(zone.right() - rect.right(), 0.0));
    }
    if rect.top() < zone.top() {
        rect = rect.translate(egui::vec2(0.0, zone.top() - rect.top()));
    }
    if rect.bottom() > zone.bottom() {
        rect = rect.translate(egui::vec2(0.0, zone.bottom() - rect.bottom()));
    }
    rect
}
