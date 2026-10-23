//! Кастомные контролы «Orbital Desk» по референс-макету.
//!
//! Все виджеты рисуются painter'ом и не хранят состояние — только запросы
//! (`Response`/`bool`).

use eframe::egui::{self, Color32, Pos2, Shape, Stroke, TextureHandle, Vec2, pos2};

use crate::render::bar_color;

use super::icons;
use super::palette::{Palette, mix_colors};

const PILL_RADIUS: f32 = 999.0;
const LIVE_REPAINT: std::time::Duration = std::time::Duration::from_millis(120);
/// Полос вертикального градиента в стекле (ТЗ §3.1).
const GLASS_BANDS: usize = 8;
/// Толщина верхнего блика стекла.
const GLASS_RIM: f32 = 1.5;
/// Горизонтальные пояса градиента заливки громкости (ТЗ §3.4).
const VOLUME_FILL_BANDS: usize = 6;

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

/// Насколько скругление угла «съедает» полосу градиента по горизонтали:
/// `0` на прямом участке, `radius` у самой кромки.
fn corner_inset(rect: egui::Rect, radius: f32, top: f32, bottom: f32) -> f32 {
    let radius = radius
        .min(rect.width() / 2.0)
        .min(rect.height() / 2.0)
        .max(0.0);
    if radius <= 0.0 {
        return 0.0;
    }
    let depth = (top - rect.top())
        .min(rect.bottom() - bottom)
        .clamp(0.0, radius);
    let inside = radius - depth;
    radius - (radius * radius - inside * inside).max(0.0).sqrt()
}

/// Стеклянная панель: вертикальный градиент `glass_top → glass_bottom`,
/// верхний блик 1–2 px и мягкая рамка. `egui::Frame` умеет только сплошную
/// заливку, поэтому фон рисуется отдельными полосами до содержимого зоны.
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
    let band_height = rect.height() / GLASS_BANDS as f32;
    for band in 0..GLASS_BANDS {
        let top = rect.top() + band_height * band as f32;
        let bottom = if band + 1 == GLASS_BANDS {
            rect.bottom()
        } else {
            top + band_height
        };
        let inset = corner_inset(rect, radius, top, bottom);
        let color = mix_colors(
            palette.glass_top,
            palette.glass_bottom,
            (band as f32 + 0.5) / GLASS_BANDS as f32,
        );
        painter.rect_filled(
            egui::Rect::from_min_max(
                pos2(rect.left() + inset, top),
                pos2(rect.right() - inset, bottom),
            ),
            0.0,
            faded(color, alpha),
        );
    }
    let rim_bottom = (rect.top() + GLASS_RIM).min(rect.bottom());
    let inset = corner_inset(rect, radius, rect.top(), rim_bottom);
    painter.rect_filled(
        egui::Rect::from_min_max(
            pos2(rect.left() + inset, rect.top()),
            pos2(rect.right() - inset, rim_bottom),
        ),
        0.0,
        faded(palette.rim_light, alpha),
    );
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
    if ui.is_rect_visible(rect) {
        let fill = if selected {
            palette.accent
        } else if response.hovered() {
            palette.chip_fill
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, PILL_RADIUS, fill);
        ui.painter()
            .galley(rect.center() - galley.size() * 0.5, galley, text_color);
    }
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
    if ui.is_rect_visible(rect) {
        let (fill, stroke) = if selected {
            (palette.accent, Stroke::NONE)
        } else {
            (palette.chip_fill, Stroke::new(1.0, palette.chip_stroke))
        };
        ui.painter()
            .rect(rect, PILL_RADIUS, fill, stroke, egui::StrokeKind::Middle);
        ui.painter()
            .galley(rect.center() - galley.size() * 0.5, galley, text_color);
    }
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
    icons::draw_search(
        ui.painter(),
        pos2(rect.left() + 16.0, rect.center().y),
        5.0,
        palette.muted,
    );
    let inner = egui::Rect::from_min_max(
        pos2(rect.left() + 28.0, rect.top()),
        pos2(rect.right() - 10.0, rect.bottom()),
    );
    let edit = egui::TextEdit::singleline(text)
        .hint_text(hint)
        .desired_width(inner.width() - 8.0)
        .align(egui::Align2::LEFT_CENTER)
        .id(id);
    let response = ui.put(inner, edit);
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
pub(crate) fn live_badge(ui: &mut egui::Ui, palette: &Palette, active: bool) {
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
        "LIVE",
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

/// Число горизонтальных срезов на столбец: egui даёт градиент только штриху,
/// поэтому столбец собирается из полосок с интерполяцией цвета.
const VIZ_SLICES: usize = 7;

/// Столбцы синтетического визуализатора: низ зелёный, верх красный.
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
    let width = (step * 0.6).max(1.5).min(step);
    for (index, level) in levels.iter().enumerate() {
        let x0 = rect.left() + step * index as f32 + (step - width) * 0.5;
        let level = level.clamp(0.0, 1.0);
        let bar_top = (rect.height() * level).max(1.5);
        let floor = rect.bottom() - bar_top;
        // Пояса градиента привязаны к высоте области, а не к столбцу (ТЗ §3.3):
        // короткий столбец остаётся зелёным, жёлтый и красный — только на подъёме.
        for slice in 0..VIZ_SLICES {
            let from = slice as f32 / VIZ_SLICES as f32;
            let to = ((slice + 1) as f32 / VIZ_SLICES as f32).min(level);
            if to <= from {
                break;
            }
            let y0 = (rect.bottom() - rect.height() * from).max(floor);
            let y1 = (rect.bottom() - rect.height() * to).max(floor);
            painter.rect_filled(
                egui::Rect::from_min_max(pos2(x0, y1), pos2(x0 + width, y0)),
                0.0,
                bar_color((from + to) * 0.5),
            );
        }
        if let Some(peak) = peaks.get(index) {
            let y = rect.bottom() - (rect.height() * peak.clamp(0.0, 1.0)).max(1.5);
            painter.rect_filled(
                egui::Rect::from_min_max(pos2(x0, y - 1.5), pos2(x0 + width, y)),
                0.0,
                palette.muted.linear_multiply(0.7),
            );
        }
    }
}

/// Громкость: динамик + тонкий accent-трек с круглой ручкой.
pub(crate) fn volume_slider(ui: &mut egui::Ui, palette: &Palette, volume: &mut f64) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(126.0, 18.0), egui::Sense::click_and_drag());
    // Иконка динамика внутри выделенного прямоугольника — иначе она наползает на
    // соседний чип.
    icons::draw_speaker(
        ui.painter(),
        pos2(rect.left() + 7.0, rect.center().y),
        7.0,
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
        return *volume != previous;
    }
    false
}

/// Большая круглая play/pause с accent-кольцом.
pub(crate) fn circle_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    playing: bool,
    enabled: bool,
    hint: &str,
) -> egui::Response {
    let size = 40.0;
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), sense);
    if ui.is_rect_visible(rect) {
        let center = rect.center();
        // ТЗ §3.4: primary play — сплошной accent, чуть светлее при наведении.
        let hoverable = response.hovered() && enabled;
        let fill = if !enabled {
            palette.chip_fill
        } else if hoverable {
            super::palette::mix_colors(palette.accent, Color32::WHITE, 0.18)
        } else {
            palette.accent
        };
        let ring = if enabled {
            super::palette::mix_colors(palette.accent, Color32::WHITE, 0.45)
        } else {
            palette.muted
        };
        let radius = size * 0.5 - 1.0;
        ui.painter().circle_filled(center, radius, fill);
        ui.painter()
            .circle_stroke(center, radius, Stroke::new(1.6, ring));
        let glyph = if enabled {
            contrast_color(fill)
        } else {
            palette.muted
        };
        if playing {
            for dx in [-4.0, 4.0] {
                ui.painter().rect_filled(
                    egui::Rect::from_center_size(center + Vec2::new(dx, 0.0), Vec2::new(3.2, 12.0)),
                    1.5,
                    glyph,
                );
            }
        } else {
            ui.painter().add(Shape::convex_polygon(
                vec![
                    pos2(center.x - 4.5, center.y - 7.0),
                    pos2(center.x - 4.5, center.y + 7.0),
                    pos2(center.x + 7.0, center.y),
                ],
                glyph,
                Stroke::NONE,
            ));
        }
    }
    if enabled {
        response.on_hover_text(hint)
    } else {
        response
    }
}

/// Вкладки с иконкой и accent-подчёркиванием активной.
pub(crate) enum TabIcon {
    Results,
    Star,
    Clock,
}

pub(crate) struct Tab<'a> {
    pub icon: TabIcon,
    pub label: &'a str,
}

struct TabButton<'a> {
    tab: &'a Tab<'a>,
    palette: &'a Palette,
    active: bool,
}

impl egui::Widget for TabButton<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let color = if self.active {
            self.palette.accent
        } else {
            self.palette.muted
        };
        let galley = ui.painter().layout_no_wrap(
            self.tab.label.to_owned(),
            egui::FontId::proportional(12.5),
            color,
        );
        let size = Vec2::new(20.0 + galley.size().x, 30.0);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
        if ui.is_rect_visible(rect) {
            let icon_center = pos2(rect.left() + 7.0, rect.center().y - 4.0);
            match self.tab.icon {
                TabIcon::Results => icons::draw_list_icon(
                    ui.painter(),
                    egui::Rect::from_center_size(icon_center, Vec2::splat(12.0)),
                    color,
                ),
                TabIcon::Star => {
                    let points = star_points(icon_center, 7.0, 3.0);
                    ui.painter()
                        .add(Shape::closed_line(points, Stroke::new(1.2, color)));
                }
                TabIcon::Clock => icons::draw_clock(ui.painter(), icon_center, 6.5, color),
            }
            ui.painter().galley(
                pos2(
                    icon_center.x + 10.0,
                    rect.center().y - 4.0 - galley.size().y * 0.5,
                ),
                galley,
                color,
            );
            if self.active {
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(
                        pos2(rect.left(), rect.bottom() - 2.0),
                        pos2(rect.right(), rect.bottom()),
                    ),
                    1.0,
                    self.palette.accent,
                );
            }
        }
        response
    }
}

/// Возвращает индекс нажатой вкладки.
pub(crate) fn tab_bar(
    ui: &mut egui::Ui,
    palette: &Palette,
    tabs: &[Tab<'_>],
    selected: usize,
) -> Option<usize> {
    let mut clicked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 16.0;
        for (index, tab) in tabs.iter().enumerate() {
            if ui
                .add(TabButton {
                    tab,
                    palette,
                    active: index == selected,
                })
                .clicked()
            {
                clicked = Some(index);
            }
        }
    });
    clicked
}

fn star_points(center: Pos2, outer: f32, inner: f32) -> Vec<Pos2> {
    (0..10)
        .map(|index| {
            let radius = if index % 2 == 0 { outer } else { inner };
            let angle = -std::f32::consts::FRAC_PI_2 + index as f32 * std::f32::consts::PI / 5.0;
            pos2(
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin(),
            )
        })
        .collect()
}

/// Круглый аватар: favicon или инициал станции.
pub(crate) fn avatar(
    ui: &mut egui::Ui,
    palette: &Palette,
    texture: Option<TextureHandle>,
    name: &str,
    size: f32,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
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
    fn corner_inset_follows_the_rounded_edge() {
        let rect = egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 60.0));
        assert!(
            (corner_inset(rect, 16.0, 0.0, 2.0) - 16.0).abs() < 0.5,
            "на самом краю полоса должна вжаться на весь радиус"
        );
        assert_eq!(
            corner_inset(rect, 16.0, 30.0, 32.0),
            0.0,
            "в середине — без вжатия"
        );
        // Радиус больше половины высоты не должен «съедать» панель целиком.
        assert!(corner_inset(rect, 999.0, 30.0, 32.0) <= 30.0);
    }
}
