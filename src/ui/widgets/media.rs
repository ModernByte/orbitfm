//! Медиа-индикаторы: LIVE-бейдж, эквалайзер и визуализатор.

use super::prelude::*;
use super::shared::*;

const LIVE_REPAINT: std::time::Duration = std::time::Duration::from_millis(120);

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
