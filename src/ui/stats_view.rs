//! Раздел «Статистика» окна настроек (ADR-0049 § 7).
//!
//! Модуль только рисует готовую сводку хранилища и возвращает [`UiAction`]:
//! доли, минуты и подписи считаются здесь, а накоплением и файлом заведует
//! `session::stats`. Отдельный файл держит `settings_panel` в разумной высоте.

use eframe::egui;

use crate::i18n;
use crate::stats::StatsSnapshot;

use super::UiAction;
use super::controls::switch_row;
use super::palette::Palette;
use super::settings_panel::{card, divider, hint};
use super::widgets::{action_pill, faded, faded_palette};

/// Высота строки списка: имя, полоска-доля и значение в одной полоске.
const ROW_HEIGHT: f32 = 22.0;
/// Отступ текста от краёв строки списка.
const ROW_INSET: f32 = 7.0;
const LABEL_SIZE: f32 = 11.5;
const VALUE_SIZE: f32 = 11.0;
const TITLE_SIZE: f32 = 10.5;

pub(crate) struct StatsView<'a> {
    pub snapshot: &'a StatsSnapshot,
    /// Значение `stats_enabled` из сохранённых настроек.
    pub enabled: bool,
    /// Кнопка «Очистить» уже нажата один раз и ждёт подтверждения.
    pub clear_armed: bool,
    /// Раздел только что очистили: ответная строка вместо пустых списков.
    pub just_cleared: bool,
}
/// Одна строка рейтинга: подпись и секунды.
struct Row {
    label: String,
    seconds: u64,
}

/// Содержимое категории: сводка за всё время и за окна, тумблер с очисткой
/// и три списка топ-10.
pub(crate) fn draw(
    ui: &mut egui::Ui,
    palette: &Palette,
    strings: &i18n::SettingsText,
    view: &StatsView<'_>,
    cards: [f32; 3],
    actions: &mut Vec<UiAction>,
) {
    let alpha = cards[0];
    card(ui, palette, alpha, |ui| {
        let dim = faded_palette(palette, alpha);
        text_row(
            ui,
            &dim,
            strings.stats_total,
            &duration(strings, view.snapshot.total_seconds),
        );
        divider(ui, palette, alpha);
        text_row(
            ui,
            &dim,
            strings.stats_week,
            &duration(strings, view.snapshot.week_seconds),
        );
        divider(ui, palette, alpha);
        text_row(
            ui,
            &dim,
            strings.stats_month,
            &duration(strings, view.snapshot.month_seconds),
        );
        ui.add_space(2.0);
        // Очистка показывает себя в этом же разделе, нового тоста нет (§ 7).
        let note = if view.just_cleared {
            strings.stats_cleared
        } else {
            strings.stats_hint
        };
        hint(ui, palette, note, alpha);
    });

    let alpha = cards[1];
    ui.add_space(10.0);
    card(ui, palette, alpha, |ui| {
        let dim = faded_palette(palette, alpha);
        if switch_row(
            ui,
            &dim,
            strings.stats_enabled,
            Some(strings.stats_enabled_hint),
            view.enabled,
            true,
        )
        .clicked()
        {
            actions.push(UiAction::SetStatsEnabled(!view.enabled));
        }
        divider(ui, palette, alpha);
        let label = if view.clear_armed {
            strings.stats_clear_confirm
        } else {
            strings.stats_clear
        };
        if action_pill(ui, &dim, label).clicked() {
            if view.clear_armed {
                actions.push(UiAction::ClearStats);
            } else {
                actions.push(UiAction::ArmStatsClear);
            }
        }
    });

    let alpha = cards[2];
    ui.add_space(10.0);
    card(ui, palette, alpha, |ui| {
        if view.snapshot.total_seconds == 0 {
            hint(ui, palette, strings.stats_empty, alpha);
            return;
        }
        let dim = faded_palette(palette, alpha);
        let stations: Vec<Row> = view
            .snapshot
            .stations
            .iter()
            .map(|station| Row {
                label: station.name.clone(),
                seconds: station.seconds,
            })
            .collect();
        let countries: Vec<Row> = view
            .snapshot
            .countries
            .iter()
            .map(|country| Row {
                label: country.code.clone(),
                seconds: country.seconds,
            })
            .collect();
        let tags: Vec<Row> = view
            .snapshot
            .tags
            .iter()
            .map(|tag| Row {
                label: tag.tag.clone(),
                seconds: tag.seconds,
            })
            .collect();
        group(ui, &dim, strings, strings.stats_stations, &stations);
        group(ui, &dim, strings, strings.stats_countries, &countries);
        group(ui, &dim, strings, strings.stats_tags, &tags);
    });
}

/// Один список: заголовок и строки с полоской-долей относительно лидера.
fn group(
    ui: &mut egui::Ui,
    dim: &Palette,
    strings: &i18n::SettingsText,
    title: &str,
    rows: &[Row],
) {
    if rows.is_empty() {
        return;
    }
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(title)
            .size(TITLE_SIZE)
            .color(dim.muted)
            .weak(),
    );
    ui.add_space(3.0);
    // Доли считаются от лидера списка: первая строка всегда на всю ширину.
    let leader = rows.iter().map(|row| row.seconds).max().unwrap_or(1).max(1);
    for row in rows {
        let share = (row.seconds as f32 / leader as f32).clamp(0.0, 1.0);
        rank_row(ui, dim, &row.label, &duration(strings, row.seconds), share);
    }
}

/// Строка рейтинга: фон-полоска на всю ширину и доля, заполненная акцентом.
fn rank_row(ui: &mut egui::Ui, palette: &Palette, label: &str, value: &str, share: f32) {
    let width = ui.available_width().max(120.0);
    let rect = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(width, ROW_HEIGHT));
    ui.advance_cursor_after_rect(rect);
    let body = rect.shrink2(egui::vec2(0.0, 2.0));
    ui.painter().rect_filled(body, 6.0, palette.chip_fill);
    if share > 0.0 {
        let mut bar = body;
        bar.max.x = body.left() + (body.width() * share).max(3.0);
        ui.painter()
            .rect_filled(bar, 6.0, faded(palette.accent, 0.28));
    }
    ui.painter().text(
        egui::pos2(rect.left() + ROW_INSET, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(LABEL_SIZE),
        palette.text,
    );
    ui.painter().text(
        egui::pos2(rect.right() - ROW_INSET, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        value,
        egui::FontId::proportional(VALUE_SIZE),
        palette.muted,
    );
}

/// Строка «подпись — значение» для суммарных строк сводки.
fn text_row(ui: &mut egui::Ui, palette: &Palette, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).size(12.5).color(palette.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(value)
                    .size(12.0)
                    .strong()
                    .color(palette.accent),
            );
        });
    });
}

/// Длительность по каталогу: минуты, часы или часы с минутами.
fn duration(strings: &i18n::SettingsText, seconds: u64) -> String {
    let minutes_total = seconds / 60;
    let hours = minutes_total / 60;
    let minutes = minutes_total % 60;
    let hours = hours.to_string();
    let minutes = minutes.to_string();
    if hours == "0" {
        i18n::fill(strings.stats_duration_minutes, &[("minutes", &minutes)])
    } else if minutes == "0" {
        i18n::fill(strings.stats_duration_hours, &[("hours", &hours)])
    } else {
        i18n::fill(
            strings.stats_duration,
            &[("hours", &hours), ("minutes", &minutes)],
        )
    }
}
