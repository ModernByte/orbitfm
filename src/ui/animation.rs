//! Единый ритм переходов интерфейса (ADR-0032).
//!
//! Модуль знает только про egui и флаг reduce motion из настроек: раскладка,
//! hit-testing, аудио-сцена и состояние session ему не принадлежат.

use eframe::egui;

/// Drawer настроек.
pub(crate) const DRAWER: f32 = 0.200;
/// Затемнение под drawer'ом.
pub(crate) const SCRIM: f32 = 0.160;
/// Вход HUD, панели, help и transport в Focus.
pub(crate) const SCENE_ENTER: f32 = 0.220;
/// Появление стеклянной панели.
pub(crate) const PANEL: f32 = 0.220;
pub(crate) const HELP: f32 = 0.170;
pub(crate) const LEGEND: f32 = 0.150;
/// Callout страны — совпадает с [`crate::ui::globe_view`] для совместимости
/// собственного fade состояния.
pub(crate) const CALLOUT: f32 = 0.180;
/// Смена вкладки Страна/Жанр.
pub(crate) const TAB: f32 = 0.120;
pub(crate) const TOAST_IN: f32 = 0.180;
pub(crate) const TOAST_OUT: f32 = 0.200;
/// Вход капсул Overlay и Listening.
pub(crate) const CAPSULE: f32 = 0.180;
/// Hover строки списка.
pub(crate) const ROW_HOVER: f32 = 0.080;
/// Press-сжатие капсульной кнопки.
pub(crate) const PRESS: f32 = 0.120;
/// Смещение стеклянной панели по горизонтали.
pub(crate) const SLIDE_X: f32 = 8.0;
/// Подъём help, legend и callout по вертикали.
pub(crate) const RISE_Y: f32 = 6.0;
/// Коэффициент press-шкалы.
pub(crate) const PRESS_SCALE: f32 = 0.96;
/// Шаг stagger строк: 18 мс на каждую следующую.
const STAGGER_STEP_MS: f32 = 18.0;
/// Шаг stagger в долях окна входа сцены.
const STAGGER_WINDOW: f32 = SCENE_ENTER * 1_000.0;
/// Задержки ближе этого края окна перестают сдвигаться, чтобы последние строки
/// не оставались невидимыми.
const STAGGER_MAX_DELAY: f32 = 0.9;
/// Вершина overshoot: 10 % сверх единицы.
const OVERSHOOT: f32 = 1.701_58;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Curve {
    /// Быстрый старт, мягкий приход — появление.
    EaseOut,
    /// Мягкий старт, быстрый уход — исчезновение.
    EaseIn,
    /// Ease-out с лёгким перелётом за 1.0; только для капсульных кнопок.
    Overshoot,
}

/// Нормализованный прогресс `t` через кривую. Вход клампится, концы точные.
/// [`Curve::Overshoot`] в середине переходит за 1.0 — это намеренно.
pub(crate) fn eased(t: f32, curve: Curve) -> f32 {
    let t = if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    };
    match curve {
        Curve::EaseOut => 1.0 - (1.0 - t).powi(3),
        Curve::EaseIn => t * t * t,
        Curve::Overshoot => {
            if t == 0.0 || t == 1.0 {
                t
            } else {
                let back = t - 1.0;
                1.0 + (OVERSHOOT + 1.0) * back.powi(3) + OVERSHOOT * back * back
            }
        }
    }
}

/// Прогресс одной строки в общем прогрессе входа: ноль для первой,
/// +18 мс на каждую следующую. Результат всегда в `0..=1`.
pub(crate) fn stagger(index: usize, progress: f32) -> f32 {
    let progress = if progress.is_finite() {
        progress.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let delay = ((index as f32) * STAGGER_STEP_MS / STAGGER_WINDOW).min(STAGGER_MAX_DELAY);
    if progress <= delay {
        return 0.0;
    }
    ((progress - delay) / (1.0 - delay)).clamp(0.0, 1.0)
}

/// Стабильный переход по `Id`: значение `0..1` для отрисовки и прозрачности.
///
/// Живёт в состоянии окна: `Transitions` создаётся вместе с ним, поэтому при
/// пересоздании viewport (смена `WindowMode`, ADR-0021) вход проигрывается заново
/// без отдельного сброса. Кадры запрашиваются только на время перехода: egui сам
/// перерисовывает кадр, пока значение между 0 и 1.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Presence {
    id: egui::Id,
    seen: bool,
}

impl Presence {
    pub(crate) fn new(id: egui::Id) -> Self {
        Self { id, seen: false }
    }

    /// Прогресс перехода к состоянию `open`. `duration` — секунды,
    /// `reduce` — мастер-выключатель [`crate::settings::Settings::reduce_motion`].
    pub(crate) fn value(
        &mut self,
        ctx: &egui::Context,
        open: bool,
        duration: f32,
        reduce: bool,
    ) -> f32 {
        if reduce || !duration.is_finite() || duration <= 0.0 {
            self.seen = true;
            return if open { 1.0 } else { 0.0 };
        }

        if !self.seen {
            // Первый кадр сидирует закрытое состояние, иначе egui отдаёт
            // конечное значение сразу и вход не проигрывается.
            self.seen = true;
            ctx.animate_bool_with_time(self.id, false, duration);
            if open {
                ctx.request_repaint();
            }
            return 0.0;
        }

        let progress = ctx.animate_bool_with_time(self.id, open, duration);
        eased(progress, if open { Curve::EaseOut } else { Curve::EaseIn })
    }
}

/// Смещение панели по X: `SLIDE_X` пикселей в закрытом состоянии, ноль в открытом.
pub(crate) fn slide_x(progress: f32) -> f32 {
    SLIDE_X * (1.0 - progress)
}

/// Подъём help/legend/callout: `RISE_Y` пикселей снизу в закрытом состоянии.
pub(crate) fn rise_y(progress: f32) -> f32 {
    RISE_Y * (1.0 - progress)
}

/// Переходы одного прогона окна. `OrbitApp` создаётся заново при пересоздании
/// viewport (ADR-0018, ADR-0021), поэтому вход проигрывается на каждом новом
/// окне без ручного `reset`.
#[derive(Debug)]
pub(crate) struct Transitions {
    /// Вход HUD и transport: один stagger на прогон окна.
    pub scene: Presence,
    /// Стеклянная панель списка станций.
    pub panel: Presence,
    pub help: Presence,
    pub legend: Presence,
    /// Drawer настроек и его scrim.
    pub drawer: Presence,
    pub scrim: Presence,
    /// Капсулы Overlay, избранное в них и строка Listening.
    pub capsules: Presence,
    pub favorites: Presence,
    pub listening: Presence,
    /// Переходы с динамическими ключами: вкладки, тосты.
    pub keyed: PresenceStore,
}

impl Transitions {
    pub(crate) fn new() -> Self {
        let presence = |name: &'static str| Presence::new(egui::Id::unique(name));
        Self {
            scene: presence("orbitfm_transition_scene"),
            panel: presence("orbitfm_transition_panel"),
            help: presence("orbitfm_transition_help"),
            legend: presence("orbitfm_transition_legend"),
            drawer: presence("orbitfm_transition_drawer"),
            scrim: presence("orbitfm_transition_scrim"),
            capsules: presence("orbitfm_transition_capsules"),
            favorites: presence("orbitfm_transition_favorites"),
            listening: presence("orbitfm_transition_listening"),
            keyed: PresenceStore::default(),
        }
    }

    /// Считывает прогрессы кадра одним вызовом: зоны только читают значения.
    /// Закрытые зоны тоже опрашиваются — иначе исчезновение не анимируется.
    pub(crate) fn frame(
        &mut self,
        ctx: &egui::Context,
        flags: ZoneFlags,
        reduce: bool,
    ) -> FrameMotion<'_> {
        let Transitions {
            scene,
            panel,
            help,
            legend,
            drawer,
            scrim,
            capsules,
            favorites,
            listening,
            keyed,
        } = self;
        FrameMotion {
            reduce,
            scene: scene.value(ctx, flags.scene, SCENE_ENTER, reduce),
            panel: panel.value(ctx, flags.panel, PANEL, reduce),
            help: help.value(ctx, flags.help, HELP, reduce),
            legend: legend.value(ctx, flags.legend, LEGEND, reduce),
            drawer: drawer.value(ctx, flags.drawer, DRAWER, reduce),
            scrim: scrim.value(ctx, flags.drawer, SCRIM, reduce),
            capsules: capsules.value(ctx, flags.capsules, CAPSULE, reduce),
            favorites: favorites.value(ctx, flags.favorites, CAPSULE, reduce),
            listening: listening.value(ctx, flags.listening, CAPSULE, reduce),
            keyed,
        }
    }
}

/// Видимость зон на кадр — из неё [`Transitions::frame`] считает прогрессы.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ZoneFlags {
    /// Focus-стол виден: вход HUD и transport одним stagger.
    pub scene: bool,
    pub panel: bool,
    pub help: bool,
    pub legend: bool,
    /// Drawer настроек открыт (scrim делит с ним флаг).
    pub drawer: bool,
    pub capsules: bool,
    pub favorites: bool,
    pub listening: bool,
}

/// Готовые прогрессы одного кадра плюс хранилище именованных переходов.
#[derive(Debug)]
pub(crate) struct FrameMotion<'a> {
    /// Мастер-выключатель движения: все прогрессы уже конечные, а зоны
    /// дополнительно гасят собственные циклы (пульс Listening).
    pub reduce: bool,
    pub scene: f32,
    pub panel: f32,
    pub help: f32,
    pub legend: f32,
    pub drawer: f32,
    pub scrim: f32,
    pub capsules: f32,
    pub favorites: f32,
    pub listening: f32,
    /// Вкладки, тосты и прочие небольшие именованные наборы.
    pub keyed: &'a mut PresenceStore,
}

/// Прозрачность по прогрессу: невидимая зона не должна рисовать каркас.
pub(crate) fn alpha(progress: f32) -> f32 {
    if progress.is_finite() {
        progress.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Набор [`Presence`] под небольшие именованные наборы (вкладки, тосты).
///
/// Ключей должно быть мало: запись хранится до вытеснения, а длинный список
/// строк анимируется общим прогрессом через [`stagger`], а не по записи на строку.
#[derive(Debug, Default)]
pub(crate) struct PresenceStore {
    entries: Vec<(egui::Id, Presence)>,
}

/// Потолок именованных переходов: сверх него самые старые вытесняются.
/// Запас держим по числу живых наборов: вкладки панели, press и цвет play/stop
/// в капсулах. Вытеснение не ломает анимацию, но перезапускает вход ключа.
const KEYED_LIMIT: usize = 12;

impl PresenceStore {
    pub(crate) fn value(
        &mut self,
        ctx: &egui::Context,
        id: egui::Id,
        open: bool,
        duration: f32,
        reduce: bool,
    ) -> f32 {
        if let Some((_, presence)) = self.entries.iter_mut().find(|(key, _)| *key == id) {
            return presence.value(ctx, open, duration, reduce);
        }
        if self.entries.len() == KEYED_LIMIT {
            self.entries.remove(0);
        }
        self.entries.push((id, Presence::new(id)));
        self.entries
            .last_mut()
            .expect("запись только что добавлена")
            .1
            .value(ctx, open, duration, reduce)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{self, AtomicUsize};

    #[test]
    fn curves_are_exact_at_the_ends() {
        for curve in [Curve::EaseOut, Curve::EaseIn, Curve::Overshoot] {
            assert_eq!(eased(0.0, curve), 0.0);
            assert_eq!(eased(1.0, curve), 1.0);
            assert_eq!(eased(-1.0, curve), 0.0);
            assert_eq!(eased(3.0, curve), 1.0);
            assert_eq!(eased(f32::NAN, curve), 0.0);
        }
    }

    #[test]
    fn ease_out_leads_and_ease_in_follows() {
        assert!(eased(0.5, Curve::EaseOut) > 0.5);
        assert!(eased(0.5, Curve::EaseIn) < 0.5);
    }

    #[test]
    fn cubic_curves_are_monotonic() {
        for curve in [Curve::EaseOut, Curve::EaseIn] {
            let mut previous = -1.0;
            for step in 0..=100u8 {
                let value = eased(f32::from(step) / 100.0, curve);
                assert!(value >= previous, "{curve:?} не монотонна на {step}");
                previous = value;
            }
        }
    }

    #[test]
    fn overshoot_flies_past_one_and_lands() {
        let peak = (0..=100u8)
            .map(|step| eased(f32::from(step) / 100.0, Curve::Overshoot))
            .fold(f32::MIN, f32::max);
        assert!(
            peak > 1.0 && peak < 1.15,
            "overshoot слишком резкий: {peak}"
        );
    }

    #[test]
    fn stagger_keeps_the_first_row_unlagged() {
        assert_eq!(stagger(0, 0.0), 0.0);
        assert!(stagger(0, 0.1) > 0.0);
        assert_eq!(stagger(1, 0.0), 0.0);
        assert!(stagger(1, 0.1) < stagger(0, 0.1));
    }

    #[test]
    fn stagger_ends_every_row_at_one() {
        for index in [0, 1, 5, 11, 40] {
            assert_eq!(stagger(index, 1.0), 1.0, "строка {index}");
            assert!(stagger(index, 0.5) <= 1.0);
        }
        assert!(stagger(2, 0.2) > 0.0);
        assert_eq!(stagger(40, 0.2), stagger(11, 0.2));
    }

    /// Один кадр с фиксированным временем.
    fn drive(ctx: &egui::Context, presence: &mut Presence, open: bool, seconds: f64) -> f32 {
        let input = egui::RawInput {
            time: Some(seconds),
            predicted_dt: 1.0 / 60.0,
            ..Default::default()
        };
        let mut progress = -1.0;
        let output = ctx.run_pass(input, |ctx| {
            progress = presence.value(ctx, open, SCENE_ENTER, false);
        });
        assert!(progress >= 0.0, "Presence не вернул прогресс");
        // Дельты текстур (шрифт на первом кадре) в тесте не применяются.
        output.drop_without_applying_deltas();
        progress
    }

    /// Считает запросы перерисовки, которые делает egui.
    fn repaint_counter(ctx: &egui::Context) -> Arc<AtomicUsize> {
        let counter = Arc::new(AtomicUsize::new(0));
        let sink = Arc::clone(&counter);
        ctx.set_request_repaint_callback(move |_| {
            sink.fetch_add(1, atomic::Ordering::Relaxed);
        });
        counter
    }

    #[test]
    fn reduce_motion_jumps_to_the_final_state() {
        let ctx = egui::Context::default();
        let mut presence = Presence::new(egui::Id::unique("reduce-motion"));
        assert_eq!(presence.value(&ctx, true, DRAWER, true), 1.0);
        assert_eq!(presence.value(&ctx, false, DRAWER, true), 0.0);
    }

    #[test]
    fn zero_duration_behaves_like_reduce_motion() {
        let ctx = egui::Context::default();
        let mut presence = Presence::new(egui::Id::unique("zero-duration"));
        assert_eq!(presence.value(&ctx, true, 0.0, false), 1.0);
        assert_eq!(presence.value(&ctx, false, f32::NAN, false), 0.0);
    }

    #[test]
    fn entrance_starts_at_zero_and_reaches_one() {
        let ctx = egui::Context::default();
        let mut presence = Presence::new(egui::Id::unique("entrance"));
        // Первый кадр сидирует закрытое состояние: вход ещё не виден.
        assert_eq!(drive(&ctx, &mut presence, true, 0.0), 0.0);

        let mut previous = -1.0;
        let mut last = 0.0;
        for step in 1..=30u32 {
            let seconds = f64::from(step) / 60.0;
            let value = drive(&ctx, &mut presence, true, seconds);
            assert!(value >= previous, "переход идёт назад на шаге {step}");
            previous = value;
            last = value;
        }
        assert!(last > 0.99, "переход не завершился: {last}");
    }

    #[test]
    fn settled_transition_stops_requesting_frames() {
        let ctx = egui::Context::default();
        let requests = repaint_counter(&ctx);
        let mut presence = Presence::new(egui::Id::unique("repaint"));

        // Середина перехода: кадры нужны, иначе анимация встанет.
        assert_eq!(drive(&ctx, &mut presence, true, 0.0), 0.0);
        let middle = drive(&ctx, &mut presence, true, 1.0 / 60.0);
        assert!(middle > 0.0 && middle < 1.0, "середина перехода: {middle}");
        assert_ne!(requests.load(atomic::Ordering::Relaxed), 0);

        drive(&ctx, &mut presence, true, 0.5);
        assert_eq!(drive(&ctx, &mut presence, true, 1.0), 1.0);
        let settled = requests.load(atomic::Ordering::Relaxed);
        for step in 61..=90u32 {
            drive(&ctx, &mut presence, true, f64::from(step) / 60.0);
        }
        // «Вечных» анимаций быть не может (ADR-0032 §6).
        assert_eq!(
            requests.load(atomic::Ordering::Relaxed),
            settled,
            "завершённый переход продолжает просить кадры"
        );
    }

    #[test]
    fn new_presence_restarts_the_entrance() {
        let ctx = egui::Context::default();
        let id = egui::Id::unique("reset");
        let mut first = Presence::new(id);
        assert_eq!(drive(&ctx, &mut first, true, 0.0), 0.0);
        assert!(drive(&ctx, &mut first, true, 1.0 / 60.0) > 0.0);

        // Пересоздание окна даёт новый `Transitions`, поэтому вход с тем же `Id`
        // играется заново, а не догоняет завершённый анимационный менеджер.
        let mut second = Presence::new(id);
        assert_eq!(drive(&ctx, &mut second, true, 2.0 / 60.0), 0.0);
    }
}
