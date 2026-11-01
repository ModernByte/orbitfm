//! Постоянные плавающие подсказки Orbital Desk: карточка управления
//! мышью и легенда тепловой карты стран.

use eframe::egui;

use std::sync::OnceLock;

use crate::i18n::{self, Catalog};
use crate::session::heat::CountryMetric;
use crate::settings::Language;

use super::animation;
use super::country_map::HeatScope;
use super::icons::{MousePose, draw_mouse};
use super::palette::{Palette, heat_color};
use super::widgets::faded;

/// Карточка управления: правый клик — орбита, ЛКМ — страна/маркер,
/// колесо — зум. Рисуются иконки мыши в трёх позах.
///
/// `progress` — прогресс входа зоны из [`animation::Transitions`]: строки
/// появляются снизу вверх одним stagger, каркас рисует `app.rs`.
pub(crate) fn help_card(ui: &mut egui::Ui, palette: &Palette, language: Language, progress: f32) {
    let text = i18n::catalog(language);
    let rows = [
        (
            MousePose::Orbit,
            text.overlay.right_drag,
            text.overlay.orbit,
        ),
        (
            MousePose::Click,
            text.overlay.left_click,
            text.overlay.country_station,
        ),
        (MousePose::Wheel, text.overlay.scroll, text.overlay.zoom),
    ];

    palette.panel(14).show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        for (index, (pose, button, action)) in rows.into_iter().enumerate() {
            let (rect, _) = ui
                .allocate_exact_size(egui::vec2(ui.available_width(), 30.0), egui::Sense::hover());
            if !ui.is_rect_visible(rect) {
                continue;
            }
            let shown = animation::alpha(animation::stagger(index, progress));
            if shown == 0.0 {
                continue;
            }
            let painter = ui.painter();
            draw_mouse(
                painter,
                egui::pos2(rect.left() + 12.0, rect.center().y),
                10.0,
                pose,
                faded(palette.accent, shown),
            );
            let text_x = rect.left() + 30.0;
            painter.text(
                egui::pos2(text_x, rect.center().y - 7.0),
                egui::Align2::LEFT_CENTER,
                button,
                egui::FontId::proportional(11.5),
                faded(palette.text, shown),
            );
            painter.text(
                egui::pos2(text_x, rect.center().y + 8.0),
                egui::Align2::LEFT_CENTER,
                action,
                egui::FontId::proportional(10.5),
                faded(palette.muted, shown),
            );
        }
        // Нижний отступ: без него последняя строка упирается в скругление
        // карточки (жалоба 2026-10-28).
        ui.add_space(8.0);
    });
}

/// Легенда карты стран: выбор метрики, радужный градиент Low → High.
pub(crate) fn country_legend(
    ui: &mut egui::Ui,
    palette: &Palette,
    language: Language,
    progress: f32,
    metric: &mut CountryMetric,
    scope: HeatScope,
) {
    let text = i18n::catalog(language);
    let shown = animation::alpha(progress);
    palette.panel(14).show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        if shown == 0.0 {
            return;
        }

        ui.scope(|ui| {
            ui.multiply_opacity(shown);

            ui.scope(|ui| {
                let visuals = ui.visuals_mut();
                let idle = &mut visuals.widgets.inactive;
                idle.weak_bg_fill = palette.chip_fill;
                idle.bg_stroke = egui::Stroke::new(1.0, palette.chip_stroke);
                idle.corner_radius = egui::CornerRadius::same(8);
                let hovered = &mut visuals.widgets.hovered;
                hovered.weak_bg_fill = palette.chip_fill;
                hovered.bg_stroke = egui::Stroke::new(1.0, palette.accent);
                hovered.corner_radius = egui::CornerRadius::same(8);

                egui::ComboBox::from_id_salt("orbitfm_country_metric")
                    .selected_text(
                        egui::RichText::new(metric_label(text, *metric))
                            .size(11.5)
                            .color(palette.text),
                    )
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for option in CountryMetric::ALL {
                            ui.selectable_value(metric, option, metric_label(text, option));
                        }
                    });
            });

            ui.add_space(7.0);
            let (rect, _) = ui
                .allocate_exact_size(egui::vec2(ui.available_width(), 32.0), egui::Sense::hover());
            if !ui.is_rect_visible(rect) {
                return;
            }
            let painter = ui.painter();
            let bar = egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.top() + 2.0),
                egui::pos2(rect.right(), rect.top() + 11.0),
            );
            let stops = heat_stops();
            let slices = stops.len() - 1;
            for (slice, color) in stops.iter().enumerate().take(slices) {
                let x0 = bar.left() + bar.width() * slice as f32 / slices as f32;
                let x1 = bar.left() + bar.width() * (slice + 1) as f32 / slices as f32;
                let color = if metric.is_on() {
                    *color
                } else {
                    faded(*color, 0.35)
                };
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(x0, bar.top()),
                        egui::pos2(x1 + 0.5, bar.bottom()),
                    ),
                    0.0,
                    color,
                );
            }
            painter.text(
                egui::pos2(bar.left(), bar.bottom() + 8.0),
                egui::Align2::LEFT_TOP,
                text.overlay.low,
                egui::FontId::proportional(10.0),
                palette.muted,
            );
            painter.text(
                egui::pos2(bar.right(), bar.bottom() + 8.0),
                egui::Align2::RIGHT_TOP,
                text.overlay.high,
                egui::FontId::proportional(10.0),
                palette.muted,
            );
            if metric.is_on() {
                painter.text(
                    egui::pos2(bar.left(), bar.bottom() + 24.0),
                    egui::Align2::LEFT_TOP,
                    scope_label(text, scope),
                    egui::FontId::proportional(9.5),
                    faded(palette.muted, 0.8),
                );
            }
        });
    });
}

fn scope_label(text: &Catalog, scope: HeatScope) -> &'static str {
    match scope {
        HeatScope::Global => text.overlay.map_scope_global,
        HeatScope::Sample => text.overlay.map_scope_sample,
        HeatScope::Top => text.overlay.map_scope_top,
        HeatScope::Snapshot => text.overlay.map_scope_snapshot,
    }
}

fn metric_label(text: &Catalog, metric: CountryMetric) -> &'static str {
    match metric {
        CountryMetric::Off => text.overlay.map_off,
        CountryMetric::Density => text.overlay.map_density,
        CountryMetric::Genre => text.overlay.map_genre,
        CountryMetric::Quality => text.overlay.map_quality,
        CountryMetric::Kind => text.overlay.map_kind,
        CountryMetric::Rating => text.overlay.map_rating,
    }
}

/// Рампа тепловой карты — те же шесть опорных точек, что и у заливки стран
/// (ТЗ §3.2), иначе легенда обещает цвета, которых на глобусе нет.
const DENSITY_SAMPLES: usize = 24;

fn heat_stops() -> &'static [egui::Color32; DENSITY_SAMPLES] {
    static STOPS: OnceLock<[egui::Color32; DENSITY_SAMPLES]> = OnceLock::new();
    STOPS.get_or_init(|| {
        std::array::from_fn(|sample| heat_color(sample as f32 / (DENSITY_SAMPLES - 1) as f32))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn density_gradient_matches_the_heatmap_ramp() {
        let stops = heat_stops();
        assert_eq!(stops.len(), DENSITY_SAMPLES);
        assert_eq!(*stops.first().expect("не пустой"), heat_color(0.0));
        assert_eq!(*stops.last().expect("не пустой"), heat_color(1.0));
        // Рампа не должна иметь плоских участков между опорными точками.
        for pair in stops.windows(2) {
            assert_ne!(pair[0], pair[1], "градиент плотности не должен иметь плато");
        }
        assert_ne!(stops[0], stops[DENSITY_SAMPLES - 1]);
    }

    #[test]
    fn every_metric_has_its_own_label() {
        for language in Language::ALL {
            let text = i18n::catalog(language);
            let mut labels: Vec<&str> = CountryMetric::ALL
                .iter()
                .map(|metric| metric_label(text, *metric))
                .collect();
            labels.sort_unstable();
            labels.dedup();
            assert_eq!(
                labels.len(),
                CountryMetric::ALL.len(),
                "{language:?}: подписи режимов должны различаться"
            );
        }
    }
}
