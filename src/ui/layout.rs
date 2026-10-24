//! Плавающая геометрия раскладки «Orbital Desk»: глобус на всё окно,
//! панели — стеклянные зоны поверх него. Плотность выводится из режима окна
//! (ADR-0021): Focus — полный стол, Overlay — капсула, Listening — одна строка.

use eframe::egui;

use crate::session::WindowMode;

pub(crate) const MARGIN: f32 = 14.0;
pub(crate) const GAP: f32 = 12.0;
pub(crate) const HUD_HEIGHT: f32 = 58.0;
/// Транспортная панель: контролы + громкость + 40 px визуализатора (ТЗ §3.3).
pub(crate) const TRANSPORT_HEIGHT: f32 = 84.0;
pub(crate) const PANEL_WIDTH: f32 = 312.0;
pub(crate) const HELP_SIZE: (f32, f32) = (216.0, 108.0);
pub(crate) const LEGEND_SIZE: (f32, f32) = (190.0, 58.0);
/// Нижняя зона капсулы Overlay с избранными станциями.
const CAPSULE_HEIGHT: (f32, f32) = (0.40, 300.0);

/// Плотность набора виджетов: один и тот же UI-код, три уровня (ADR-0021 п. 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayoutDensity {
    Full,
    Compact,
    Minimal,
}

impl LayoutDensity {
    pub(crate) fn for_mode(mode: WindowMode) -> Self {
        match mode {
            WindowMode::Focus => Self::Full,
            WindowMode::Overlay => Self::Compact,
            WindowMode::Listening => Self::Minimal,
        }
    }
}

/// Прямоугольники зон Orbital Desk для текущего кадра.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DeskLayout {
    pub density: LayoutDensity,
    /// Глобус available: full-bleed в Focus, мини-глобус в Overlay, нет в Listening.
    pub globe: Option<egui::Rect>,
    pub hud: Option<egui::Rect>,
    pub panel: Option<egui::Rect>,
    pub transport: Option<egui::Rect>,
    /// Карточка управления мышью: видима только по требованию (первый запуск или `?`).
    pub help: Option<egui::Rect>,
    pub legend: Option<egui::Rect>,
    /// Нижняя зона капсулы Overlay: избранные станции.
    pub capsule: Option<egui::Rect>,
}

impl DeskLayout {
    pub(crate) fn compute(
        viewport: egui::Rect,
        panel_open: bool,
        help_open: bool,
        mode: WindowMode,
    ) -> Self {
        match LayoutDensity::for_mode(mode) {
            LayoutDensity::Full => Self::full(viewport, panel_open, help_open),
            LayoutDensity::Compact => Self::compact(viewport),
            LayoutDensity::Minimal => Self::minimal(viewport),
        }
    }

    fn full(viewport: egui::Rect, panel_open: bool, help_open: bool) -> Self {
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
            density: LayoutDensity::Full,
            globe: Some(viewport),
            hud: Some(hud),
            panel,
            transport: Some(transport),
            help,
            legend: Some(legend),
            capsule: None,
        }
    }

    /// Капсула Overlay: станция сверху, мини-глобус в центре, избранные снизу.
    fn compact(viewport: egui::Rect) -> Self {
        let hud_height = HUD_HEIGHT.min(viewport.height() * 0.22).max(48.0);
        let hud = inset(viewport, viewport.top() + MARGIN, hud_height);
        let capsule_height = (viewport.height() * CAPSULE_HEIGHT.0)
            .min(CAPSULE_HEIGHT.1)
            .max(96.0);
        let capsule = inset(
            viewport,
            viewport.bottom() - MARGIN - capsule_height,
            capsule_height,
        );
        let top = hud.bottom() + GAP;
        let bottom = capsule.top() - GAP;
        let globe = (bottom - top >= 80.0).then(|| {
            egui::Rect::from_min_max(
                egui::pos2(viewport.left() + MARGIN, top),
                egui::pos2(viewport.right() - MARGIN, bottom),
            )
        });

        Self {
            density: LayoutDensity::Compact,
            globe,
            hud: Some(hud),
            panel: None,
            transport: None,
            help: None,
            legend: None,
            capsule: Some(capsule),
        }
    }

    /// Listening: единственная строка now-playing с пульсом.
    fn minimal(viewport: egui::Rect) -> Self {
        Self {
            density: LayoutDensity::Minimal,
            globe: None,
            hud: Some(inset(
                viewport,
                viewport.top() + MARGIN,
                viewport.height() - 2.0 * MARGIN,
            )),
            panel: None,
            transport: None,
            help: None,
            legend: None,
            capsule: None,
        }
    }

    /// Точка попадает в плавающую зону — глобус игнорирует ввод под ней.
    pub(crate) fn blocks_point(&self, point: egui::Pos2) -> bool {
        [
            self.hud,
            self.transport,
            self.legend,
            self.panel,
            self.help,
            self.capsule,
        ]
        .into_iter()
        .flatten()
        .any(|zone| zone.contains(point))
    }
}

fn inset(viewport: egui::Rect, top: f32, height: f32) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(viewport.left() + MARGIN, top),
        egui::vec2(viewport.width() - 2.0 * MARGIN, height),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))
    }

    fn focus(panel_open: bool, help_open: bool) -> DeskLayout {
        DeskLayout::compute(viewport(), panel_open, help_open, WindowMode::Focus)
    }

    #[test]
    fn globe_spans_the_whole_viewport() {
        let layout = focus(true, true);
        assert_eq!(layout.globe, Some(viewport()));
        assert_eq!(layout.density, LayoutDensity::Full);
    }

    #[test]
    fn panels_float_inside_with_margins() {
        let layout = focus(true, true);
        let hud = layout.hud.expect("hud есть");
        let transport = layout.transport.expect("transport есть");
        assert!(layout.globe.expect("глобус есть").contains(hud.min));
        assert!(hud.top() > 0.0);
        assert!(transport.bottom() < viewport().bottom());
        assert!(hud.bottom() < transport.top());
        let panel = layout.panel.expect("панель открыта");
        assert!(panel.right() <= viewport().right());
        assert!(panel.top() >= hud.bottom());
        assert!(panel.bottom() <= transport.top());
    }

    #[test]
    fn closed_panel_does_not_block_its_area() {
        let layout = focus(false, true);
        assert!(layout.panel.is_none());
        let panel_point = egui::pos2(1280.0 - 200.0, 400.0);
        assert!(!layout.blocks_point(panel_point));
        assert!(layout.blocks_point(egui::pos2(640.0, 20.0)));
    }

    #[test]
    fn hidden_help_does_not_block_its_area() {
        let layout = focus(true, false);
        assert!(layout.help.is_none());
        let help_point = egui::pos2(MARGIN + 40.0, HUD_HEIGHT + MARGIN + GAP + 10.0);
        assert!(!layout.blocks_point(help_point));
    }

    #[test]
    fn visible_help_blocks_its_area() {
        let layout = focus(true, true);
        let help = layout.help.expect("help открыт");
        assert!(layout.blocks_point(help.center()));
    }

    #[test]
    fn narrow_window_clamps_floating_zones() {
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(760.0, 500.0));
        let layout = DeskLayout::compute(viewport, true, true, WindowMode::Focus);
        let panel = layout.panel.expect("панель открыта");
        assert!(panel.left() >= viewport.left() + MARGIN - 0.5);
        assert!(panel.right() <= viewport.right() - MARGIN + 0.5);
        assert!(layout.help.expect("help открыт").width() <= viewport.width() - 2.0 * MARGIN);
        assert!(layout.legend.expect("легенда есть").width() <= viewport.width() - 2.0 * MARGIN);
    }

    #[test]
    fn overlay_is_a_three_zone_capsule() {
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(420.0, 620.0));
        let layout = DeskLayout::compute(viewport, true, true, WindowMode::Overlay);
        assert_eq!(layout.density, LayoutDensity::Compact);
        assert!(layout.panel.is_none());
        assert!(layout.transport.is_none());
        assert!(layout.help.is_none());
        assert!(layout.legend.is_none());
        let hud = layout.hud.expect("верх капсулы");
        let globe = layout.globe.expect("мини-глобус");
        let capsule = layout.capsule.expect("избранные");
        assert!(hud.top() >= viewport.top() + MARGIN);
        assert!(globe.top() > hud.bottom());
        assert!(globe.bottom() < capsule.top());
        assert!(capsule.bottom() <= viewport.bottom() - MARGIN + 0.5);
        assert!(layout.blocks_point(hud.center()));
        assert!(layout.blocks_point(capsule.center()));
        assert!(!layout.blocks_point(globe.center()));
    }

    #[test]
    fn listening_is_a_single_row() {
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(420.0, 152.0));
        let layout = DeskLayout::compute(viewport, true, true, WindowMode::Listening);
        assert_eq!(layout.density, LayoutDensity::Minimal);
        assert!(layout.globe.is_none());
        assert!(layout.panel.is_none());
        assert!(layout.transport.is_none());
        assert!(layout.capsule.is_none());
        let row = layout.hud.expect("строка listening");
        assert_eq!(row.left(), viewport.left() + MARGIN);
        assert_eq!(row.width(), viewport.width() - 2.0 * MARGIN);
        assert_eq!(row.height(), viewport.height() - 2.0 * MARGIN);
        assert!(layout.blocks_point(row.center()));
    }

    #[test]
    fn density_follows_the_window_mode() {
        assert_eq!(
            LayoutDensity::for_mode(WindowMode::Focus),
            LayoutDensity::Full
        );
        assert_eq!(
            LayoutDensity::for_mode(WindowMode::Overlay),
            LayoutDensity::Compact
        );
        assert_eq!(
            LayoutDensity::for_mode(WindowMode::Listening),
            LayoutDensity::Minimal
        );
    }
}
