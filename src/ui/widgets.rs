//! Кастомные контролы «Orbital Desk» по референс-макету.
//!
//! Все виджеты рисуются painter'ом и не хранят состояние — только запросы
//! (`Response`/`bool`).

use eframe::egui::{
    self, Color32, Pos2, Shape, Stroke, TextureHandle, Vec2,
    epaint::{PathStroke, RoundedRect, tessellator},
    pos2,
};

use crate::i18n;
use crate::render::bar_color;
use crate::settings::{Language, sanitized_volume};

use super::animation;
use super::icons;
use super::palette::{Palette, mix_colors};

const PILL_RADIUS: f32 = 999.0;
const LIVE_REPAINT: std::time::Duration = std::time::Duration::from_millis(120);
/// Кадры пульсации костяка списка.
const SKELETON_REPAINT: std::time::Duration = std::time::Duration::from_millis(110);
/// Толщина верхнего блика стекла.
const GLASS_RIM: f32 = 1.5;
/// Осветление диска play/stop при наведении (ТЗ §3.4).
const HOVER_LIFT: f32 = 0.18;
/// Осветление обода активного диска.
const RING_LIFT: f32 = 0.45;
/// Потолок overshoot-шкалы: появление капсульной кнопки не может превышать
/// конечный размер более чем на 15 % (ADR-0032 §2).
const POP_LIMIT: f32 = 1.15;
/// Горизонтальные пояса градиента заливки громкости (ТЗ §3.4).
const VOLUME_FILL_BANDS: usize = 6;
/// Шаг громкости стрелками клавиатуры.
const VOLUME_KEY_STEP: f64 = 0.05;

/// Прозрачность цвета без обесцвечивания: масштабируется только альфа.
pub(crate) fn faded(color: Color32, alpha: f32) -> Color32 {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(
        r,
        g,
        b,
        (f32::from(a) * alpha.clamp(0.0, 1.0)).round() as u8,
    )
}

/// Вся палитра на заданной прозрачности: содержимое капсулы гаснет тем же
/// способом, что и её каркас (ADR-0032 §7 — движение только в отрисовке).
/// Контролы получают уже погашенную палитру, поэтому аллокации не меняются.
pub(crate) fn faded_palette(palette: &Palette, alpha: f32) -> Palette {
    let mut dim = *palette;
    for color in [
        &mut dim.card_fill,
        &mut dim.card_stroke,
        &mut dim.muted,
        &mut dim.error,
        &mut dim.accent,
        &mut dim.marker,
        &mut dim.space_top,
        &mut dim.space_bottom,
        &mut dim.glass_top,
        &mut dim.glass_bottom,
        &mut dim.rim_light,
        &mut dim.live,
        &mut dim.chip_fill,
        &mut dim.chip_stroke,
        &mut dim.text,
    ] {
        *color = faded(*color, alpha);
    }
    dim
}

/// Групповое погасшение отрисовки: нужно для контролов, чей цвет приходит из
/// `visuals` egui, а не из нашей палитры (кнопки-иконки). Прозрачность
/// умножается на текущую, поэтому вложенные группы не теряют общий фэйд.
/// Аллокации не двигаются — гаснет только нарисованное внутри `add`.
pub(crate) fn faded_group<T>(
    ui: &mut egui::Ui,
    alpha: f32,
    add: impl FnOnce(&mut egui::Ui) -> T,
) -> T {
    let base = ui.opacity();
    ui.multiply_opacity(animation::alpha(alpha));
    let result = add(ui);
    ui.set_opacity(base);
    result
}

/// Шкала появления капсульной кнопки: ease-out с перелётом через 1.0, но не
/// дальше [`POP_LIMIT`]. Reduce motion сразу даёт конечный размер (ADR-0032 §3).
pub(crate) fn pop_scale(progress: f32, reduce: bool) -> f32 {
    if reduce {
        return 1.0;
    }
    animation::eased(progress, animation::Curve::Overshoot).clamp(0.0, POP_LIMIT)
}

/// Прогрессы hover/press одного контрола (ADR-0034). При reduce motion
/// `animation_time` равен нулю, и egui сразу отдаёт конечное значение.
pub(crate) fn control_motion(
    ui: &egui::Ui,
    id: egui::Id,
    hovered: bool,
    pressed: bool,
) -> (f32, f32) {
    let time = if ui.style().animation_time > 0.0 {
        animation::HOVER
    } else {
        0.0
    };
    let hover = ui
        .ctx()
        .animate_bool_with_time(id.with("orbitfm-hover"), hovered, time);
    let press = ui
        .ctx()
        .animate_bool_with_time(id.with("orbitfm-press"), pressed, time);
    (hover, press)
}

/// Кольцо клавиатурного фокуса вокруг прямоугольного контрола.
pub(crate) fn focus_ring(ui: &egui::Ui, response: &egui::Response, rect: egui::Rect, radius: f32) {
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.expand(1.5),
            radius,
            Stroke::new(1.8, ui.visuals().selection.bg_fill),
            egui::StrokeKind::Outside,
        );
    }
}

/// Кольцо фокуса для круглых кнопок.
pub(crate) fn circle_focus_ring(
    ui: &egui::Ui,
    response: &egui::Response,
    center: Pos2,
    radius: f32,
) {
    if response.has_focus() {
        ui.painter().circle_stroke(
            center,
            radius,
            Stroke::new(1.8, ui.visuals().selection.bg_fill),
        );
    }
}

/// Контур скруглённого прямоугольника — те же точки, что у обводки egui:
/// берём их у тесселятора, чтобы заливка и рамка совпадали пиксель в пиксель.
fn rounded_contour(rounded: RoundedRect) -> Vec<Pos2> {
    let mut points = Vec::new();
    tessellator::path::rounded_rectangle(&mut points, rounded);
    points
}

/// Заливка стекла одним мешем: цвет вершины зависит только от `y`, а
/// растеризатор интерполирует линейную функцию без искажений. В отличие от
/// полос, швов между «слоями» градиента нет, а скругления задаются точным
/// контуром, а не прямоугольным шагом.
fn gradient_fill(rounded: RoundedRect, top: Color32, bottom: Color32) -> egui::Mesh {
    let rect = rounded.rect();
    gradient_fill_with(rounded, |y| {
        mix_colors(top, bottom, (y - rect.top()) / rect.height().max(1.0))
    })
}

/// Тот же веер, но цвет каждой вершины задаёт вызывающий: так столбцы
/// визуализатора получают градиент по абсолютной высоте поля.
fn gradient_fill_with(rounded: RoundedRect, color_at: impl Fn(f32) -> Color32) -> egui::Mesh {
    let rect = rounded.rect();
    let contour = rounded_contour(rounded);
    let mut mesh = egui::Mesh::default();
    mesh.reserve_vertices(contour.len() + 1);
    mesh.reserve_triangles(contour.len().saturating_sub(1));
    mesh.colored_vertex(rect.center(), color_at(rect.center().y));
    for point in &contour {
        mesh.colored_vertex(*point, color_at(point.y));
    }
    let count = contour.len() as u32;
    for index in 0..count {
        mesh.add_triangle(0, 1 + index, 1 + (index + 1) % count);
    }
    mesh
}

/// Верхний блик: открытая линия по верхней кромке вместе с закруглениями.
/// Прямоугольная полоса блика торчала бы из углов панели.
fn top_rim(rounded: RoundedRect) -> Vec<Pos2> {
    let rect = rounded.rect();
    let radius = rounded.corner_radius().nw.max(0.0);
    rounded_contour(rounded)
        .into_iter()
        .filter(|point| point.y <= rect.top() + radius + 0.25)
        .collect()
}

/// Стеклянная панель: вертикальный градиент `glass_top → glass_bottom`,
/// верхний блик 1–2 px и мягкая рамка. `egui::Frame` умеет только сплошную
/// заливку, а `Shape::gradient_rect` не скругляет углы, поэтому фон — один
/// меш по контуру скруглённого прямоугольника.
pub(crate) fn glass_panel(
    painter: &egui::Painter,
    rect: egui::Rect,
    palette: &Palette,
    radius: f32,
    alpha: f32,
) {
    if !rect.is_finite() || rect.width() < 2.0 || rect.height() < 2.0 {
        return;
    }
    let rounded = RoundedRect::new(rect, radius);
    painter.add(Shape::mesh(gradient_fill(
        rounded,
        faded(palette.glass_top, alpha),
        faded(palette.glass_bottom, alpha),
    )));
    painter.add(Shape::line(
        top_rim(rounded),
        PathStroke::new(GLASS_RIM, faded(palette.rim_light, alpha)),
    ));
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(1.0, faded(palette.card_stroke, alpha)),
        egui::StrokeKind::Middle,
    );
}

/// Pill-кнопка сегментированного переключателя (Country / Genre).
pub(crate) fn pill(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    selected: bool,
) -> egui::Response {
    let text_color = if selected {
        contrast_color(palette.accent)
    } else {
        palette.text
    };
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(13.0),
        text_color,
    );
    let size = Vec2::new(galley.size().x + 28.0, 26.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let (hover, press) = control_motion(
        ui,
        response.id,
        response.hovered(),
        response.is_pointer_button_down_on(),
    );
    if ui.is_rect_visible(rect) {
        let (fill, stroke) = if selected {
            (
                mix_colors(palette.accent, Color32::WHITE, HOVER_LIFT * hover),
                Stroke::NONE,
            )
        } else {
            (
                mix_colors(
                    Color32::TRANSPARENT,
                    palette.chip_fill,
                    hover.max(0.45 * press),
                ),
                Stroke::new(1.0, faded(palette.chip_stroke, hover)),
            )
        };
        ui.painter()
            .rect(rect, PILL_RADIUS, fill, stroke, egui::StrokeKind::Middle);
        ui.painter()
            .galley(rect.center() - galley.size() * 0.5, galley, text_color);
    }
    focus_ring(ui, &response, rect, PILL_RADIUS);
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, true, selected, label));
    response
}

/// Жанровый чип (All / Rock / Pop / …).
pub(crate) fn chip(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    selected: bool,
) -> egui::Response {
    let text_color = if selected {
        contrast_color(palette.accent)
    } else {
        palette.text
    };
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(11.5),
        text_color,
    );
    let size = Vec2::new(galley.size().x + 18.0, 22.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let (hover, press) = control_motion(
        ui,
        response.id,
        response.hovered(),
        response.is_pointer_button_down_on(),
    );
    if ui.is_rect_visible(rect) {
        let (fill, stroke) = if selected {
            (
                mix_colors(palette.accent, Color32::WHITE, 0.08 * hover),
                Stroke::NONE,
            )
        } else {
            (
                mix_colors(
                    palette.chip_fill,
                    palette.accent,
                    0.14 * hover + 0.06 * press,
                ),
                Stroke::new(
                    1.0,
                    mix_colors(palette.chip_stroke, palette.accent, 0.65 * hover),
                ),
            )
        };
        ui.painter()
            .rect(rect, PILL_RADIUS, fill, stroke, egui::StrokeKind::Middle);
        ui.painter()
            .galley(rect.center() - galley.size() * 0.5, galley, text_color);
    }
    focus_ring(ui, &response, rect, PILL_RADIUS);
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, selected, label));
    response
}

/// Результат pill-поля поиска.
#[derive(Clone)]
pub(crate) struct FieldOutput {
    pub rect: egui::Rect,
    /// Enter отпущен (focus lost) — сигнал к поиску.
    pub enter: bool,
    /// Ответ поля (для якоря `Popup::from_response`).
    pub response: egui::Response,
}

/// Pill-поле поиска с лупой.
pub(crate) fn search_field(
    ui: &mut egui::Ui,
    palette: &Palette,
    id: egui::Id,
    hint: &str,
    text: &mut String,
    width: f32,
) -> FieldOutput {
    let (rect, background) = ui.allocate_exact_size(Vec2::new(width, 30.0), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return FieldOutput {
            rect,
            enter: false,
            response: background,
        };
    }
    ui.painter()
        .rect_filled(rect, PILL_RADIUS, palette.chip_fill);
    ui.painter().rect_stroke(
        rect,
        PILL_RADIUS,
        Stroke::new(1.0, palette.chip_stroke),
        egui::StrokeKind::Inside,
    );
    let inner = egui::Rect::from_min_max(
        pos2(rect.left() + 28.0, rect.top()),
        pos2(rect.right() - 10.0, rect.bottom()),
    );
    // Рамку поля рисуем сами: собственный frame TextEdit дал бы вторую
    // прямоугольную обводку внутри pill-формы.
    let edit = egui::TextEdit::singleline(text)
        .hint_text(hint)
        .desired_width(inner.width() - 8.0)
        .align(egui::Align2::LEFT_CENTER)
        .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 5)))
        .id(id);
    let response = ui.put(inner, edit);
    let focused = response.has_focus();
    let (hover, _) = control_motion(ui, id, background.hovered() || response.hovered(), false);
    let highlight = if focused { 1.0 } else { hover };
    if highlight > 0.0 {
        let rim = mix_colors(palette.chip_stroke, palette.accent, 0.85 * highlight);
        ui.painter().rect_stroke(
            rect,
            PILL_RADIUS,
            Stroke::new(1.0, rim),
            egui::StrokeKind::Inside,
        );
    }
    icons::paint_icon(
        ui,
        icons::Icon::Search,
        pos2(rect.left() + 17.0, rect.center().y),
        15.0,
        mix_colors(palette.muted, palette.accent, highlight),
    );
    let enter = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
    FieldOutput {
        rect,
        enter,
        response: if response.has_focus() || enter {
            response
        } else {
            background
        },
    }
}

/// Бейдж «● LIVE» с пульсом точки; сам просит перерисовку.
pub(crate) fn live_badge(ui: &mut egui::Ui, palette: &Palette, active: bool, language: Language) {
    let text = i18n::catalog(language);
    let time = ui.input(|input| input.time);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(52.0, 18.0), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let color = if active { palette.live } else { palette.muted };
    let pulse = if active {
        (0.5 + 0.5 * (time * 3.0).sin()) as f32
    } else {
        0.0
    };
    let dot = pos2(rect.left() + 8.0, rect.center().y);
    if active {
        // ТЗ §3.4: мягкое зелёное свечение «пилюли» — два слоя вместо тени.
        ui.painter()
            .rect_filled(rect.expand(4.0), PILL_RADIUS, faded(palette.live, 0.05));
        ui.painter()
            .rect_filled(rect.expand(2.0), PILL_RADIUS, faded(palette.live, 0.10));
    }
    ui.painter().circle_filled(dot, 2.5 + 1.5 * pulse, color);
    if active {
        ui.painter().circle_stroke(
            dot,
            4.5 + 2.5 * pulse,
            Stroke::new(1.0, color.linear_multiply(0.5 - 0.4 * pulse)),
        );
        ui.ctx().request_repaint_after(LIVE_REPAINT);
    }
    ui.painter().text(
        pos2(rect.left() + 16.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        text.common.live,
        egui::FontId::proportional(10.5),
        color,
    );
}

/// Столбики эквалайзера внутри готового прямоугольника — без аллокации.
pub(crate) fn eq_bars_in(
    ui: &egui::Ui,
    palette: &Palette,
    rect: egui::Rect,
    bars: usize,
    active: bool,
) {
    if !ui.is_rect_visible(rect) || bars == 0 {
        return;
    }
    let time = ui.input(|input| input.time);
    let height = rect.height();
    let step = rect.width() / bars as f32;
    for bar in 0..bars {
        let phase = bar as f32 * 1.7;
        let level = if active {
            0.35 + 0.65 * (time as f32 * 5.5 + phase).sin().abs()
        } else {
            0.25
        };
        let x = rect.left() + step * (bar as f32 + 0.5);
        let bar_width = (step - 2.0).max(1.5) * 0.5;
        let h = (height * level).max(2.0);
        ui.painter().rect_filled(
            egui::Rect::from_min_max(
                pos2(x - bar_width, rect.bottom() - h),
                pos2(x + bar_width, rect.bottom()),
            ),
            1.5,
            if active { palette.live } else { palette.muted },
        );
    }
    if active {
        ui.ctx().request_repaint_after(LIVE_REPAINT);
    }
}

/// Зазор между столбцами, px: тонкие полосы с воздухом, как у cava.
const VIZ_BAR_GAP: f32 = 2.0;
/// Скругление столбца и высота «шапки» пика, px.
const VIZ_BAR_RADIUS: f32 = 1.0;
const VIZ_PEAK_HEIGHT: f32 = 2.0;
const VIZ_BAR_MIN_HEIGHT: f32 = 1.5;

/// Узкий визуализатор для узких поверхностей: 16 полос, в 2,5 раза уже столового
/// `VIZ_SIZE` (156×46). Число полос фиксировано требованием и не связано с
/// настройкой `viz_bars` стола (ADR-0022). Капсула Overlay и шапка выпадающей
/// панели используют один размер — у обеих контролы живут в одной строке с
/// текстом.
pub(crate) const NARROW_VIZ_BARS: usize = 16;
pub(crate) const NARROW_VIZ_SIZE: (f32, f32) = (62.0, 16.0);

/// Прореживание полос модели до фиксированного числа: bucket-max по целым
/// интервалам источника, без аллокаций (стек-массив). При `len < N` интервалы
/// перекрываются, поэтому пустых корзин не возникает вовсе.
pub(crate) fn bucket_max<const N: usize>(source: &[f32]) -> [f32; N] {
    let mut out = [0.0; N];
    if source.is_empty() {
        return out;
    }
    let len = source.len();
    for (index, slot) in out.iter_mut().enumerate() {
        let start = index * len / N;
        let end = ((index + 1) * len / N).clamp(start + 1, len);
        *slot = source[start..end].iter().copied().fold(0.0, f32::max);
    }
    out
}

/// Столбцы визуализатора: градиент привязан к высоте поля, поэтому короткий
/// столбец остаётся зелёным, а жёлтый и красный появляются только на подъёме.
/// Пики висят над столбцами отдельными шапками.
pub(crate) fn viz_bars(
    ui: &egui::Ui,
    palette: &Palette,
    rect: egui::Rect,
    levels: &[f32],
    peaks: &[f32],
) {
    if !ui.is_rect_visible(rect) || levels.is_empty() || rect.width() < 8.0 || rect.height() < 8.0 {
        return;
    }
    let painter = ui.painter();
    let step = rect.width() / levels.len() as f32;
    let width = (step - VIZ_BAR_GAP).max(1.5).min(step);
    let color_at = |y: f32| bar_color(1.0 - ((y - rect.top()) / rect.height()).clamp(0.0, 1.0));

    for (index, level) in levels.iter().enumerate() {
        let x_center = rect.left() + step * (index as f32 + 0.5);
        let level = if level.is_finite() {
            level.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let height = (rect.height() * level).max(VIZ_BAR_MIN_HEIGHT);
        let bar = egui::Rect::from_min_max(
            pos2(x_center - width * 0.5, rect.bottom() - height),
            pos2(x_center + width * 0.5, rect.bottom()),
        );
        let rounded = RoundedRect::new(bar, VIZ_BAR_RADIUS.min(width * 0.5));
        painter.add(Shape::mesh(gradient_fill_with(rounded, color_at)));

        if let Some(peak) = peaks.get(index).copied().filter(|peak| peak.is_finite()) {
            let peak = peak.clamp(0.0, 1.0);
            if peak > level + 0.01 {
                let y = (rect.bottom() - rect.height() * peak).max(rect.top() + VIZ_PEAK_HEIGHT);
                let cap = egui::Rect::from_min_max(
                    pos2(bar.left(), y - VIZ_PEAK_HEIGHT),
                    pos2(bar.right(), y),
                );
                painter.rect_filled(
                    cap,
                    VIZ_PEAK_HEIGHT * 0.5,
                    mix_colors(color_at(y), Color32::WHITE, 0.45),
                );
            }
        }
    }

    // Тонкая опорная линия: столбцы не «висят» в воздухе.
    painter.hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, palette.chip_stroke),
    );
}

/// Результат слайдера громкости: значение изменилось и/или изменение
/// закоммичено (ADR-0031, commit-on-release). Пока идёт drag — только preview,
/// запись в настройки происходит на отпускание, клик по треку или клавишу.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct SliderResponse {
    pub(crate) changed: bool,
    pub(crate) committed: bool,
}

/// Громкость: динамик + тонкий accent-трек с круглой ручкой в заданной ширине:
/// капсуле Overlay нужен короткий трек, transport резервирует место под
/// визуализатор. Стрелки шагают на 5 %, Home/End — в крайние положения.
pub(crate) fn volume_slider_width(
    ui: &mut egui::Ui,
    palette: &Palette,
    volume: &mut f64,
    width: f32,
    language: Language,
) -> SliderResponse {
    let text = i18n::catalog(language);
    let label = text.transport.volume;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, 18.0), egui::Sense::click_and_drag());
    focus_ring(ui, &response, rect, 9.0);

    let mut changed = false;
    if response.has_focus() {
        let (down, up, home, end) = ui.input(|input| {
            (
                input.key_pressed(egui::Key::ArrowDown) || input.key_pressed(egui::Key::ArrowLeft),
                input.key_pressed(egui::Key::ArrowUp) || input.key_pressed(egui::Key::ArrowRight),
                input.key_pressed(egui::Key::Home),
                input.key_pressed(egui::Key::End),
            )
        });
        let next = if home {
            Some(0.0)
        } else if end {
            Some(1.0)
        } else if up {
            Some((*volume + VOLUME_KEY_STEP).min(1.0))
        } else if down {
            Some((*volume - VOLUME_KEY_STEP).max(0.0))
        } else {
            None
        };
        if let Some(next) = next
            && next != *volume
        {
            *volume = next;
            changed = true;
        }
    }

    // Колесо — третий дискретный ввод наравне со стрелками и кликом (ADR-0031 §7):
    // только когда указатель над слайдером, иначе оно ворует скролл у списка
    // станций и глобуса. Шаг считаем из сырых событий: egui растягивает щелчок
    // на несколько кадров (`smooth_scroll_delta` = 40 px + угасающий хвост), и
    // каждый хвост дал бы лишнюю запись в settings.json. Обнуляем сглаженную
    // дельту на всех кадрах под указателем, чтобы остаток щелчка не доехал
    // `ScrollArea` drawer'а, а повтор тех же входных данных в многопроходной
    // отрисовке отсекаем по `time` — иначе один щелчок применился бы дважды.
    let mut wheel = false;
    if response.hovered() {
        // Нормировка по line_scroll_speed (на native — 40 px за щелчок):
        // щелчок — ровно один шаг 5 %, как у стрелок.
        let line_px = ui
            .ctx()
            .memory(|mem| mem.options.input_options.line_scroll_speed);
        let (notches, time) = ui.input(|input| {
            let mut notches = 0.0f32;
            for event in &input.events {
                if let egui::Event::MouseWheel {
                    unit,
                    delta,
                    phase: egui::TouchPhase::Move,
                    ..
                } = event
                {
                    notches += match unit {
                        egui::MouseWheelUnit::Point => delta.y,
                        egui::MouseWheelUnit::Line => delta.y * line_px,
                        egui::MouseWheelUnit::Page => {
                            delta.y * input.viewport_rect().height().max(1.0)
                        }
                    };
                }
            }
            (notches, input.time)
        });
        ui.input_mut(|input| input.smooth_scroll_delta.y = 0.0);
        let wheel_key = response.id.with("volume_wheel");
        let handled = ui
            .ctx()
            .data(|d| d.get_temp::<f64>(wheel_key) == Some(time));
        if notches != 0.0 && !handled {
            ui.ctx().data_mut(|d| d.insert_temp(wheel_key, time));
            let unit = f64::from(line_px.max(1.0));
            let next = sanitized_volume(*volume + f64::from(notches) / unit * VOLUME_KEY_STEP);
            if next != *volume {
                *volume = next;
                changed = true;
                wheel = true;
            }
        }
    }

    // Иконка динамика внутри выделенного прямоугольника — иначе она наползает на
    // соседний чип.
    icons::paint_icon(
        ui,
        icons::Icon::Volume,
        pos2(rect.left() + 9.0, rect.center().y),
        14.0,
        palette.text,
    );
    let track = egui::Rect::from_min_max(
        pos2(rect.left() + 20.0, rect.center().y - 2.0),
        pos2(rect.right() - 6.0, rect.center().y + 2.0),
    );
    ui.painter().rect_filled(track, 2.0, palette.chip_fill);
    let knob_x = track.left() + track.width() * *volume as f32;
    // ТЗ §3.4: заливка громкости — мягкий градиент accent → светлее вправо.
    let fill_width = (knob_x - track.left()).max(0.0);
    if fill_width >= 1.0 {
        let lighter = mix_colors(palette.accent, Color32::WHITE, 0.28);
        for band in 0..VOLUME_FILL_BANDS {
            let from = track.left() + fill_width * band as f32 / VOLUME_FILL_BANDS as f32;
            // Перекрытие в полпикселя: между поясами не должно быть швов.
            let to = track.left() + fill_width * (band + 1) as f32 / VOLUME_FILL_BANDS as f32 + 0.5;
            ui.painter().rect_filled(
                egui::Rect::from_min_max(pos2(from, track.top()), pos2(to, track.bottom())),
                2.0,
                mix_colors(
                    palette.accent,
                    lighter,
                    band as f32 / (VOLUME_FILL_BANDS - 1) as f32,
                ),
            );
        }
    }
    ui.painter()
        .circle_filled(pos2(knob_x, track.center().y), 5.0, palette.text);
    if (response.dragged() || response.clicked())
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let fraction = ((pointer.x - track.left()) / track.width()).clamp(0.0, 1.0);
        let previous = *volume;
        *volume = f64::from(fraction);
        changed |= *volume != previous;
    }
    response.widget_info(|| egui::WidgetInfo::slider(true, *volume, label));
    // Коммит — отпускание drag, клик, клавиша или колесо: всё, что не «продолжающийся
    // drag» (ADR-0031). Колесо дискретно, как стрелки, поэтому пишет настройки
    // сразу. На кадре отпускания changed может быть false (значение не двинулось),
    // но drag_stopped всё равно закрывает pending.
    let committed = wheel || response.drag_stopped() || (changed && !response.dragged());
    response.on_hover_text(text.settings.volume_hint);
    SliderResponse { changed, committed }
}

/// Большая круглая play/pause с accent-кольцом.
pub(crate) fn circle_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    playing: bool,
    enabled: bool,
    hint: &str,
) -> egui::Response {
    circle_button_sized(ui, palette, 40.0, playing, enabled, hint)
}

/// Та же кнопка заданного диаметра: капсулы используют меньший размер.
pub(crate) fn circle_button_sized(
    ui: &mut egui::Ui,
    palette: &Palette,
    size: f32,
    playing: bool,
    enabled: bool,
    hint: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::splat(size),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    if ui.is_rect_visible(rect) {
        let center = rect.center();
        // ТЗ §3.4: primary play — сплошной accent, чуть светлее при наведении.
        let hoverable = response.hovered() && enabled;
        let fill = if !enabled {
            palette.chip_fill
        } else if hoverable {
            lift(palette.accent, HOVER_LIFT)
        } else {
            palette.accent
        };
        let ring = if enabled {
            lift(palette.accent, RING_LIFT)
        } else {
            palette.muted
        };
        let glyph = if enabled {
            contrast_color(fill)
        } else {
            palette.muted
        };
        let radius = draw_disc(ui, rect, 1.0, fill, ring, glyph, playing);
        if radius > 0.0 {
            circle_focus_ring(ui, &response, center, radius + 2.0);
        }
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, hint));
    if enabled {
        response.on_hover_text(hint)
    } else {
        response
    }
}

/// Капсульная кнопка play/stop на один кадр: всё, что нужно для её отрисовки.
///
/// Поля посчитаны вызывающим — переход цвета между состояниями воспроизведения
/// и общий фэйд входа принадлежат капсуле, а не виджету.
pub(crate) struct CapsuleDisc<'a> {
    /// Палитра уже погашена фэйдом входа: из неё берутся лишь приглушённые
    /// цвета неактивной кнопки.
    pub palette: &'a Palette,
    /// Диаметр аллокации; шкала живёт только в отрисовке.
    pub size: f32,
    /// Шкала появления из [`pop_scale`].
    pub pop: f32,
    /// Цвет диска: смешение «пауза → эфир» с альфой входа.
    pub fill: Color32,
    pub playing: bool,
    pub enabled: bool,
    pub hint: &'a str,
}

/// Капсульная кнопка play/stop: появление с overshoot (`pop`), press-сжатие по
/// [`animation::PRESS`] и цвет диска из [`CapsuleDisc`].
///
/// Аллокация всегда [`Vec2::splat(disc.size)`]: шкала живёт только в отрисовке,
/// поэтому hit-testing не смещается (ADR-0032 §7).
pub(crate) fn capsule_button(
    ui: &mut egui::Ui,
    motion: &mut animation::FrameMotion<'_>,
    press_key: egui::Id,
    disc: &CapsuleDisc<'_>,
) -> egui::Response {
    let CapsuleDisc {
        palette,
        size,
        pop,
        fill,
        playing,
        enabled,
        hint,
    } = *disc;
    let (rect, response) = ui.allocate_exact_size(
        Vec2::splat(size),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    if ui.is_rect_visible(rect) {
        let center = rect.center();
        let mut scale = pop.max(0.0);
        // Reduce motion глушит шкалу: переход не опрашивается вовсе, чтобы не
        // занимать слот в PresenceStore окна.
        if !motion.reduce {
            let pressed = response.is_pointer_button_down_on();
            let progress = motion.keyed.value(
                ui.ctx(),
                press_key,
                pressed,
                animation::PRESS,
                motion.reduce,
            );
            scale *= pressed_scale(progress);
        }
        let hoverable = response.hovered() && enabled;
        let fill = if !enabled {
            palette.chip_fill
        } else if hoverable {
            lift(fill, HOVER_LIFT)
        } else {
            fill
        };
        let ring = if enabled {
            lift(fill, RING_LIFT)
        } else {
            palette.muted
        };
        let glyph = if enabled {
            // Глиф наследует альфу диска: погасшая при входе капсула не должна
            // светиться иконкой. Выбор контраста берём из непрозрачного оттенка —
            // у premultiplied-каналов погасшего диска luminance ниже, и глиф мог
            // бы перевернуться посреди входа.
            let [r, g, b, _] = fill.to_srgba_unmultiplied();
            faded(
                contrast_color(Color32::from_rgb(r, g, b)),
                f32::from(fill.a()) / 255.0,
            )
        } else {
            palette.muted
        };
        let radius = draw_disc(ui, rect, scale, fill, ring, glyph, playing);
        if radius > 0.0 {
            circle_focus_ring(ui, &response, center, radius + 2.0);
        }
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, hint));
    if enabled {
        response.on_hover_text(hint)
    } else {
        response
    }
}

/// Диск с глифом play/pause, нарисованный от центра `rect` с множителем `scale`.
/// Возвращает радиус отрисованного диска (0.0 — рисовать нечего).
fn draw_disc(
    ui: &egui::Ui,
    rect: egui::Rect,
    scale: f32,
    fill: Color32,
    ring: Color32,
    glyph: Color32,
    playing: bool,
) -> f32 {
    let center = rect.center();
    let radius = (rect.width() * 0.5 - 1.0) * scale;
    if radius <= 0.0 {
        return 0.0;
    }
    ui.painter().circle_filled(center, radius, fill);
    ui.painter()
        .circle_stroke(center, radius, Stroke::new(1.6, ring));
    if playing {
        for dx in [-4.0, 4.0] {
            ui.painter().rect_filled(
                egui::Rect::from_center_size(
                    center + Vec2::new(dx * scale, 0.0),
                    Vec2::new(3.2, 12.0) * scale,
                ),
                1.5 * scale,
                glyph,
            );
        }
    } else {
        let point = |dx: f32, dy: f32| center + Vec2::new(dx, dy) * scale;
        ui.painter().add(Shape::convex_polygon(
            vec![point(-4.5, -7.0), point(-4.5, 7.0), point(7.0, 0.0)],
            glyph,
            Stroke::NONE,
        ));
    }
    radius
}

/// Press-сжатие: 1.0 в покое, [`animation::PRESS_SCALE`] при удержании.
fn pressed_scale(progress: f32) -> f32 {
    1.0 - (1.0 - animation::PRESS_SCALE) * animation::alpha(progress)
}

/// Осветление к белому без изменения альфы: hover-подсветка не должна
/// «пробивать» фэйд входа капсулы. Каналы берём в unmultiplied-виде, потому что
/// `Color32` хранит их уже умноженными на альфу.
fn lift(color: Color32, amount: f32) -> Color32 {
    let [r, g, b, _] = mix_colors(color, Color32::WHITE, amount).to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, color.a())
}

/// Однорядный сегментированный переключатель вкладок. Активный сегмент
/// подсвечен accent-пилюлей, которая плавно переезжает к новому выбору.
///
/// Пилюля живёт только в отрисовке: каждый сегмент занимает фиксированную
/// долю ширины, поэтому хиттест не зависит от анимации (ADR-0032 §7).
/// Возвращает индекс нажатого сегмента.
pub(crate) fn segmented_tabs(
    ui: &mut egui::Ui,
    palette: &Palette,
    id_salt: &str,
    labels: &[&str],
    selected: usize,
    reduce: bool,
) -> Option<usize> {
    if labels.is_empty() {
        return None;
    }
    let selected = selected.min(labels.len() - 1);
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return None;
    }

    let track = rect.shrink(0.5);
    ui.painter()
        .rect_filled(track, PILL_RADIUS, palette.chip_fill);
    ui.painter().rect_stroke(
        track,
        PILL_RADIUS,
        Stroke::new(1.0, palette.chip_stroke),
        egui::StrokeKind::Inside,
    );

    let inner = track.shrink(3.0);
    let segment = inner.width() / labels.len() as f32;
    let position = if reduce {
        selected as f32
    } else {
        ui.ctx().animate_value_with_time(
            ui.scope_id().with((id_salt, "pill")),
            selected as f32,
            animation::TAB,
        )
    };
    let pill = egui::Rect::from_min_size(
        pos2(inner.left() + segment * position, inner.top()),
        Vec2::new(segment, inner.height()),
    );
    ui.painter().rect_filled(pill, PILL_RADIUS, palette.accent);

    let mut clicked = None;
    for (index, label) in labels.iter().enumerate() {
        let cell = egui::Rect::from_min_size(
            pos2(inner.left() + segment * index as f32, inner.top()),
            Vec2::new(segment, inner.height()),
        );
        let response = ui.interact(
            cell,
            ui.scope_id().with((id_salt, index)),
            egui::Sense::click(),
        );
        let active = index == selected;
        let (hover, press) = control_motion(
            ui,
            response.id,
            response.hovered(),
            response.is_pointer_button_down_on(),
        );
        if !active && (hover > 0.0 || press > 0.0) {
            ui.painter().rect_filled(
                cell.shrink(1.0),
                PILL_RADIUS,
                faded(palette.text, 0.05 * hover + 0.04 * press),
            );
        }
        let color = if active {
            contrast_color(palette.accent)
        } else {
            mix_colors(palette.muted, palette.text, hover)
        };
        ui.painter().text(
            cell.center(),
            egui::Align2::CENTER_CENTER,
            *label,
            egui::FontId::proportional(12.5),
            color,
        );
        focus_ring(ui, &response, cell.shrink(1.0), PILL_RADIUS);
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::Tab, true, index == selected, *label)
        });
        if response.clicked() {
            clicked = Some(index);
        }
    }
    clicked
}

/// Знак центрированной заглушки списка.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EmptyMark {
    Globe,
    Search,
    Star,
    Clock,
    Filter,
}

pub(crate) struct EmptyState<'a> {
    pub mark: EmptyMark,
    pub title: &'a str,
    pub hint: Option<&'a str>,
    pub action: Option<&'a str>,
}

/// Центрированная заглушка списка: знак в круге, заголовок, подсказка и
/// необязательное действие. `height` — высота вьюпорта списка: заглушка
/// центрируется по ней, а не по растущему содержимому `ScrollArea`.
/// Возвращает true, если действие нажато.
pub(crate) fn empty_state(
    ui: &mut egui::Ui,
    palette: &Palette,
    state: EmptyState<'_>,
    height: f32,
) -> bool {
    let EmptyState {
        mark,
        title,
        hint,
        action,
    } = state;
    let height = height.max(120.0);
    let width = ui.available_width();
    let block = 96.0 + hint.map_or(0.0, |_| 34.0) + action.map_or(0.0, |_| 42.0);
    let top = ((height - block) * 0.34).clamp(6.0, 180.0);
    let mut clicked = false;
    ui.allocate_ui_with_layout(
        Vec2::new(width, height),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            ui.add_space(top);
            ui.set_max_width((width - 36.0).max(120.0));
            ui.vertical_centered(|ui| {
                let (mark_rect, _) =
                    ui.allocate_exact_size(Vec2::splat(52.0), egui::Sense::hover());
                if ui.is_rect_visible(mark_rect) {
                    let center = mark_rect.center();
                    ui.painter().circle_filled(center, 25.0, palette.chip_fill);
                    ui.painter()
                        .circle_stroke(center, 25.0, Stroke::new(1.0, palette.chip_stroke));
                    let (icon, size) = match mark {
                        EmptyMark::Globe => (icons::Icon::Globe, 22.0),
                        EmptyMark::Search => (icons::Icon::Search, 21.0),
                        EmptyMark::Star => (icons::Icon::Star, 22.0),
                        EmptyMark::Clock => (icons::Icon::Clock, 22.0),
                        EmptyMark::Filter => (icons::Icon::Funnel, 21.0),
                    };
                    icons::paint_icon(ui, icon, center, size, palette.accent);
                }
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new(title)
                        .size(14.0)
                        .strong()
                        .color(palette.text),
                );
                if let Some(hint) = hint {
                    ui.add_space(3.0);
                    ui.add(
                        egui::Label::new(egui::RichText::new(hint).size(11.5).color(palette.muted))
                            .wrap()
                            .halign(egui::Align::Center),
                    );
                }
                if let Some(action) = action {
                    ui.add_space(12.0);
                    clicked = action_pill(ui, palette, action).clicked();
                }
            });
        },
    );
    clicked
}

/// Accent-кнопка действия: pill-форма, светлеет при наведении.
pub(crate) fn action_pill(ui: &mut egui::Ui, palette: &Palette, label: &str) -> egui::Response {
    let font = egui::FontId::proportional(12.5);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), palette.text);
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(galley.size().x + 30.0, 30.0),
        egui::Sense::click(),
    );
    if ui.is_rect_visible(rect) {
        let (hover, _) = control_motion(
            ui,
            response.id,
            response.hovered(),
            response.is_pointer_button_down_on(),
        );
        let fill = lift(palette.accent, HOVER_LIFT * hover);
        ui.painter().rect_filled(rect, PILL_RADIUS, fill);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            font,
            contrast_color(fill),
        );
    }
    focus_ring(ui, &response, rect, PILL_RADIUS);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    response
}

/// Малошумная кнопка на всю ширину: подложка появляется только на hover.
pub(crate) fn quiet_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    color: Color32,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let (hover, _) = control_motion(ui, response.id, response.hovered(), false);
        if hover > 0.0 {
            ui.painter()
                .rect_filled(rect, 8.0, faded(palette.chip_fill, hover));
        }
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(12.0),
            color,
        );
    }
    focus_ring(ui, &response, rect, 8.0);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    response
}

/// Карточка ошибки списка с кнопкой повтора. Возвращает клик по Retry.
pub(crate) fn error_card(
    ui: &mut egui::Ui,
    palette: &Palette,
    title: &str,
    detail: &str,
    retry: &str,
) -> bool {
    let mut clicked = false;
    egui::Frame::new()
        .fill(faded(palette.error, 0.08))
        .stroke(Stroke::new(1.0, faded(palette.error, 0.35)))
        .corner_radius(12)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (icon_rect, _) =
                    ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
                icons::paint_icon(
                    ui,
                    icons::Icon::Alert,
                    icon_rect.center(),
                    16.0,
                    palette.error,
                );
                ui.label(
                    egui::RichText::new(title)
                        .size(13.0)
                        .strong()
                        .color(palette.error),
                );
            });
            if !detail.is_empty() {
                ui.add_space(3.0);
                ui.add(
                    egui::Label::new(egui::RichText::new(detail).size(11.0).color(palette.muted))
                        .wrap(),
                );
            }
            ui.add_space(10.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                clicked = action_pill(ui, palette, retry).clicked();
            });
        });
    clicked
}

/// Костяк списка на время поиска: аватары и строки пульсируют одной фазой.
/// `height` — высота вьюпорта; `label` уходит в accessibility вместо текста.
pub(crate) fn skeleton_rows(
    ui: &mut egui::Ui,
    palette: &Palette,
    compact: bool,
    height: f32,
    label: &str,
    reduce: bool,
) {
    let row_height = if compact { 42.0 } else { 58.0 };
    let count = ((height / row_height).floor() as usize).clamp(2, 8);
    let time = ui.input(|input| input.time) as f32;
    for index in 0..count {
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), row_height),
            egui::Sense::hover(),
        );
        if index == 0 {
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, false, label));
        }
        if !ui.is_rect_visible(rect) {
            continue;
        }
        let phase = if reduce {
            0.0
        } else {
            (time * 2.4 + index as f32 * 0.45).sin().abs()
        };
        let bone = faded(palette.text, 0.05 + 0.05 * phase);
        let avatar_radius = if compact { 13.0 } else { 18.0 };
        let avatar_center = pos2(rect.left() + 6.0 + avatar_radius, rect.center().y);
        ui.painter()
            .circle_filled(avatar_center, avatar_radius, bone);
        let text_left = avatar_center.x + avatar_radius + 10.0;
        let name_y = rect.center().y - if compact { 4.0 } else { 7.0 };
        let width = (rect.right() - text_left - 42.0).max(40.0);
        ui.painter().rect_filled(
            egui::Rect::from_min_size(pos2(text_left, name_y - 4.0), Vec2::new(width * 0.55, 8.0)),
            4.0,
            bone,
        );
        ui.painter().rect_filled(
            egui::Rect::from_min_size(pos2(text_left, name_y + 7.0), Vec2::new(width * 0.32, 6.0)),
            3.0,
            bone,
        );
        ui.painter().rect_filled(
            egui::Rect::from_center_size(
                pos2(rect.right() - 16.0, rect.center().y),
                Vec2::splat(9.0),
            ),
            3.0,
            bone,
        );
    }
    if !reduce {
        ui.ctx().request_repaint_after(SKELETON_REPAINT);
    }
}

/// Круглый аватар: favicon или инициал станции.
pub(crate) fn avatar(
    ui: &mut egui::Ui,
    palette: &Palette,
    texture: Option<TextureHandle>,
    name: &str,
    size: f32,
) -> egui::Rect {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return rect;
    }
    let center = rect.center();
    let radius = size * 0.5;
    if let Some(texture) = texture {
        const SEGMENTS: usize = 32;
        let mut mesh = egui::Mesh::with_texture(texture.id());
        mesh.vertices.push(egui::epaint::Vertex {
            pos: center,
            uv: pos2(0.5, 0.5),
            color: Color32::WHITE,
        });
        for segment in 0..=SEGMENTS {
            let angle = segment as f32 * std::f32::consts::TAU / SEGMENTS as f32;
            let point = center + Vec2::new(angle.cos(), angle.sin()) * radius;
            let uv = (point - rect.min) / size;
            mesh.vertices.push(egui::epaint::Vertex {
                pos: point,
                uv: pos2(uv.x, uv.y),
                color: Color32::WHITE,
            });
        }
        for segment in 1..=SEGMENTS as u32 {
            mesh.add_triangle(0, segment, segment + 1);
        }
        ui.painter().add(Shape::mesh(mesh));
    } else {
        ui.painter()
            .circle_filled(center, radius, palette.chip_fill);
        let initial = name
            .trim()
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| "?".to_owned());
        ui.painter().text(
            center,
            egui::Align2::CENTER_CENTER,
            initial,
            egui::FontId::proportional(size * 0.45),
            palette.text,
        );
    }
    ui.painter()
        .circle_stroke(center, radius, Stroke::new(1.0, palette.card_stroke));
    rect
}

/// Флаг-эмодзи по ISO alpha-2; пустая строка при невалидном коде.
pub(crate) fn flag_emoji(iso_a2: &str) -> String {
    let code: Vec<char> = iso_a2
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .collect();
    if code.len() != 2 {
        return String::new();
    }
    code.iter()
        .filter_map(|c| {
            char::from_u32(0x0001_F1E6 + (c.to_ascii_uppercase() as u32 - 'A' as u32))
                .filter(|c| !c.is_ascii())
        })
        .collect()
}

pub(crate) fn contrast_color(color: Color32) -> Color32 {
    let luma =
        0.299 * f32::from(color.r()) + 0.587 * f32::from(color.g()) + 0.114 * f32::from(color.b());
    if luma > 150.0 {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

/// Знак OrbitFM: планета с орбитальным кольцом и точкой-станцией.
pub(crate) fn logo_mark(ui: &mut egui::Ui, palette: &Palette, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    let center = rect.center();
    let radius = size * 0.32;
    painter.circle_stroke(center, radius, Stroke::new(1.6, palette.accent));
    let ring = Vec2::new(radius * 1.25, radius * 0.5);
    let tilt: f32 = -0.5;
    let ring_points: Vec<Pos2> = (0..=48)
        .map(|step| {
            let angle = step as f32 * std::f32::consts::TAU / 48.0;
            let (s, c) = (angle.sin(), angle.cos());
            pos2(
                center.x + ring.x * c * tilt.cos() - ring.y * s * tilt.sin(),
                center.y + ring.x * c * tilt.sin() + ring.y * s * tilt.cos(),
            )
        })
        .collect();
    painter.add(Shape::closed_line(
        ring_points,
        Stroke::new(1.2, palette.chip_stroke),
    ));
    painter.circle_filled(
        center + Vec2::new(ring.x * 0.7, -radius * 0.55),
        2.2,
        palette.live,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::AccentColor;

    #[test]
    fn bucket_max_collapses_source_bars_to_narrow_field() {
        let rising: Vec<f32> = (0..24).map(|i| i as f32 / 24.0).collect();
        let out = bucket_max::<NARROW_VIZ_BARS>(&rising);
        assert_eq!(out.len(), NARROW_VIZ_BARS);
        assert!(
            (out[15] - 23.0 / 24.0).abs() < 1e-6,
            "в последней корзине — верх исходных полос"
        );
        assert!(
            out.windows(2).all(|pair| pair[0] <= pair[1]),
            "монотонность сохраняется"
        );
    }

    #[test]
    fn bucket_max_never_leaves_an_empty_bucket() {
        for len in [1usize, 3, 8, 15, 16, 17, 64] {
            let source = vec![0.5; len];
            let out = bucket_max::<NARROW_VIZ_BARS>(&source);
            assert!(
                out.iter().all(|value| *value == 0.5),
                "пустая корзина при len {len}"
            );
        }
        assert_eq!(bucket_max::<NARROW_VIZ_BARS>(&[]), [0.0; NARROW_VIZ_BARS]);
    }

    #[test]
    fn flag_emoji_maps_iso_to_regional_indicators() {
        assert_eq!(flag_emoji("gb"), "\u{1F1EC}\u{1F1E7}");
        assert_eq!(flag_emoji(" DE "), "\u{1F1E9}\u{1F1EA}");
        assert_eq!(flag_emoji("S"), "");
        assert_eq!(flag_emoji(""), "");
        assert_eq!(flag_emoji("12"), "");
    }

    #[test]
    fn contrast_prefers_dark_text_on_light_accent() {
        assert_eq!(contrast_color(Color32::WHITE), Color32::BLACK);
        assert_eq!(contrast_color(Color32::BLACK), Color32::WHITE);
    }

    /// Один кадр со слайдером: указатель в `pointer`, колесо на `notch` щелчков.
    /// Щелчок приходит в единицах Line — ровно как его даёт winit на Wayland.
    /// Первый вызов без указателя запоминает прямоугольник слайдера в `slot` —
    /// по его центру тесты и наводят курсор.
    fn wheel_frame(
        ctx: &egui::Context,
        palette: &Palette,
        volume: &mut f64,
        notch: f32,
        pointer: Option<Pos2>,
        time: f64,
        slot: &mut Option<egui::Rect>,
    ) -> SliderResponse {
        let mut events = Vec::new();
        if let Some(pos) = pointer {
            events.push(egui::Event::PointerMoved(pos));
        }
        if notch != 0.0 {
            events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: Vec2::new(0.0, notch),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
                modifiers: egui::Modifiers::NONE,
            });
        }
        let input = egui::RawInput {
            time: Some(time),
            predicted_dt: 1.0 / 60.0,
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(400.0, 200.0),
            )),
            events,
            ..Default::default()
        };
        let mut response = SliderResponse::default();
        let output = ctx.run_ui(input, |ui| {
            let origin = ui.cursor().min;
            response = volume_slider_width(ui, palette, volume, 120.0, Language::English);
            if slot.is_none() {
                *slot = Some(egui::Rect::from_min_size(origin, Vec2::new(120.0, 18.0)));
            }
        });
        output.drop_without_applying_deltas();
        response
    }

    /// Пробный кадр без колеса: возвращает центр слайдера для наведения указателя.
    fn slider_center(
        ctx: &egui::Context,
        palette: &Palette,
        volume: &mut f64,
        slot: &mut Option<egui::Rect>,
    ) -> Pos2 {
        wheel_frame(ctx, palette, volume, 0.0, None, 0.1, slot);
        slot.expect("пробный кадр запоминает прямоугольник")
            .center()
    }

    /// Флаги взаимодействия egui 0.37 берутся из прошлого прохода, поэтому перед
    /// колесом нужен «кадр-движение»: указатель уже на месте, колеса нет.
    fn settle_hover(
        ctx: &egui::Context,
        palette: &Palette,
        volume: &mut f64,
        pointer: Pos2,
        slot: &mut Option<egui::Rect>,
    ) {
        wheel_frame(ctx, palette, volume, 0.0, Some(pointer), 0.2, slot);
    }

    #[test]
    fn wheel_over_slider_steps_volume_and_commits() {
        let ctx = egui::Context::default();
        let palette = Palette::for_ui(egui::Theme::Dark, AccentColor::Blue);
        let mut volume = 0.5;
        let mut slot = None;
        let center = slider_center(&ctx, &palette, &mut volume, &mut slot);
        settle_hover(&ctx, &palette, &mut volume, center, &mut slot);
        let hover = Some(center);

        let answer = wheel_frame(&ctx, &palette, &mut volume, 1.0, hover, 1.0, &mut slot);
        assert!(
            answer.changed && answer.committed,
            "колесо коммитит сразу, как стрелки"
        );
        assert!(
            (volume - 0.55).abs() < 1e-9,
            "щелчок вверх: +5 %, получено {volume}"
        );

        let answer = wheel_frame(&ctx, &palette, &mut volume, -2.0, hover, 2.0, &mut slot);
        assert!(answer.changed && answer.committed);
        assert!(
            (volume - 0.45).abs() < 1e-9,
            "два щелчка вниз: −10 %, получено {volume}"
        );
    }

    #[test]
    fn wheel_is_deduplicated_within_one_input_time() {
        let ctx = egui::Context::default();
        let palette = Palette::for_ui(egui::Theme::Dark, AccentColor::Blue);
        let mut volume = 0.5;
        let mut slot = None;
        let center = slider_center(&ctx, &palette, &mut volume, &mut slot);
        settle_hover(&ctx, &palette, &mut volume, center, &mut slot);
        // Многопроходность egui: тот же `time` не должен применять дельту дважды.
        wheel_frame(
            &ctx,
            &palette,
            &mut volume,
            1.0,
            Some(center),
            1.0,
            &mut slot,
        );
        let after_first = volume;
        let answer = wheel_frame(
            &ctx,
            &palette,
            &mut volume,
            1.0,
            Some(center),
            1.0,
            &mut slot,
        );
        assert_eq!(
            volume, after_first,
            "повтор тех же входных данных — без шага"
        );
        assert!(!answer.changed && !answer.committed);
    }

    #[test]
    fn wheel_outside_slider_leaves_volume_and_scroll_alone() {
        let ctx = egui::Context::default();
        let palette = Palette::for_ui(egui::Theme::Dark, AccentColor::Blue);
        let mut volume = 0.5;
        let mut slot = None;
        slider_center(&ctx, &palette, &mut volume, &mut slot);
        let far = pos2(350.0, 180.0);
        settle_hover(&ctx, &palette, &mut volume, far, &mut slot);
        let answer = wheel_frame(&ctx, &palette, &mut volume, 1.0, Some(far), 1.0, &mut slot);
        assert!(
            !answer.changed && !answer.committed,
            "указатель мимо — дельта не наша"
        );
        assert_eq!(volume, 0.5);
    }

    #[test]
    fn wheel_clamps_at_volume_edges_without_commit() {
        let ctx = egui::Context::default();
        let palette = Palette::for_ui(egui::Theme::Dark, AccentColor::Blue);
        let mut volume = 1.0;
        let mut slot = None;
        let center = slider_center(&ctx, &palette, &mut volume, &mut slot);
        settle_hover(&ctx, &palette, &mut volume, center, &mut slot);
        let answer = wheel_frame(
            &ctx,
            &palette,
            &mut volume,
            1.0,
            Some(center),
            1.0,
            &mut slot,
        );
        assert_eq!(volume, 1.0, "выше максимума не прыгает");
        assert!(
            !answer.committed,
            "на границе записывать в настройки нечего"
        );
    }

    #[test]
    fn faded_scales_alpha_and_clamps() {
        let base = Color32::from_rgb(0x12, 0x24, 0x36);
        let half = faded(base, 0.5);
        assert_eq!(half.a(), 128);
        let [r, g, b, _] = half.to_srgba_unmultiplied();
        assert_eq!(
            (r, g, b),
            (0x12, 0x24, 0x36),
            "фэйд не обесцвечивает каналы"
        );
        assert_eq!(faded(base, -1.0).a(), 0);
        assert_eq!(faded(base, 4.0), base);

        let translucent = Color32::from_rgba_unmultiplied(0x12, 0x24, 0x36, 200);
        assert_eq!(faded(translucent, 0.5).a(), 100);
        assert_eq!(faded(translucent, 1.0).a(), 200);
    }

    #[test]
    fn glass_fill_spans_the_rounded_contour() {
        let rect = egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 60.0));
        let rounded = RoundedRect::new(rect, 16.0);
        let top = Color32::from_rgb(60, 70, 90);
        let bottom = Color32::from_rgb(10, 15, 25);
        let mesh = gradient_fill(rounded, top, bottom);

        assert!(!mesh.is_empty(), "заливка панели не должна быть пустой");
        assert_eq!(
            mesh.indices.len(),
            3 * (mesh.vertices.len() - 1),
            "заливка — веер треугольников из центра"
        );
        for vertex in &mesh.vertices {
            assert!(
                rect.expand(0.5).contains(vertex.pos),
                "{:?} вышел за панель",
                vertex.pos
            );
        }
        let highest = mesh
            .vertices
            .iter()
            .min_by(|a, b| a.pos.y.total_cmp(&b.pos.y))
            .expect("вершины есть");
        let lowest = mesh
            .vertices
            .iter()
            .max_by(|a, b| a.pos.y.total_cmp(&b.pos.y))
            .expect("вершины есть");
        assert!((highest.pos.y - rect.top()).abs() < 0.5, "верх панели");
        assert!((lowest.pos.y - rect.bottom()).abs() < 0.5, "низ панели");
        assert_eq!(highest.color, top, "верхняя кромка — glass_top");
        assert_eq!(lowest.color, bottom, "нижняя кромка — glass_bottom");
    }

    #[test]
    fn top_rim_stays_on_the_upper_edge() {
        let rect = egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 60.0));
        let rim = top_rim(RoundedRect::new(rect, 16.0));

        assert!(rim.len() > 4, "блик идёт по кромке, а не точкой");
        for point in &rim {
            assert!(
                point.y <= rect.top() + 16.5,
                "блик ушёл ниже закругления: {point:?}"
            );
            assert!(rect.expand(0.5).contains(*point));
        }
        assert!(rim.first().expect("есть точки").x <= rect.left() + 16.5);
        assert!(rim.last().expect("есть точки").x >= rect.right() - 16.5);
    }

    /// Палитра для тестов: важен альфа-канал, а не конкретный оттенок.
    fn test_palette() -> Palette {
        Palette::for_ui(egui::Theme::Dark, AccentColor::Blue)
    }

    /// `Color32` хранит каналы уже умноженными на альфу, поэтому round-trip
    /// через unmultiplied даёт погрешность в один квант.
    fn same_hue(first: Color32, second: Color32) -> bool {
        let [r0, g0, b0, _] = first.to_srgba_unmultiplied();
        let [r1, g1, b1, _] = second.to_srgba_unmultiplied();
        [(r0, r1), (g0, g1), (b0, b1)]
            .iter()
            .all(|(from, to)| (*from as i16 - *to as i16).abs() <= 1)
    }

    #[test]
    fn faded_palette_dims_every_channel_and_keeps_hue() {
        let palette = test_palette();
        let dim = faded_palette(&palette, 0.5);
        assert_eq!(dim.text, faded(palette.text, 0.5));
        assert_eq!(dim.accent, faded(palette.accent, 0.5));
        assert_eq!(dim.glass_top, faded(palette.glass_top, 0.5));
        assert!(
            same_hue(palette.accent, dim.accent),
            "фэйд не обесцвечивает палитру"
        );

        assert_eq!(faded_palette(&palette, 1.0).chip_fill, palette.chip_fill);
        assert_eq!(faded_palette(&palette, 0.0).text.a(), 0);
        assert_eq!(faded_palette(&palette, f32::NAN).text.a(), 0);
    }

    #[test]
    fn pop_scale_flies_past_one_but_not_past_the_ceiling() {
        assert_eq!(pop_scale(0.0, false), 0.0);
        assert_eq!(pop_scale(1.0, false), 1.0);
        let peak = (0..=100u8)
            .map(|step| pop_scale(f32::from(step) / 100.0, false))
            .fold(0.0f32, f32::max);
        assert!(
            peak > 1.0 && peak <= POP_LIMIT,
            "перелёт слишком резкий: {peak}"
        );
    }

    #[test]
    fn reduce_motion_freezes_scale_at_the_final_size() {
        for step in 0..=100u8 {
            assert_eq!(pop_scale(f32::from(step) / 100.0, true), 1.0);
        }
    }

    #[test]
    fn pressed_scale_sits_between_rest_and_pinch() {
        assert_eq!(pressed_scale(0.0), 1.0);
        assert!(
            (pressed_scale(1.0) - animation::PRESS_SCALE).abs() < 1e-6,
            "удержание даёт полный press-размер"
        );
        let middle = pressed_scale(0.5);
        assert!(middle < 1.0 && middle > animation::PRESS_SCALE);
    }

    #[test]
    fn lift_brightens_without_piercing_the_entrance_fade() {
        let dim = faded(Color32::from_rgb(0x3B, 0x82, 0xF6), 0.5);
        let lifted = lift(dim, HOVER_LIFT);
        assert_eq!(lifted.a(), dim.a(), "hover не должен поднимать альфу");
        let [r0, g0, b0, _] = dim.to_srgba_unmultiplied();
        let [r, g, b, _] = lifted.to_srgba_unmultiplied();
        let before = (u16::from(r0), u16::from(g0), u16::from(b0));
        let after = (u16::from(r), u16::from(g), u16::from(b));
        assert!(
            r > r0 && g > g0 && b > b0,
            "hover светлее покоя: {after:?} против {before:?}"
        );
        assert_eq!(lift(Color32::WHITE, 1.0), Color32::WHITE);
    }
}
