//! Кастомные контролы «Orbital Desk» по референс-макету.
//!
//! Все виджеты рисуются painter'ом и не хранят состояние — только запросы
//! (`Response`/`bool`). Фасад разделён по семействам: `shared` (цвет, движение,
//! меши), `chrome` (стекло, pill, чипы, вкладки), `inputs` (поиск, громкость),
//! `media` (LIVE, эквалайзер, визуализатор), `buttons` (диски, капсулы,
//! действия), `content` (заглушки, ошибки, аватар, знак).

mod buttons;
mod chrome;
mod content;
mod inputs;
mod media;
mod prelude;
mod shared;

pub(crate) use buttons::{
    CapsuleDisc, action_pill, capsule_button, circle_button, circle_button_sized, quiet_button,
};
pub(crate) use chrome::{chip, glass_panel, pill, segmented_tabs};
pub(crate) use content::{
    EmptyMark, EmptyState, avatar, contrast_color, empty_state, error_card, flag_emoji, logo_mark,
    skeleton_rows,
};
pub(crate) use inputs::{search_field, volume_slider_width};
pub(crate) use media::{
    NARROW_VIZ_BARS, NARROW_VIZ_SIZE, bucket_max, eq_bars_in, live_badge, viz_bars,
};
pub(crate) use shared::{control_motion, faded, faded_group, faded_palette, focus_ring, pop_scale};

#[cfg(test)]
mod tests {
    use super::buttons::{lift, pressed_scale};
    use super::inputs::SliderResponse;
    use super::prelude::*;
    use super::shared::{HOVER_LIFT, POP_LIMIT, gradient_fill, top_rim};
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
