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
/// Панель списка: 360 px — имя станции не упирается в звезду, а правый блок
/// «битрейт/кодек + звезда» получает собственный запас (ADR-0033, ревизия
/// 2026-10-28).
pub(crate) const PANEL_WIDTH: f32 = 360.0;
/// Модальное окно настроек (ADR-0031, ревизия 2026-10-28): 760×544 на столе
/// 1100×800, clamp по viewport. Слева — колонка категорий, справа — карточки
/// настроек; в узких окнах колонка сжимается до иконочного рейла.
pub(crate) const SETTINGS_MODAL: (f32, f32) = (760.0, 544.0);
/// Карточка управления: три строки по 30 px плюс поля 14 сверху и снизу.
pub(crate) const HELP_SIZE: (f32, f32) = (216.0, 128.0);
/// Легенда карты стран: выбор метрики + градиент Low → High.
pub(crate) const LEGEND_SIZE: (f32, f32) = (216.0, 100.0);
/// Нижняя зона капсулы Overlay с избранными станциями.
const CAPSULE_HEIGHT: (f32, f32) = (0.40, 300.0);
/// Поля выпадающей панели: стекло не касается кромки окна, чтобы скругления
/// читались как карточка поверх стола (ADR-0033).
const PANEL_MARGIN: f32 = 8.0;
/// Шапка панели: строка бренда, поиск и режим поиска (8 px запаса).
const PANEL_HEADER_HEIGHT: f32 = 134.0;
/// Подвал панели: what's playing и контролы.
const PANEL_FOOTER_HEIGHT: f32 = 64.0;

/// Плотность набора виджетов: один и тот же UI-код, четыре уровня (ADR-0021,
/// ADR-0033).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayoutDensity {
    Full,
    Compact,
    Minimal,
    /// Выпадающая панель бар-виджета: одна стеклянная поверхность без глобуса.
    Panel,
}

impl LayoutDensity {
    pub(crate) fn for_mode(mode: WindowMode) -> Self {
        match mode {
            WindowMode::Focus => Self::Full,
            WindowMode::Overlay => Self::Compact,
            WindowMode::Listening => Self::Minimal,
            WindowMode::Panel => Self::Panel,
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
    /// Единая стеклянная поверхность выпадающей панели (ADR-0033); в остальных
    /// плотностях зоны рисуются отдельными карточками.
    pub surface: Option<egui::Rect>,
    /// Карточка управления мышью: видима только по требованию (первый запуск или
    /// переключатель в настройках).
    pub help: Option<egui::Rect>,
    pub legend: Option<egui::Rect>,
    /// Нижняя зона капсулы Overlay: избранные станции.
    pub capsule: Option<egui::Rect>,
    /// Модальное окно настроек по центру: видно только когда открыто (ADR-0031).
    pub settings: Option<egui::Rect>,
}

impl DeskLayout {
    pub(crate) fn compute(
        viewport: egui::Rect,
        panel_open: bool,
        help_open: bool,
        settings_open: bool,
        mode: WindowMode,
    ) -> Self {
        match LayoutDensity::for_mode(mode) {
            LayoutDensity::Full => Self::full(viewport, panel_open, help_open, settings_open),
            LayoutDensity::Compact => Self::compact(viewport, settings_open),
            LayoutDensity::Minimal => Self::minimal(viewport),
            LayoutDensity::Panel => Self::panel(viewport, settings_open),
        }
    }

    fn full(viewport: egui::Rect, panel_open: bool, help_open: bool, settings_open: bool) -> Self {
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
            surface: None,
            help,
            legend: Some(legend),
            capsule: None,
            settings: settings_rect(viewport, settings_open),
        }
    }

    /// Капсула Overlay: станция сверху, мини-глобус в центре, избранные снизу.
    fn compact(viewport: egui::Rect, settings_open: bool) -> Self {
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
            surface: None,
            help: None,
            legend: None,
            capsule: Some(capsule),
            settings: settings_rect(viewport, settings_open),
        }
    }

    /// Listening: единственная строка now-playing с пульсом. Окно настроек здесь
    /// не помещается (ADR-0031): настройки открываются через Overlay.
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
            surface: None,
            help: None,
            legend: None,
            capsule: None,
            settings: None,
        }
    }

    /// Выпадающая панель: одна поверхность, внутри — шапка, список и подвал.
    /// Модальное окно настроек рисуется поверх неё отдельным слоем: сцена
    /// остаётся на месте под scrim'ом (ADR-0031, ревизия 2026-10-28).
    fn panel(viewport: egui::Rect, settings_open: bool) -> Self {
        let surface = viewport.shrink(PANEL_MARGIN);
        let width = surface.width();
        let header_height = PANEL_HEADER_HEIGHT.min(surface.height() * 0.4);
        let footer_height = PANEL_FOOTER_HEIGHT.min(surface.height() * 0.25);
        let hud = egui::Rect::from_min_size(surface.min, egui::vec2(width, header_height));
        let transport = egui::Rect::from_min_size(
            egui::pos2(surface.left(), surface.bottom() - footer_height),
            egui::vec2(width, footer_height),
        );
        let top = hud.bottom() + 2.0;
        let bottom = (transport.top() - 2.0)
            .max(top + 80.0)
            .min(surface.bottom());
        let panel = egui::Rect::from_min_max(
            egui::pos2(surface.left(), top),
            egui::pos2(surface.right(), bottom),
        );

        Self {
            density: LayoutDensity::Panel,
            globe: None,
            hud: Some(hud),
            panel: Some(panel),
            transport: Some(transport),
            surface: Some(surface),
            help: None,
            legend: None,
            capsule: None,
            settings: settings_rect(viewport, settings_open),
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
            self.surface,
            self.settings,
        ]
        .into_iter()
        .flatten()
        .any(|zone| zone.contains(point))
    }

    /// Отступ тоста от нижнего края окна: стек встаёт над занятой нижней зоной
    /// текущей плотности, поэтому в Focus он не режет transport, а в Overlay —
    /// избранное (ADR-0032 §4). В Listening нижней зоны нет — только поле.
    pub(crate) fn toast_offset(&self, viewport: egui::Rect) -> f32 {
        let bottom_zone = match self.density {
            LayoutDensity::Full | LayoutDensity::Panel => self.transport,
            LayoutDensity::Compact => self.capsule,
            LayoutDensity::Minimal => None,
        };
        bottom_zone.map_or(MARGIN, |zone| viewport.bottom() - zone.top() + GAP)
    }
}

/// Модальное окно настроек: центр вьюпорта, размеры ужимаются до полей.
fn settings_rect(viewport: egui::Rect, settings_open: bool) -> Option<egui::Rect> {
    settings_open.then(|| {
        let size = egui::vec2(
            SETTINGS_MODAL
                .0
                .min((viewport.width() - 2.0 * MARGIN).max(1.0)),
            SETTINGS_MODAL
                .1
                .min((viewport.height() - 2.0 * MARGIN).max(1.0)),
        );
        egui::Rect::from_center_size(viewport.center(), size)
    })
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
        DeskLayout::compute(viewport(), panel_open, help_open, false, WindowMode::Focus)
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
        let layout = DeskLayout::compute(viewport, true, true, false, WindowMode::Focus);
        let panel = layout.panel.expect("панель открыта");
        assert!(panel.left() >= viewport.left() + MARGIN - 0.5);
        assert!(panel.right() <= viewport.right() - MARGIN + 0.5);
        assert!(layout.help.expect("help открыт").width() <= viewport.width() - 2.0 * MARGIN);
        assert!(layout.legend.expect("легенда есть").width() <= viewport.width() - 2.0 * MARGIN);
    }

    #[test]
    fn overlay_is_a_three_zone_capsule() {
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(500.0, 620.0));
        let layout = DeskLayout::compute(viewport, true, true, false, WindowMode::Overlay);
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
        let layout = DeskLayout::compute(viewport, true, true, false, WindowMode::Listening);
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
    fn open_settings_modal_is_centered_and_blocks_the_globe() {
        let layout = DeskLayout::compute(viewport(), true, false, true, WindowMode::Focus);
        let modal = layout.settings.expect("модалка открыта");
        assert_eq!(modal.width(), SETTINGS_MODAL.0);
        assert_eq!(modal.height(), SETTINGS_MODAL.1);
        assert_eq!(modal.center(), viewport().center());
        assert!(modal.right() <= viewport().right() - MARGIN + 0.5);
        assert!(modal.top() >= viewport().top() + MARGIN - 0.5);
        assert!(modal.bottom() <= viewport().bottom() - MARGIN + 0.5);
        assert!(layout.blocks_point(modal.center()));
    }

    #[test]
    fn closed_settings_modal_does_not_block_anything() {
        let layout = DeskLayout::compute(viewport(), false, false, false, WindowMode::Focus);
        assert!(layout.settings.is_none());
        assert!(!layout.blocks_point(viewport().center()));
    }

    /// Якорь стека тостов зависит от плотности: над занятой нижней зоной, а не
    /// на постоянном отступе (ADR-0032 §4).
    #[test]
    fn toast_anchor_clears_the_bottom_zone_of_its_density() {
        let rect = viewport();
        let layout = focus(false, false);
        let transport = layout.transport.expect("transport есть");
        assert_eq!(
            layout.toast_offset(rect),
            rect.bottom() - transport.top() + GAP
        );

        let overlay = DeskLayout::compute(rect, false, false, false, WindowMode::Overlay);
        let capsule = overlay.capsule.expect("капсула есть");
        assert_eq!(
            overlay.toast_offset(rect),
            rect.bottom() - capsule.top() + GAP
        );

        // Listening: нижней зоны нет, остаётся только поле окна.
        let listening = DeskLayout::compute(rect, false, false, false, WindowMode::Listening);
        assert_eq!(listening.toast_offset(rect), MARGIN);
    }

    #[test]
    fn settings_modal_clamps_to_a_narrow_viewport() {
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(360.0, 480.0));
        let layout = DeskLayout::compute(viewport, false, false, true, WindowMode::Overlay);
        let modal = layout.settings.expect("модалка в Overlay");
        assert_eq!(modal.width(), viewport.width() - 2.0 * MARGIN);
        assert_eq!(modal.height(), viewport.height() - 2.0 * MARGIN);
        assert_eq!(modal.center(), viewport.center());
        assert!(layout.blocks_point(modal.center()));
    }

    #[test]
    fn listening_has_no_settings_zone() {
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(420.0, 152.0));
        let layout = DeskLayout::compute(viewport, false, false, true, WindowMode::Listening);
        assert!(
            layout.settings.is_none(),
            "в Listening настройки не рисуются"
        );
    }

    #[test]
    fn panel_keeps_its_scene_under_the_settings_modal() {
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 640.0));
        let layout = DeskLayout::compute(viewport, true, true, true, WindowMode::Panel);
        assert!(layout.surface.is_some(), "панель остаётся под scrim'ом");
        assert!(layout.hud.is_some());
        assert!(layout.panel.is_some());
        assert!(layout.transport.is_some());
        let modal = layout.settings.expect("модалка поверх панели");
        assert_eq!(modal.width(), viewport.width() - 2.0 * MARGIN);
        assert_eq!(modal.height(), SETTINGS_MODAL.1);
        assert_eq!(modal.center(), viewport.center());
        assert!(layout.blocks_point(modal.center()));
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
        assert_eq!(
            LayoutDensity::for_mode(WindowMode::Panel),
            LayoutDensity::Panel
        );
    }

    #[test]
    fn bar_panel_stacks_header_list_and_footer_on_one_surface() {
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 640.0));
        let layout = DeskLayout::compute(viewport, true, true, false, WindowMode::Panel);
        assert_eq!(layout.density, LayoutDensity::Panel);
        assert!(layout.globe.is_none());
        assert!(layout.help.is_none());
        assert!(layout.legend.is_none());
        assert!(layout.capsule.is_none());

        let surface = layout.surface.expect("поверхность панели");
        assert_eq!(surface.min, egui::pos2(PANEL_MARGIN, PANEL_MARGIN));
        assert!(surface.right() <= viewport.right());
        assert!(surface.bottom() <= viewport.bottom());

        let header = layout.hud.expect("шапка");
        let list = layout.panel.expect("список");
        let footer = layout.transport.expect("подвал");
        assert_eq!(header.width(), surface.width());
        assert_eq!(footer.width(), surface.width());
        assert!(header.top() >= surface.top());
        assert!(list.top() >= header.bottom());
        assert!(footer.top() >= list.bottom());
        assert!(footer.bottom() <= surface.bottom());
        assert!(list.height() > 200.0);
        assert!(layout.blocks_point(surface.center()));
    }

    #[test]
    fn bar_panel_toast_anchor_clears_the_footer() {
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 640.0));
        let layout = DeskLayout::compute(viewport, true, false, false, WindowMode::Panel);
        let footer = layout.transport.expect("подвал");
        assert_eq!(
            layout.toast_offset(viewport),
            viewport.bottom() - footer.top() + GAP
        );
    }
}
