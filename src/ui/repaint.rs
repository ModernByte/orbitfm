//! Политика перерисовки: один язык для всех видов кадров стола.
//!
//! Волна 1 плана `docs/plans/render-frame-budget.md`: вместо разрозненных
//! `request_repaint*` и локальных констант — [`RepaintReason`] и
//! [`FrameMode`] (режим кадра целиком). Частота кадров задаётся в одном месте
//! ([`cadence`]), поэтому её можно проверить тестом без egui.
//!
//! Режимы (ADR-0023 авто-вращение, ADR-0028 движение, ADR-0032 reduce motion):
//!
//! | Режим | Когда | Периодический кадр |
//! |---|---|---|
//! | Asleep | окно скрыто, `reduce_motion`, без эфира | нет |
//! | Breathing | движение включено, эфира нет | 66 мс (~15 fps) |
//! | Auto-spin | `motion = Off`, `reduce_motion = false` | 33 мс (~30 fps) |
//! | Play | эфир | 33 мс (~30 fps) |
//!
//! Важно: `motion = Off` — не покой, а авто-вращение (ADR-0023); настоящий
//! «сон» даёт `reduce_motion` (или скрытое окно) без эфира.

use std::time::Duration;

use eframe::egui;

/// Взаимодействие и затухание callout: не чаще 60 fps и только пока движется.
const INTERACTION: Duration = Duration::from_millis(16);
/// Эфир, авто-вращение и пульс маркера: ~30 кадров в секунду.
const PLAYBACK: Duration = Duration::from_millis(33);
/// Idle-дыхание планеты: ~15 кадров в секунду (ADR-0028).
const IDLE_BREATH: Duration = Duration::from_millis(66);

/// Причина запроса кадра; определяет частоту через [`cadence`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RepaintReason {
    /// Событие из фона, IPC или ввода: рисовать немедленно.
    Input,
    /// Перетаскивание, колесо, инерция камеры — пока не остановилось.
    Interaction,
    /// Затухание callout (ADR-0032).
    Callout,
    /// Эфир: спектр, пульс маркера, орбитальное кольцо.
    Playback,
    /// Авто-вращение глобуса при `motion = Off` (ADR-0023).
    IdleSpin,
    /// Idle-дыхание глобуса без эфира.
    IdleBreath,
    /// Опрос фоновых задач (favicon, skeleton, прогресс) с конкретной паузой.
    Background(Duration),
    /// Один будильник на точный срок (таймер сна, начало авто-вращения).
    Deadline(Duration),
}

/// Периодический режим кадра стола: кто держит окно бодрым без внешних событий.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameMode {
    /// Покой: кадров по расписанию нет.
    Asleep,
    /// Дыхание глобуса без эфира.
    Breathing,
    /// Авто-вращение при `motion = Off` (ADR-0023).
    AutoSpin,
    /// Эфир.
    Play,
}

/// Частота кадра для причины; `None` — рисовать немедленно.
pub(crate) fn cadence(reason: RepaintReason) -> Option<Duration> {
    match reason {
        RepaintReason::Input => None,
        RepaintReason::Interaction | RepaintReason::Callout => Some(INTERACTION),
        RepaintReason::Playback | RepaintReason::IdleSpin => Some(PLAYBACK),
        RepaintReason::IdleBreath => Some(IDLE_BREATH),
        RepaintReason::Background(delay) | RepaintReason::Deadline(delay) => Some(delay),
    }
}

/// Просит у egui кадр по причине: немедленно или через [`cadence`].
pub(crate) fn schedule(ctx: &egui::Context, reason: RepaintReason) {
    match cadence(reason) {
        Some(delay) => ctx.request_repaint_after(delay),
        None => ctx.request_repaint(),
    }
}

/// Режим кадра по состоянию стола.
///
/// `playing` важнее `reduce_motion`: в эфире кадры нужны визуализатору и пульсу
/// даже при уменьшенном движении (план § 3.1). `motion_enabled` — эффективная
/// реактивность [`crate::session::Session::motion_reactivity`], уже свёрнутая
/// с `reduce_motion` и режимом окна.
pub(crate) fn frame_mode(
    window_visible: bool,
    reduce_motion: bool,
    motion_enabled: bool,
    playing: bool,
) -> FrameMode {
    if !window_visible {
        return FrameMode::Asleep;
    }
    if playing {
        return FrameMode::Play;
    }
    if reduce_motion {
        return FrameMode::Asleep;
    }
    if motion_enabled {
        FrameMode::Breathing
    } else {
        FrameMode::AutoSpin
    }
}

/// Периодический кадр режима; `None` — режим молчит до события или будильника.
pub(crate) fn periodic_reason(mode: FrameMode) -> Option<RepaintReason> {
    match mode {
        FrameMode::Asleep => None,
        FrameMode::Breathing => Some(RepaintReason::IdleBreath),
        FrameMode::AutoSpin => Some(RepaintReason::IdleSpin),
        FrameMode::Play => Some(RepaintReason::Playback),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cadence_table_is_stable() {
        assert_eq!(cadence(RepaintReason::Input), None);
        assert_eq!(cadence(RepaintReason::Interaction), Some(INTERACTION));
        assert_eq!(cadence(RepaintReason::Callout), Some(INTERACTION));
        assert_eq!(cadence(RepaintReason::Playback), Some(PLAYBACK));
        assert_eq!(cadence(RepaintReason::IdleSpin), Some(PLAYBACK));
        assert_eq!(cadence(RepaintReason::IdleBreath), Some(IDLE_BREATH));
        assert_eq!(
            cadence(RepaintReason::Background(Duration::from_millis(120))),
            Some(Duration::from_millis(120))
        );
    }

    #[test]
    fn asleep_never_requests_periodic_frames() {
        assert_eq!(periodic_reason(FrameMode::Asleep), None);
        // reduce_motion без эфира — покой, а не дыхание.
        assert_eq!(
            frame_mode(true, true, false, false),
            FrameMode::Asleep,
            "reduce_motion без эфира даёт ноль кадров"
        );
        // Скрытое окно молчит даже в эфире: на macOS вьюпорт не рисуется.
        assert_eq!(frame_mode(false, false, true, true), FrameMode::Asleep);
    }

    #[test]
    fn playing_keeps_frames_even_with_reduce_motion() {
        assert_eq!(frame_mode(true, true, false, true), FrameMode::Play);
        assert_eq!(
            periodic_reason(FrameMode::Play),
            Some(RepaintReason::Playback)
        );
    }

    #[test]
    fn motion_off_is_auto_spin_not_sleep() {
        // motion = Off и reduce_motion = false — это авто-вращение (ADR-0023),
        // а не сон: 30 fps, пока глобус крутится.
        assert_eq!(frame_mode(true, false, false, false), FrameMode::AutoSpin);
        assert_eq!(
            periodic_reason(FrameMode::AutoSpin),
            Some(RepaintReason::IdleSpin)
        );
        assert_eq!(
            cadence(periodic_reason(FrameMode::AutoSpin).unwrap()),
            Some(PLAYBACK)
        );
    }

    #[test]
    fn breathing_is_idle_speed() {
        assert_eq!(frame_mode(true, false, true, false), FrameMode::Breathing);
        assert_eq!(
            cadence(periodic_reason(FrameMode::Breathing).unwrap()),
            Some(IDLE_BREATH)
        );
    }

    #[test]
    fn deadline_is_a_single_alarm_not_a_periodic_frame() {
        let reason = RepaintReason::Deadline(Duration::from_secs(5));
        assert_eq!(cadence(reason), Some(Duration::from_secs(5)));
        // Ни один режим не выдаёт Deadline: будильник ставится точечно.
        for mode in [
            FrameMode::Asleep,
            FrameMode::Breathing,
            FrameMode::AutoSpin,
            FrameMode::Play,
        ] {
            assert!(!matches!(
                periodic_reason(mode),
                Some(RepaintReason::Deadline(_))
            ));
        }
    }
}
