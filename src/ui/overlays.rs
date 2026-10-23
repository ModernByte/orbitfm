//! Постоянные плавающие подсказки Orbital Desk: карточка управления
//! мышью и легенда плотности станций.

use eframe::egui;

use std::sync::OnceLock;

use super::icons::{MousePose, draw_mouse};
use super::palette::{Palette, heat_color};

/// Карточка управления: правый клик — орбита, ЛКМ — страна/маркер,
/// колесо — зум. Рисуются иконки мыши в трёх позах.
pub(crate) fn help_card(ui: &mut egui::Ui, palette: &Palette) {
    let rows = [
        (MousePose::Orbit, "Right-drag", "orbit"),
        (MousePose::Click, "Left click", "country / station"),
        (MousePose::Wheel, "Scroll", "zoom"),
    ];

    palette.panel(14).show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        for (pose, button, action) in rows {
            let (rect, _) = ui
                .allocate_exact_size(egui::vec2(ui.available_width(), 30.0), egui::Sense::hover());
            if !ui.is_rect_visible(rect) {
                continue;
            }
            let painter = ui.painter();
            draw_mouse(
                painter,
                egui::pos2(rect.left() + 12.0, rect.center().y),
                10.0,
                pose,
                palette.accent,
            );
            let text_x = rect.left() + 30.0;
            painter.text(
                egui::pos2(text_x, rect.center().y - 7.0),
                egui::Align2::LEFT_CENTER,
                button,
                egui::FontId::proportional(11.5),
                palette.text,
            );
            painter.text(
                egui::pos2(text_x, rect.center().y + 8.0),
                egui::Align2::LEFT_CENTER,
                action,
                egui::FontId::proportional(10.5),
                palette.muted,
            );
        }
    });
}

/// Легенда «Station density»: радужный градиент Low → High.
pub(crate) fn density_legend(ui: &mut egui::Ui, palette: &Palette) {
    palette.panel(14).show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 34.0), egui::Sense::hover());
        if !ui.is_rect_visible(rect) {
            return;
        }
        let painter = ui.painter();
        painter.text(
            egui::pos2(rect.left(), rect.top()),
            egui::Align2::LEFT_TOP,
            "Station density",
            egui::FontId::proportional(11.0),
            palette.muted,
        );

        let bar = egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.top() + 15.0),
            egui::pos2(rect.right(), rect.top() + 24.0),
        );
        let stops = density_stops();
        let slices = stops.len() - 1;
        for (slice, color) in stops.iter().enumerate().take(slices) {
            let x0 = bar.left() + bar.width() * slice as f32 / slices as f32;
            let x1 = bar.left() + bar.width() * (slice + 1) as f32 / slices as f32;
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(x0, bar.top()),
                    egui::pos2(x1 + 0.5, bar.bottom()),
                ),
                0.0,
                *color,
            );
        }
        painter.text(
            egui::pos2(bar.left(), bar.bottom() + 8.0),
            egui::Align2::LEFT_TOP,
            "Low",
            egui::FontId::proportional(10.0),
            palette.muted,
        );
        painter.text(
            egui::pos2(bar.right(), bar.bottom() + 8.0),
            egui::Align2::RIGHT_TOP,
            "High",
            egui::FontId::proportional(10.0),
            palette.muted,
        );
    });
}

/// Рампа плотности легенды — те же шесть опорных точек, что и у заливки стран
/// (ТЗ §3.2), иначе легенда обещает цвета, которых на глобусе нет.
const DENSITY_SAMPLES: usize = 24;

fn density_stops() -> &'static [egui::Color32; DENSITY_SAMPLES] {
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
        let stops = density_stops();
        assert_eq!(stops.len(), DENSITY_SAMPLES);
        assert_eq!(*stops.first().expect("не пустой"), heat_color(0.0));
        assert_eq!(*stops.last().expect("не пустой"), heat_color(1.0));
        // Рампа не должна иметь плоских участков между опорными точками.
        for pair in stops.windows(2) {
            assert_ne!(pair[0], pair[1], "градиент плотности не должен иметь плато");
        }
        assert_ne!(stops[0], stops[DENSITY_SAMPLES - 1]);
    }
}
