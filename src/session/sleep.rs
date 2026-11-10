//! Таймер сна: остановка эфира по дедлайну, в окне и в простое без окна.
//!
//! Дедлайн живёт в `Session`, а не в аудио-ядре: таймер обязан переживать
//! закрытие окна, но не должен переживать выход процесса. Пока окно открыто,
//! события проверяются в `tick`; в простое — веткой `park`.
//!
//! События таймера — фейд, предупреждение, истечение и смена отображаемого
//! отсчёта (ADR-0055…0058). `next_event` возвращает ближайшее из них, поэтому
//! парковка остаётся без интервальных таймеров.

use std::time::{Duration, Instant};

use crate::audio::PlaybackState;
use crate::i18n;
use crate::ipc::MAX_SLEEP_MINUTES;
use crate::power::{self, PowerInhibit};
use crate::settings::SleepAction;

use super::notice::NoticeKey;

/// За сколько до действия показывается предупреждение (ADR-0058).
pub(crate) const SLEEP_WARNING_LEAD: Duration = Duration::from_secs(60);

/// Дедлайн таймера сна. Арифметика отделена от аудио-ядра, чтобы
/// проверяться в тестах без живого `Session` (ADR-0046).
#[derive(Debug, Default)]
pub(crate) struct SleepTimer {
    deadline: Option<Instant>,
    minutes: Option<u16>,
    /// Начало затухания; `None` — фейд выключен или уже сработал.
    fade_start: Option<Instant>,
    /// Предупреждение уже показано: ровно один раз на таймер (ADR-0058).
    warning_shown: bool,
}

impl SleepTimer {
    /// Включает или заменяет таймер. `minutes` ограничен
    /// `1..=MAX_SLEEP_MINUTES`, `fade_seconds` — длительность затухания.
    pub(crate) fn set(
        &mut self,
        now: Instant,
        minutes: u16,
        fade_seconds: u16,
    ) -> Result<(), String> {
        if !(1..=MAX_SLEEP_MINUTES).contains(&minutes) {
            return Err(format!(
                "таймер сна: минуты должны быть в диапазоне 1..={MAX_SLEEP_MINUTES}"
            ));
        }
        self.deadline = Some(now + Duration::from_secs(u64::from(minutes) * 60));
        self.minutes = Some(minutes);
        self.warning_shown = false;
        self.rearm_fade(now, fade_seconds);
        Ok(())
    }

    /// Пересчитывает начало фейда при смене настройки у активного таймера.
    pub(crate) fn set_fade(&mut self, now: Instant, fade_seconds: u16) {
        self.rearm_fade(now, fade_seconds);
    }

    fn rearm_fade(&mut self, now: Instant, fade_seconds: u16) {
        self.fade_start = match (self.deadline, fade_seconds) {
            (Some(deadline), 1..) => {
                let fade = Duration::from_secs(u64::from(fade_seconds));
                Some(if deadline.saturating_duration_since(now) <= fade {
                    now
                } else {
                    deadline - fade
                })
            }
            _ => None,
        };
    }

    pub(crate) fn clear(&mut self) {
        self.deadline = None;
        self.minutes = None;
        self.fade_start = None;
        self.warning_shown = false;
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// Выбранная длительность для контрола настроек: `None` — таймер выключен.
    pub(crate) fn minutes(&self) -> Option<u16> {
        self.minutes
    }

    /// Сколько осталось до срабатывания на момент `now`.
    pub(crate) fn remaining(&self, now: Instant) -> Option<Duration> {
        self.deadline
            .map(|deadline| deadline.saturating_duration_since(now))
    }

    pub(crate) fn expired(&self, now: Instant) -> bool {
        self.deadline.is_some_and(|deadline| now >= deadline)
    }

    /// Начало фейда, если он ещё не сработал.
    #[allow(dead_code)]
    pub(crate) fn fade_start(&self) -> Option<Instant> {
        self.fade_start
    }

    /// Забирает сработавший фейд: повторно событие не вернётся.
    pub(crate) fn take_fade(&mut self, now: Instant) -> bool {
        if self.fade_start.is_some_and(|start| now >= start) {
            self.fade_start = None;
            true
        } else {
            false
        }
    }

    /// Момент предупреждения: за минуту до действия.
    pub(crate) fn warning_start(&self) -> Option<Instant> {
        self.deadline
            .and_then(|deadline| deadline.checked_sub(SLEEP_WARNING_LEAD))
    }

    #[allow(dead_code)]
    pub(crate) fn warning_shown(&self) -> bool {
        self.warning_shown
    }

    pub(crate) fn mark_warning_shown(&mut self) {
        self.warning_shown = true;
    }

    /// Предупреждение пора показать и оно ещё не показано.
    pub(crate) fn warning_due(&self, now: Instant) -> bool {
        !self.warning_shown && self.warning_start().is_some_and(|start| now >= start)
    }

    /// Сколько секунд показывать в отсчёте: до последней минуты — целые
    /// минуты, в последнюю — целые секунды (ADR-0057).
    #[allow(dead_code)]
    pub(crate) fn display_seconds(&self, now: Instant) -> Option<u64> {
        let remaining = self.remaining(now)?;
        let secs = remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0);
        Some(if secs > 60 {
            secs.div_ceil(60) * 60
        } else {
            secs
        })
    }

    /// Когда отображаемая строка изменится: до последней минуты — на границе
    /// минуты, в последнюю — на границе секунды.
    pub(crate) fn next_display_change(&self, now: Instant) -> Option<Instant> {
        let remaining = self.remaining(now)?;
        if remaining <= Duration::from_secs(60) {
            let fraction = remaining - Duration::from_secs(remaining.as_secs());
            Some(
                now + if fraction.is_zero() {
                    Duration::from_secs(1)
                } else {
                    fraction
                },
            )
        } else {
            let whole_minutes = Duration::from_secs(remaining.as_secs() / 60 * 60);
            let to_boundary = remaining - whole_minutes;
            Some(
                now + if to_boundary.is_zero() {
                    Duration::from_secs(60)
                } else {
                    to_boundary
                },
            )
        }
    }

    /// Ближайшее будущее событие: истечение, фейд, предупреждение или смена
    /// отсчёта. `None` — будить сессию нечем.
    ///
    /// Порядок приоритета: фейд (если ещё не started), затем предупреждение,
    /// затем истечение. Такты отображения учитываются только между фейдом и
    /// последней минутой — в последнюю минуту будимся только на действие, чтобы
    /// не поднимать парковку каждый такт отсчёта «в никуда».
    pub(crate) fn next_event(&self, now: Instant, warning_enabled: bool) -> Option<Instant> {
        let deadline = self.deadline?;
        // В последнюю минуту отсчёт идёт по секундам: будимся только на действие.
        if deadline.saturating_duration_since(now) <= Duration::from_secs(60) {
            return Some(deadline);
        }
        // Фейд ещё не начался — это ближайшее событие; такты отображения до него
        // не нужны.
        if let Some(fade) = self.fade_start.filter(|start| *start > now) {
            return Some(fade);
        }
        let mut candidates = Vec::new();
        if let Some(warning) = self
            .warning_start()
            .filter(|_| warning_enabled && !self.warning_shown)
        {
            candidates.push(warning);
        }
        candidates.push(deadline);
        if let Some(display) = self.next_display_change(now) {
            candidates.push(display);
        }
        candidates.into_iter().filter(|event| *event > now).min()
    }
}

impl super::Session {
    /// Включает таймер сна на `minutes` минут. Повторный вызов заменяет дедлайн.
    pub(crate) fn set_sleep(&mut self, minutes: u16) -> Result<(), String> {
        let fade = self.settings.settings().sleep_fade_seconds;
        self.sleep.set(Instant::now(), minutes, fade)?;
        // Новый таймер снимает разовый запрет действия (ADR-0058).
        self.sleep_action_once = None;
        // Прежний фейд относился к прежнему дедлайну: громкость возвращается,
        // а новый фейд взведён `set` (ADR-0056).
        self.cancel_sleep_fade(true);
        // TTL `caffeinate` считается от нового дедлайна: старый носитель
        // снимается, `sync_inhibition` берёт лок заново (ADR-0054 §3).
        self.release_sleep_inhibition();
        self.reset_inhibition_retry();
        self.sync_inhibition();
        Ok(())
    }

    /// Отменяет таймер сна. Без активного таймера — no-op.
    pub(crate) fn clear_sleep(&mut self) {
        self.sleep.clear();
        self.sleep_action_once = None;
        self.cancel_sleep_fade(true);
        self.release_sleep_inhibition();
        self.reset_inhibition_retry();
        self.sync_inhibition();
    }

    /// Действие по истечении таймера (ADR-0055).
    pub(crate) fn set_sleep_action(&mut self, action: SleepAction) -> Result<(), String> {
        self.settings
            .set_sleep_action(action)
            .map_err(|err| err.to_string())
    }

    /// Длительность затухания (ADR-0056): уже начатый фейд пересчитывается.
    pub(crate) fn set_sleep_fade_seconds(&mut self, seconds: u16) -> Result<u16, String> {
        self.settings
            .set_sleep_fade_seconds(seconds)
            .map_err(|err| err.to_string())?;
        let applied = self.settings.settings().sleep_fade_seconds;
        self.apply_sleep_fade_setting(applied);
        Ok(applied)
    }

    /// Запрет сна на время таймера (ADR-0054): лок берётся или снимается сразу.
    pub(crate) fn set_sleep_prevent_sleep(&mut self, enabled: bool) -> Result<(), String> {
        self.settings
            .set_sleep_prevent_sleep(enabled)
            .map_err(|err| err.to_string())?;
        self.reset_inhibition_retry();
        self.sync_inhibition();
        Ok(())
    }

    /// Окно предупреждения за минуту до действия (ADR-0058).
    pub(crate) fn set_sleep_warning(&mut self, enabled: bool) -> Result<(), String> {
        self.settings
            .set_sleep_warning(enabled)
            .map_err(|err| err.to_string())
    }

    /// Смена настройки затухания у активного таймера (ADR-0056): уже начатый
    /// фейд отменяется с возвратом громкости, новый отсчитывается от дедлайна.
    pub(crate) fn apply_sleep_fade_setting(&mut self, seconds: u16) {
        self.cancel_sleep_fade(true);
        self.sleep.set_fade(Instant::now(), seconds);
    }

    /// Начало плавного затухания: длительность берётся из настроек на момент
    /// события, чтобы смена настройки не зависела от порядка вызовов.
    pub(super) fn start_sleep_fade(&mut self) {
        let seconds = self.settings.settings().sleep_fade_seconds;
        if seconds == 0 {
            return;
        }
        match self.audio.start_fade(u64::from(seconds) * 1_000) {
            Ok(()) => self.sleep_fading = true,
            Err(err) => {
                tracing::warn!(error = %err, "не удалось начать затухание перед сном");
            }
        }
    }

    /// Снимает затухание. `restore` возвращает громкость, которая была до фейда;
    /// при остановке эфира восстанавливать нечего.
    pub(super) fn cancel_sleep_fade(&mut self, restore: bool) {
        if !self.sleep_fading {
            return;
        }
        self.sleep_fading = false;
        if let Err(err) = self.audio.cancel_fade(restore) {
            tracing::warn!(error = %err, "не удалось снять затухание перед сном");
        }
    }

    /// Показывает окно предупреждения ровно один раз на таймер. Носитель
    /// выбирает presentation-слой: потомок стола или отдельный прогон
    /// припаркованного процесса (ADR-0058).
    fn show_sleep_warning(&mut self) {
        self.sleep_warning_open = true;
        // Живой UI-пасс нарисует предупреждение immediate-потомком; парковка
        // откроет отдельное окно (ADR-0058).
        self.window.request_warning();
        self.wake.waker()();
    }

    pub(crate) fn close_sleep_warning(&mut self) {
        self.sleep_warning_open = false;
    }

    /// «Отменить» в предупреждении: действие отменяется только на этот таймер,
    /// отсчёт досчитывает, громкость возвращается (ADR-0058).
    pub(crate) fn cancel_sleep_warning(&mut self) {
        self.sleep_action_once = Some(SleepAction::Nothing);
        self.close_sleep_warning();
        self.cancel_sleep_fade(true);
        self.publish_status();
    }

    /// Действие по истечении: разовый выбор из предупреждения важнее настройки.
    pub(crate) fn effective_sleep_action(&self) -> SleepAction {
        self.sleep_action_once
            .unwrap_or(self.settings.settings().sleep_action)
    }

    /// Сколько осталось до срабатывания таймера.
    #[allow(dead_code)]
    pub(crate) fn sleep_remaining(&self) -> Option<Duration> {
        self.sleep.remaining(Instant::now())
    }

    pub(crate) fn sleep_minutes(&self) -> Option<u16> {
        self.sleep.minutes()
    }

    /// Отображаемые секунды для кнопки часов, трея и статуса.
    pub(crate) fn sleep_display_seconds(&self) -> Option<u64> {
        self.sleep.display_seconds(Instant::now())
    }

    /// Оставшееся время, отформатированное под язык интерфейса (ADR-0057).
    pub(crate) fn sleep_remaining_label(&self) -> Option<String> {
        let secs = self.sleep_display_seconds()?;
        Some(i18n::sleep_remaining_label(self.language, secs))
    }

    /// Проверяет события таймера. Вызывается из `tick` и из ветки простоя `park`.
    pub(crate) fn poll_sleep_events(&mut self) {
        let now = Instant::now();
        if self.sleep.take_fade(now) {
            self.start_sleep_fade();
        }
        if self.settings.settings().sleep_warning && self.sleep.warning_due(now) {
            self.sleep.mark_warning_shown();
            self.show_sleep_warning();
        }
        if self.sleep.expired(now) {
            self.sleep_expiry();
        }
    }

    /// Истечение таймера: порядок шагов зафиксирован ADR-0055.
    fn sleep_expiry(&mut self) {
        self.sleep.clear();
        // Свой `block`-лок не должен мешать собственному действию.
        self.release_sleep_inhibition();
        // Эфир останавливается: возвращать громкость некому.
        self.cancel_sleep_fade(false);
        if let Err(err) = self.stop_playback() {
            tracing::warn!(error = %err, "не удалось остановить эфир по таймеру сна");
        }
        self.sync_tray();
        self.publish_status();
        self.close_sleep_warning();
        self.notify_info(NoticeKey::SleepExpired);
        self.perform_sleep_action();
    }

    /// Ближайший срок перерисовки окна ради отсчёта.
    pub(crate) fn sleep_repaint_after(&self) -> Option<Duration> {
        self.sleep
            .next_display_change(Instant::now())
            .map(|next| next.saturating_duration_since(Instant::now()))
    }

    /// Ближайшее событие таймера для ветки простоя.
    pub(crate) fn sleep_next_event(&self) -> Option<Instant> {
        self.sleep
            .next_event(Instant::now(), self.settings.settings().sleep_warning)
    }

    /// Синхронизирует ингибитор сна с текущим состоянием (ADR-0054 §4).
    /// Лок удерживается ровно пока играет эфир и тикает таймер.
    ///
    /// Отказ acquire и смерть носителя кэшируются: повторной попытки на каждом
    /// тике нет, иначе лог и статус забивались бы одинаковыми ошибками. Сброс
    /// кэша — смена таймера, настройки или состояния эфира.
    pub(crate) fn sync_inhibition(&mut self) {
        let enabled = self.settings.settings().sleep_prevent_sleep;
        let active =
            self.sleep.deadline().is_some() && self.audio.state() != PlaybackState::Stopped;
        let needed = enabled && active && !self.window.is_quitting();

        let mut keep = None;
        if let Some(mut guard) = self.sleep_inhibition.take() {
            if needed && guard.is_alive() {
                keep = Some(guard);
            } else if !guard.is_alive() && needed {
                // Носитель умер: таймер работает, но лок больше не держится.
                self.notify_info(NoticeKey::InhibitionLost);
                self.sleep_inhibition_failed = true;
                self.sleep_inhibition_detail = None;
            }
        }
        self.sleep_inhibition = keep;

        if !needed {
            // Состояние «лок не нужен» — точка сброса кэша отказа: следующий
            // запуск эфира или таймера снова попробует взять лок.
            self.sleep_inhibition_failed = false;
            self.sleep_inhibition_detail = None;
            return;
        }
        if self.sleep_inhibition.is_some() || self.sleep_inhibition_failed {
            return;
        }

        match PowerInhibit::acquire("сон таймера OrbitFM", self.sleep_inhibition_ttl()) {
            Ok(guard) => {
                self.sleep_inhibition = Some(guard);
                self.sleep_inhibition_detail = None;
            }
            Err(err) => {
                self.sleep_inhibition_failed = true;
                self.sleep_inhibition_detail = Some(err.to_string());
                tracing::warn!(error = %err, "лок сна не взят");
            }
        }
    }

    /// TTL для `caffeinate`: оставшееся время таймера, вверх до целой секунды.
    /// `-t 0` означал бы «без таймаута» и оставил бы лок после выхода из строя
    /// `-w`, поэтому минимум — одна секунда (ADR-0054 §3).
    fn sleep_inhibition_ttl(&self) -> u64 {
        self.sleep
            .remaining(Instant::now())
            .map_or(0, |remaining| {
                remaining
                    .as_secs()
                    .saturating_add(u64::from(remaining.subsec_nanos() > 0))
            })
            .max(1)
    }

    /// Сбрасывает кэш неудачного acquire: вызывается при смене таймера и
    /// настройки «не давать системе уснуть».
    fn reset_inhibition_retry(&mut self) {
        self.sleep_inhibition_failed = false;
        self.sleep_inhibition_detail = None;
    }

    /// Освобождает лок сна перед собственным действием таймера (ADR-0055).
    pub(crate) fn release_sleep_inhibition(&mut self) {
        self.sleep_inhibition = None;
        self.sleep_inhibition_detail = None;
        self.sleep_inhibition_failed = false;
    }

    /// Действие по истечении таймера: ничего / сон / выключение (ADR-0055).
    pub(crate) fn perform_sleep_action(&mut self) {
        let action = match self.effective_sleep_action() {
            SleepAction::Nothing => power::PowerAction::Nothing,
            SleepAction::Suspend => power::PowerAction::Suspend,
            SleepAction::PowerOff => power::PowerAction::PowerOff,
        };
        if let Err(detail) = power::perform(action) {
            self.notify_error_detail(NoticeKey::SleepActionFailed, detail);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_validates_range_and_replaces_deadline() {
        let mut timer = SleepTimer::default();
        let now = Instant::now();

        assert!(timer.set(now, 0, 0).is_err());
        assert!(timer.set(now, MAX_SLEEP_MINUTES + 1, 0).is_err());
        assert_eq!(timer.minutes(), None);
        assert_eq!(timer.deadline(), None);

        timer.set(now, 90, 0).expect("90 минут допустимы");
        assert_eq!(timer.minutes(), Some(90));
        assert_eq!(timer.remaining(now), Some(Duration::from_secs(90 * 60)));
        assert!(!timer.expired(now + Duration::from_secs(90 * 60 - 1)));
        assert!(timer.expired(now + Duration::from_secs(90 * 60)));

        timer
            .set(now, 15, 0)
            .expect("повторная установка заменяет дедлайн");
        assert_eq!(timer.minutes(), Some(15));
        assert_eq!(timer.remaining(now), Some(Duration::from_secs(15 * 60)));
    }

    #[test]
    fn clear_disables_timer_and_remaining_saturates() {
        let mut timer = SleepTimer::default();
        let now = Instant::now();
        timer.set(now, 5, 0).expect("5 минут допустимы");

        assert_eq!(
            timer.remaining(now + Duration::from_secs(10 * 60)),
            Some(Duration::ZERO)
        );
        timer.clear();
        assert_eq!(timer.minutes(), None);
        assert_eq!(timer.remaining(now), None);
        assert!(!timer.expired(now + Duration::from_secs(24 * 60 * 60)));
    }

    #[test]
    fn fade_starts_before_deadline_or_immediately_when_longer_than_timer() {
        let mut timer = SleepTimer::default();
        let now = Instant::now();
        timer.set(now, 15, 300).expect("15 минут");
        assert_eq!(
            timer.fade_start(),
            Some(now + Duration::from_secs(10 * 60)),
            "фейд 300 с начинается за 5 минут до дедлайна"
        );

        timer.set(now, 5, 300).expect("5 минут");
        assert_eq!(
            timer.fade_start(),
            Some(now),
            "фейд длиннее таймера начинается сразу"
        );

        timer.set(now, 15, 0).expect("без фейда");
        assert_eq!(timer.fade_start(), None);

        timer.set(now, 15, 60).expect("фейд минута");
        assert_eq!(
            timer.fade_start(),
            Some(now + Duration::from_secs(14 * 60)),
            "фейд начинается за минуту до дедлайна"
        );
        // `take_fade` забирает сработавший фейд ровно один раз: на границе он уже
        // срабатывает, более ранний вызов — нет.
        timer.take_fade(now + Duration::from_secs(14 * 60));
        assert!(!timer.take_fade(now + Duration::from_secs(13 * 60)));
        assert_eq!(timer.fade_start(), None, "фейд забирается один раз");
    }

    #[test]
    fn display_rounds_up_to_minutes_then_to_seconds() {
        let mut timer = SleepTimer::default();
        let now = Instant::now();
        timer.set(now, 60, 0).expect("час");

        assert_eq!(timer.display_seconds(now), Some(3600));
        assert_eq!(
            timer.display_seconds(now + Duration::from_secs(1)),
            Some(3600),
            "до последней минуты отсчёт минутный"
        );
        assert_eq!(
            timer.display_seconds(now + Duration::from_secs(3600 - 90)),
            Some(120)
        );
        assert_eq!(
            timer.display_seconds(now + Duration::from_secs(3600 - 60)),
            Some(60)
        );
        assert_eq!(
            timer.display_seconds(now + Duration::from_secs(3600 - 59)),
            Some(59),
            "в последнюю минуту — секунды"
        );
    }

    #[test]
    fn next_display_change_is_the_next_minute_or_second_boundary() {
        let mut timer = SleepTimer::default();
        let now = Instant::now();
        timer.set(now, 10, 0).expect("10 минут");

        assert_eq!(
            timer.next_display_change(now),
            Some(now + Duration::from_secs(60)),
            "ровно на границе минуты следующая смена через минуту"
        );
        assert_eq!(
            timer.next_display_change(now + Duration::from_secs(30)),
            Some(now + Duration::from_secs(60))
        );
        assert_eq!(
            timer.next_display_change(now + Duration::from_secs(9 * 60 + 59)),
            Some(now + Duration::from_secs(9 * 60 + 60)),
            "в последнюю минуту — граница секунды"
        );
    }

    #[test]
    fn next_event_picks_fade_then_warning_then_expiry() {
        let mut timer = SleepTimer::default();
        let now = Instant::now();
        timer.set(now, 10, 300).expect("10 минут");

        let fade = now + Duration::from_secs(5 * 60);
        let warning = now + Duration::from_secs(9 * 60);
        let deadline = now + Duration::from_secs(10 * 60);

        assert_eq!(timer.next_event(now, true), Some(fade));
        assert_eq!(
            timer.next_event(fade, true),
            Some(fade + Duration::from_secs(60))
        );
        assert_eq!(
            timer.next_event(warning - Duration::from_secs(30), true),
            Some(warning)
        );
        assert_eq!(
            timer.next_event(warning, true),
            Some(warning + Duration::from_secs(60))
        );
        assert_eq!(
            timer.next_event(deadline - Duration::from_secs(30), false),
            Some(deadline)
        );
    }

    #[test]
    fn warning_is_shown_once_per_timer() {
        let mut timer = SleepTimer::default();
        let now = Instant::now();
        timer.set(now, 2, 0).expect("2 минуты");
        let warning = timer.warning_start().expect("предупреждение есть");

        assert!(!timer.warning_due(warning - Duration::from_secs(1)));
        assert!(timer.warning_due(warning));
        assert!(timer.warning_due(warning + Duration::from_secs(1)));
        timer.mark_warning_shown();
        assert!(!timer.warning_due(warning + Duration::from_secs(1)));
        assert!(
            !timer
                .next_event(warning + Duration::from_secs(1), true)
                .is_some_and(|event| event == warning)
        );

        timer.set(now, 2, 0).expect("новый таймер");
        assert!(!timer.warning_shown());
    }
}
