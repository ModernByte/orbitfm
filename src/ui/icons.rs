//! SVG-иконки Lucide (ADR-0034): единый штрих, перекраска через tint и
//! анимированная hover-подсветка. Иллюстрация мыши для help остаётся ручной:
//! ей нужны позы курсора, которых нет в готовом наборе.

use eframe::egui::{self, Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Vec2, pos2};

use crate::i18n;
use crate::settings::Language;

use super::palette::{Palette, mix_colors};
use super::widgets::{control_motion, faded, focus_ring};

/// Оптический размер глифа внутри кнопки.
const GLYPH: f32 = 19.0;
/// Квадратная иконочная кнопка шапок и панелей. `pub(crate)` — по этой же
/// константе капсула Overlay резервирует ширину правого блока контролов.
pub(crate) const SIDE: f32 = 30.0;
/// Кнопки капсул и строк: компактнее, но заметно крупнее прежних 22–24 px.
/// Используется и капсулой для того же резерва.
pub(crate) const SMALL_SIDE: f32 = 28.0;
/// Транспортные кнопки на треть шире квадратных: 32×28 (stop/prev/next).
/// Ширина нужна капсуле Overlay, чтобы резерв совпал с аллокацией.
pub(crate) const WIDE_SIDE: f32 = 32.0;

/// Набор встроенных иконок. Файлы — `assets/icons/*.svg`, белый штрих
/// перекрашивается `Image::tint` под палитру (ADR-0034).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Icon {
    Search,
    Settings,
    Moon,
    Panel,
    Dice,
    Close,
    Star,
    StarFilled,
    Heart,
    HeartFilled,
    SkipBack,
    SkipForward,
    Stop,
    Volume,
    Expand,
    Help,
    Sort,
    Rows,
    RowsCompact,
    Globe,
    Clock,
    Funnel,
    FunnelX,
    Alert,
}

impl Icon {
    fn source(self) -> egui::ImageSource<'static> {
        match self {
            Self::Search => egui::include_image!("../../assets/icons/search.svg"),
            Self::Settings => egui::include_image!("../../assets/icons/settings.svg"),
            Self::Moon => egui::include_image!("../../assets/icons/moon.svg"),
            Self::Panel => egui::include_image!("../../assets/icons/panel-left.svg"),
            Self::Dice => egui::include_image!("../../assets/icons/dice-5.svg"),
            Self::Close => egui::include_image!("../../assets/icons/x.svg"),
            Self::Star => egui::include_image!("../../assets/icons/star.svg"),
            Self::StarFilled => egui::include_image!("../../assets/icons/star-filled.svg"),
            Self::Heart => egui::include_image!("../../assets/icons/heart.svg"),
            Self::HeartFilled => egui::include_image!("../../assets/icons/heart-filled.svg"),
            Self::SkipBack => egui::include_image!("../../assets/icons/skip-back.svg"),
            Self::SkipForward => egui::include_image!("../../assets/icons/skip-forward.svg"),
            Self::Stop => egui::include_image!("../../assets/icons/square-filled.svg"),
            Self::Volume => egui::include_image!("../../assets/icons/volume-2.svg"),
            Self::Expand => egui::include_image!("../../assets/icons/maximize-2.svg"),
            Self::Help => egui::include_image!("../../assets/icons/circle-help.svg"),
            Self::Sort => egui::include_image!("../../assets/icons/arrow-up-down.svg"),
            Self::Rows => egui::include_image!("../../assets/icons/rows-3.svg"),
            Self::RowsCompact => egui::include_image!("../../assets/icons/rows-4.svg"),
            Self::Globe => egui::include_image!("../../assets/icons/globe.svg"),
            Self::Clock => egui::include_image!("../../assets/icons/clock.svg"),
            Self::Funnel => egui::include_image!("../../assets/icons/funnel.svg"),
            Self::FunnelX => egui::include_image!("../../assets/icons/funnel-x.svg"),
            Self::Alert => egui::include_image!("../../assets/icons/circle-alert.svg"),
        }
    }
}

/// Рисует иконку по центру прямоугольника. Размер фиксирован вызывающим:
/// загрузчик растеризует SVG ровно под физический размер и кэширует его
/// (ADR-0034 §5), поэтому масштабировать глиф анимацией нельзя.
pub(crate) fn paint_icon(ui: &egui::Ui, icon: Icon, center: Pos2, size: f32, color: Color32) {
    let rect = Rect::from_center_size(center, Vec2::splat(size));
    egui::Image::new(icon.source())
        .tint(color)
        .paint_at(ui, rect);
}

/// Цвет иконки: в покое базовый, на hover подмешивается акцент.
fn icon_tint(base: Color32, accent: Color32, hover: f32) -> Color32 {
    mix_colors(base, accent, 0.5 * hover)
}

/// Кнопка-иконка с fade-in подложкой и accent-ободком на hover.
///
/// Геометрия и hit-зона фиксированы: анимируются только альфы отрисовки
/// (ADR-0032 §7). Закрытие получает базовый цвет и прогресс hover, чтобы
/// заливные звёзды и сердца сохранили свой цвет.
fn icon_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    size: Vec2,
    enabled: bool,
    active: bool,
    hint: &str,
    paint: impl FnOnce(&egui::Ui, Rect, Color32, f32),
) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let hovered = enabled && response.hovered();
    let pressed = enabled && response.is_pointer_button_down_on();
    let (hover, press) = control_motion(ui, response.id, hovered, pressed);
    let radius = (size.x.min(size.y) * 0.34).clamp(7.0, 11.0);

    if ui.is_rect_visible(rect) {
        let persistent = if active { 0.14 } else { 0.0 };
        let fill = 0.09 * hover + 0.07 * press + persistent;
        if fill > 0.0 {
            ui.painter()
                .rect_filled(rect.shrink(1.0), radius, faded(palette.text, fill));
        }
        let rim = (if active { 0.30 } else { 0.0 } + 0.22 * hover).min(0.5);
        if rim > 0.0 {
            ui.painter().rect_stroke(
                rect.shrink(1.0),
                radius,
                Stroke::new(1.0, faded(palette.accent, rim)),
                egui::StrokeKind::Inside,
            );
        }
    }

    let base = if enabled { palette.text } else { palette.muted };
    paint(ui, rect, base, if enabled { hover } else { 0.0 });
    focus_ring(ui, &response, rect, radius);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, hint));
    if enabled {
        response.on_hover_text(hint)
    } else {
        response
    }
}

pub(crate) fn star_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    filled: bool,
    color: Color32,
    hint: &str,
) -> Response {
    icon_button(
        ui,
        palette,
        Vec2::splat(SMALL_SIDE),
        true,
        filled,
        hint,
        move |ui, rect, base, hover| {
            let tint = icon_tint(if filled { color } else { base }, palette.accent, hover);
            let icon = if filled { Icon::StarFilled } else { Icon::Star };
            paint_icon(ui, icon, rect.center(), 18.0, tint);
        },
    )
}

pub(crate) fn heart_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    filled: bool,
    color: Color32,
    hint: &str,
) -> Response {
    icon_button(
        ui,
        palette,
        Vec2::splat(SMALL_SIDE),
        true,
        filled,
        hint,
        move |ui, rect, base, hover| {
            let tint = icon_tint(if filled { color } else { base }, palette.accent, hover);
            let icon = if filled {
                Icon::HeartFilled
            } else {
                Icon::Heart
            };
            paint_icon(ui, icon, rect.center(), 18.0, tint);
        },
    )
}

pub(crate) fn stop_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    enabled: bool,
    language: Language,
) -> Response {
    icon_button(
        ui,
        palette,
        Vec2::new(WIDE_SIDE, SMALL_SIDE),
        enabled,
        false,
        i18n::catalog(language).common.stop,
        |ui, rect, base, hover| {
            let tint = icon_tint(base, palette.accent, hover);
            paint_icon(ui, Icon::Stop, rect.center(), 16.0, tint);
        },
    )
}

pub(crate) fn prev_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    enabled: bool,
    language: Language,
) -> Response {
    icon_button(
        ui,
        palette,
        Vec2::new(WIDE_SIDE, SMALL_SIDE),
        enabled,
        false,
        i18n::catalog(language).common.previous_station,
        |ui, rect, base, hover| {
            let tint = icon_tint(base, palette.accent, hover);
            paint_icon(ui, Icon::SkipBack, rect.center(), 20.0, tint);
        },
    )
}

pub(crate) fn next_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    enabled: bool,
    language: Language,
) -> Response {
    icon_button(
        ui,
        palette,
        Vec2::new(WIDE_SIDE, SMALL_SIDE),
        enabled,
        false,
        i18n::catalog(language).common.next_station,
        |ui, rect, base, hover| {
            let tint = icon_tint(base, palette.accent, hover);
            paint_icon(ui, Icon::SkipForward, rect.center(), 20.0, tint);
        },
    )
}

pub(crate) fn gear_button(ui: &mut egui::Ui, palette: &Palette, hint: &str) -> Response {
    icon_button(
        ui,
        palette,
        Vec2::splat(SIDE),
        true,
        false,
        hint,
        |ui, rect, base, hover| {
            paint_icon(
                ui,
                Icon::Settings,
                rect.center(),
                GLYPH,
                icon_tint(base, palette.accent, hover),
            );
        },
    )
}

pub(crate) fn moon_button(ui: &mut egui::Ui, palette: &Palette, hint: &str) -> Response {
    icon_button(
        ui,
        palette,
        Vec2::splat(SIDE),
        true,
        false,
        hint,
        |ui, rect, base, hover| {
            paint_icon(
                ui,
                Icon::Moon,
                rect.center(),
                GLYPH,
                icon_tint(base, palette.accent, hover),
            );
        },
    )
}

/// Кнопка раскрытия/сворачивания панели списка.
pub(crate) fn panel_button(ui: &mut egui::Ui, palette: &Palette, hint: &str) -> Response {
    icon_button(
        ui,
        palette,
        Vec2::splat(SIDE),
        true,
        false,
        hint,
        |ui, rect, base, hover| {
            paint_icon(
                ui,
                Icon::Panel,
                rect.center(),
                GLYPH,
                icon_tint(base, palette.accent, hover),
            );
        },
    )
}

/// Кнопка рулетки: кубик без подписи для шапки выпадающей панели.
pub(crate) fn roulette_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    enabled: bool,
    hint: &str,
) -> Response {
    icon_button(
        ui,
        palette,
        Vec2::splat(SIDE),
        enabled,
        false,
        hint,
        |ui, rect, base, hover| {
            paint_icon(
                ui,
                Icon::Dice,
                rect.center(),
                GLYPH,
                icon_tint(base, palette.accent, hover),
            );
        },
    )
}

/// Кнопка скрытия окна (×). По смыслу не выход: процесс и поток остаются живыми.
pub(crate) fn close_button(ui: &mut egui::Ui, palette: &Palette, hint: &str) -> Response {
    icon_button(
        ui,
        palette,
        Vec2::splat(SIDE),
        true,
        false,
        hint,
        |ui, rect, base, hover| {
            paint_icon(
                ui,
                Icon::Close,
                rect.center(),
                GLYPH,
                icon_tint(base, palette.accent, hover),
            );
        },
    )
}

/// Кнопка сортировки списка: две стрелки вверх/вниз.
pub(crate) fn sort_button(ui: &mut egui::Ui, palette: &Palette, hint: &str) -> Response {
    icon_button(
        ui,
        palette,
        Vec2::splat(SIDE),
        true,
        false,
        hint,
        |ui, rect, base, hover| {
            paint_icon(
                ui,
                Icon::Sort,
                rect.center(),
                GLYPH,
                icon_tint(base, palette.accent, hover),
            );
        },
    )
}

/// Показать/скрыть строку фильтров списка: активное состояние подсвечивает
/// воронку акцентом, как выбранный режим.
pub(crate) fn filters_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    hint: &str,
    visible: bool,
) -> Response {
    icon_button(
        ui,
        palette,
        Vec2::splat(SMALL_SIDE),
        true,
        visible,
        hint,
        move |ui, rect, base, hover| {
            let tint = if visible {
                palette.accent
            } else {
                icon_tint(base, palette.accent, hover)
            };
            paint_icon(ui, Icon::Funnel, rect.center(), 18.0, tint);
        },
    )
}

/// Сброс всех фильтров списка: перечёркнутая воронка гаснет, когда сбрасывать
/// нечего.
pub(crate) fn clear_filters_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    enabled: bool,
    hint: &str,
) -> Response {
    icon_button(
        ui,
        palette,
        Vec2::splat(SMALL_SIDE),
        enabled,
        false,
        hint,
        |ui, rect, base, hover| {
            paint_icon(
                ui,
                Icon::FunnelX,
                rect.center(),
                18.0,
                icon_tint(base, palette.accent, hover),
            );
        },
    )
}

/// Переключатель плотности строк: три линии в покое, четыре в компактном режиме.
pub(crate) fn density_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    hint: &str,
    compact: bool,
) -> Response {
    icon_button(
        ui,
        palette,
        Vec2::splat(SIDE),
        true,
        compact,
        hint,
        move |ui, rect, base, hover| {
            let tint = if compact {
                mix_colors(palette.accent, base, 0.15)
            } else {
                icon_tint(base, palette.accent, hover)
            };
            let icon = if compact {
                Icon::RowsCompact
            } else {
                Icon::Rows
            };
            paint_icon(ui, icon, rect.center(), GLYPH, tint);
        },
    )
}

/// Возврат из капсулы в полный Focus: четыре угла, разведённые наружу.
pub(crate) fn expand_button(ui: &mut egui::Ui, palette: &Palette, hint: &str) -> Response {
    icon_button(
        ui,
        palette,
        Vec2::splat(SMALL_SIDE),
        true,
        false,
        hint,
        |ui, rect, base, hover| {
            paint_icon(
                ui,
                Icon::Expand,
                rect.center(),
                18.0,
                icon_tint(base, palette.accent, hover),
            );
        },
    )
}

/// Кнопка карточки управления (знак «?»).
pub(crate) fn help_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    hint: &str,
    active: bool,
) -> Response {
    icon_button(
        ui,
        palette,
        Vec2::splat(SIDE),
        true,
        active,
        hint,
        move |ui, rect, base, hover| {
            let tint = if active {
                palette.accent
            } else {
                icon_tint(base, palette.accent, hover)
            };
            paint_icon(ui, Icon::Help, rect.center(), GLYPH, tint);
        },
    )
}

/// Поза мыши для карточки подсказок (эскиз, левая панель).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MousePose {
    /// Вращение — ПКМ drag.
    Orbit,
    /// Одиночный клик ЛКМ.
    Click,
    /// Колесо.
    Wheel,
}

/// Точки дуги окружности от `from` до `to` (рад), ~2 точки на 10°.
pub(crate) fn arc_points(center: Pos2, radius: f32, from: f32, to: f32) -> Vec<Pos2> {
    let steps = (((to - from).abs() / 0.35).ceil() as usize).max(2);
    (0..=steps)
        .map(|step| {
            let angle = from + (to - from) * step as f32 / steps as f32;
            pos2(
                center.x + angle.cos() * radius,
                center.y + angle.sin() * radius,
            )
        })
        .collect()
}

pub(crate) fn draw_mouse(
    painter: &egui::Painter,
    center: Pos2,
    size: f32,
    pose: MousePose,
    color: Color32,
) {
    let body = egui::Rect::from_center_size(
        center + Vec2::new(0.0, size * 0.1),
        Vec2::new(size * 0.85, size * 1.25),
    );
    painter.rect_stroke(
        body,
        egui::CornerRadius::same(6),
        Stroke::new(1.5, color),
        egui::StrokeKind::Middle,
    );
    match pose {
        MousePose::Orbit => {
            painter.line_segment(
                [
                    pos2(body.center().x, body.top() + size * 0.15),
                    pos2(body.center().x, body.top() + size * 0.5),
                ],
                Stroke::new(1.5, color),
            );
            let arrow_r = size * 0.85;
            painter.add(Shape::line(
                arc_points(
                    center,
                    arrow_r,
                    std::f32::consts::TAU * 1.25,
                    std::f32::consts::TAU * 1.75,
                ),
                Stroke::new(1.3, color),
            ));
        }
        MousePose::Click => {
            let button = egui::Rect::from_min_max(
                pos2(body.left() + 1.5, body.top() + 1.5),
                pos2(body.center().x - 1.0, body.top() + size * 0.55),
            );
            painter.rect_filled(button, egui::CornerRadius::same(3), color);
        }
        MousePose::Wheel => {
            painter.rect_filled(
                egui::Rect::from_center_size(
                    pos2(body.center().x, body.top() + size * 0.32),
                    Vec2::new(size * 0.16, size * 0.34),
                ),
                egui::CornerRadius::same(3),
                color,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_points_to_an_svg_asset() {
        for icon in [
            Icon::Search,
            Icon::Settings,
            Icon::Moon,
            Icon::Panel,
            Icon::Dice,
            Icon::Close,
            Icon::Star,
            Icon::StarFilled,
            Icon::Heart,
            Icon::HeartFilled,
            Icon::SkipBack,
            Icon::SkipForward,
            Icon::Stop,
            Icon::Volume,
            Icon::Expand,
            Icon::Help,
            Icon::Sort,
            Icon::Rows,
            Icon::RowsCompact,
            Icon::Globe,
            Icon::Clock,
            Icon::Funnel,
            Icon::FunnelX,
            Icon::Alert,
        ] {
            let uri = icon.source().uri().map(str::to_owned);
            assert!(
                uri.as_deref().is_some_and(|uri| uri.ends_with(".svg")),
                "{icon:?}: загрузчик SVG выбирает файл по расширению, получено {uri:?}"
            );
        }
    }

    #[test]
    fn hover_tint_reaches_accent_only_at_full_progress() {
        let base = Color32::from_rgb(100, 100, 100);
        let accent = Color32::from_rgb(220, 60, 40);
        assert_eq!(icon_tint(base, accent, 0.0), base);
        let halfway = icon_tint(base, accent, 1.0);
        assert!(halfway.r() > base.r() && halfway.r() < accent.r());
    }
}
