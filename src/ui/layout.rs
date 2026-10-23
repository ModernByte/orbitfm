//! Плавающая геометрия раскладки «Orbital Desk»: глобус на всё окно,
//! панели — стеклянные зоны поверх него.

use eframe::egui;

pub(crate) const MARGIN: f32 = 14.0;
pub(crate) const GAP: f32 = 12.0;
pub(crate) const HUD_HEIGHT: f32 = 58.0;
/// Транспортная панель: контролы + громкость + 40 px визуализатора (ТЗ §3.3).
pub(crate) const TRANSPORT_HEIGHT: f32 = 84.0;
pub(crate) const PANEL_WIDTH: f32 = 312.0;
pub(crate) const HELP_SIZE: (f32, f32) = (216.0, 108.0);
pub(crate) const LEGEND_SIZE: (f32, f32) = (190.0, 58.0);

/// Прямоугольники зон Orbital Desk для текущего кадра.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DeskLayout {
    /// Full-bleed глобус — всё доступное окно.
    pub globe: egui::Rect,
    pub hud: egui::Rect,
    pub panel: Option<egui::Rect>,
    pub transport: egui::Rect,
    /// Карточка управления мышью: видима только по требованию (первый запуск или `?`).
    pub help: Option<egui::Rect>,
    pub legend: egui::Rect,
}

impl DeskLayout {
    pub(crate) fn compute(viewport: egui::Rect, panel_open: bool, help_open: bool) -> Self {
        let hud_height = HUD_HEIGHT.min(viewport.height() * 0.22).max(48.0);
        let transport_height = TRANSPORT_HEIGHT.min(viewport.height() * 0.2).max(48.0);

        let hud = egui::Rect::from_min_size(
            viewport.min + egui::vec2(MARGIN, MARGIN),
            egui::vec2(viewport.width() - 2.0 * MARGIN, hud_height),
        );
        let transport = egui::Rect::from_min_size(
            egui::pos2(
                viewport.left() + MARGIN,
                viewport.bottom() - MARGIN - transport_height,
            ),
            egui::vec2(viewport.width() - 2.0 * MARGIN, transport_height),
        );

        let content_top = hud.bottom() + GAP;
        let content_bottom = transport.top() - GAP;

        let panel = panel_open.then(|| {
            let width = PANEL_WIDTH.min(viewport.width() - 2.0 * MARGIN);
            egui::Rect::from_min_size(
                egui::pos2(viewport.right() - MARGIN - width, content_top),
                egui::vec2(width, (content_bottom - content_top).max(120.0)),
            )
        });

        let help = help_open.then(|| {
            egui::Rect::from_min_size(
                egui::pos2(viewport.left() + MARGIN, content_top),
                egui::vec2(
                    HELP_SIZE.0.min(viewport.width() - 2.0 * MARGIN),
                    HELP_SIZE.1,
                ),
            )
        });
        let legend = egui::Rect::from_min_size(
            egui::pos2(
                viewport.left() + MARGIN,
                transport.top() - GAP - LEGEND_SIZE.1,
            ),
            egui::vec2(
                LEGEND_SIZE.0.min(viewport.width() - 2.0 * MARGIN),
                LEGEND_SIZE.1,
            ),
        );

        Self {
            globe: viewport,
            hud,
            panel,
            transport,
            help,
            legend,
        }
    }

    /// Точка попадает в плавающую зону — глобус игнорирует ввод под ней.
    pub(crate) fn blocks_point(&self, point: egui::Pos2) -> bool {
        self.hud.contains(point)
            || self.transport.contains(point)
            || self.legend.contains(point)
            || self.panel.is_some_and(|panel| panel.contains(point))
            || self.help.is_some_and(|help| help.contains(point))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))
    }

    #[test]
    fn globe_spans_the_whole_viewport() {
        let layout = DeskLayout::compute(viewport(), true, true);
        assert_eq!(layout.globe, viewport());
    }

    #[test]
    fn panels_float_inside_with_margins() {
        let layout = DeskLayout::compute(viewport(), true, true);
        assert!(layout.globe.contains(layout.hud.min));
        assert!(layout.hud.top() > layout.globe.top());
        assert!(layout.transport.bottom() < layout.globe.bottom());
        assert!(layout.hud.bottom() < layout.transport.top());
        let panel = layout.panel.expect("панель открыта");
        assert!(panel.right() <= layout.globe.right());
        assert!(panel.top() >= layout.hud.bottom());
        assert!(panel.bottom() <= layout.transport.top());
    }

    #[test]
    fn closed_panel_does_not_block_its_area() {
        let layout = DeskLayout::compute(viewport(), false, true);
        assert!(layout.panel.is_none());
        let panel_point = egui::pos2(1280.0 - 200.0, 400.0);
        assert!(!layout.blocks_point(panel_point));
        assert!(layout.blocks_point(egui::pos2(640.0, 20.0)));
    }

    #[test]
    fn hidden_help_does_not_block_its_area() {
        let layout = DeskLayout::compute(viewport(), true, false);
        assert!(layout.help.is_none());
        let help_point = egui::pos2(MARGIN + 40.0, HUD_HEIGHT + MARGIN + GAP + 10.0);
        assert!(!layout.blocks_point(help_point));
    }

    #[test]
    fn visible_help_blocks_its_area() {
        let layout = DeskLayout::compute(viewport(), true, true);
        let help = layout.help.expect("help открыт");
        assert!(layout.blocks_point(help.center()));
    }

    #[test]
    fn narrow_window_clamps_floating_zones() {
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(760.0, 500.0));
        let layout = DeskLayout::compute(viewport, true, true);
        let panel = layout.panel.expect("панель открыта");
        assert!(panel.left() >= viewport.left() + MARGIN - 0.5);
        assert!(panel.right() <= viewport.right() - MARGIN + 0.5);
        assert!(layout.help.expect("help открыт").width() <= viewport.width() - 2.0 * MARGIN);
        assert!(layout.legend.width() <= viewport.width() - 2.0 * MARGIN);
    }
}
