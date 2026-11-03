//! Общие примитивы: цвет, движение, кольца фокуса и меши стекла.

use super::prelude::*;

pub(super) const PILL_RADIUS: f32 = 999.0;

/// Осветление диска play/stop при наведении (ТЗ §3.4).
pub(super) const HOVER_LIFT: f32 = 0.18;
/// Осветление обода активного диска.
pub(super) const RING_LIFT: f32 = 0.45;
/// Потолок overshoot-шкалы: появление капсульной кнопки не может превышать
/// конечный размер более чем на 15 % (ADR-0032 §2).
pub(super) const POP_LIMIT: f32 = 1.15;

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
pub(super) fn rounded_contour(rounded: RoundedRect) -> Vec<Pos2> {
    let mut points = Vec::new();
    tessellator::path::rounded_rectangle(&mut points, rounded);
    points
}

/// Заливка стекла одним мешем: цвет вершины зависит только от `y`, а
/// растеризатор интерполирует линейную функцию без искажений. В отличие от
/// полос, швов между «слоями» градиента нет, а скругления задаются точным
/// контуром, а не прямоугольным шагом.
pub(super) fn gradient_fill(rounded: RoundedRect, top: Color32, bottom: Color32) -> egui::Mesh {
    let rect = rounded.rect();
    gradient_fill_with(rounded, |y| {
        mix_colors(top, bottom, (y - rect.top()) / rect.height().max(1.0))
    })
}

/// Тот же веер, но цвет каждой вершины задаёт вызывающий: так столбцы
/// визуализатора получают градиент по абсолютной высоте поля.
pub(super) fn gradient_fill_with(
    rounded: RoundedRect,
    color_at: impl Fn(f32) -> Color32,
) -> egui::Mesh {
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
pub(super) fn top_rim(rounded: RoundedRect) -> Vec<Pos2> {
    let rect = rounded.rect();
    let radius = rounded.corner_radius().nw.max(0.0);
    rounded_contour(rounded)
        .into_iter()
        .filter(|point| point.y <= rect.top() + radius + 0.25)
        .collect()
}
