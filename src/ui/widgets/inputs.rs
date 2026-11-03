//! Поля ввода: поиск и слайдер громкости.

use super::prelude::*;
use super::shared::*;

/// Горизонтальные пояса градиента заливки громкости (ТЗ §3.4).
const VOLUME_FILL_BANDS: usize = 6;
/// Шаг громкости стрелками клавиатуры.
const VOLUME_KEY_STEP: f64 = 0.05;

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
    // `ScrollArea` окна настроек, а повтор тех же входных данных в многопроходной
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
