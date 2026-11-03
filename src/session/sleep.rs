//! Таймер сна: остановка эфира по дедлайну, в окне и в простое без окна.
//!
//! Дедлайн живёт в `Session`, а не в аудио-ядре: таймер обязан переживать
//! закрытие окна, но не должен переживать выход процесса. Пока окно открыто,
//! дедлайн проверяется в `tick`; в простое — отдельной веткой `park`.

use std::time::{Duration, Instant};

use crate::ipc::MAX_SLEEP_MINUTES;

use super::notice::NoticeKey;

/// Дедлайн таймера сна. Арифметика отделена от аудио-ядра, чтобы
/// проверяться в тестах без живого `Session` (ADR-0046).
#[derive(Debug, Default)]
pub(crate) struct SleepTimer {
    deadline: Option<Instant>,
    minutes: Option<u16>,
}

impl SleepTimer {
    /// Включает или заменяет таймер. `minutes` ограничен
    /// `1..=MAX_SLEEP_MINUTES`.
    pub(crate) fn set(&mut self, now: Instant, minutes: u16) -> Result<(), String> {
        if !(1..=MAX_SLEEP_MINUTES).contains(&minutes) {
            return Err(format!(
                "таймер сна: минуты должны быть в диапазоне 1..={MAX_SLEEP_MINUTES}"
            ));
        }
        self.deadline = Some(now + Duration::from_secs(u64::from(minutes) * 60));
        self.minutes = Some(minutes);
        Ok(())
    }

    pub(crate) fn clear(&mut self) {
        self.deadline = None;
        self.minutes = None;
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
}

impl super::Session {
    /// Включает таймер сна на `minutes` минут. Повторный вызов заменяет дедлайн.
    pub(crate) fn set_sleep(&mut self, minutes: u16) -> Result<(), String> {
        self.sleep.set(Instant::now(), minutes)
    }

    /// Отменяет таймер сна. Без активного таймера — no-op.
    pub(crate) fn clear_sleep(&mut self) {
        self.sleep.clear();
    }

    /// Сколько осталось до срабатывания таймера.
    pub(crate) fn sleep_remaining(&self) -> Option<Duration> {
        self.sleep.remaining(Instant::now())
    }

    pub(crate) fn sleep_deadline(&self) -> Option<Instant> {
        self.sleep.deadline()
    }

    /// Выбранная длительность для контрола настроек: `None` — таймер выключен.
    pub(crate) fn sleep_minutes(&self) -> Option<u16> {
        self.sleep.minutes()
    }

    /// Проверяет дедлайн. Вызывается из `tick` и из ветки простоя `park`.
    pub(crate) fn poll_sleep(&mut self) {
        if !self.sleep.expired(Instant::now()) {
            return;
        }
        self.clear_sleep();
        if let Err(err) = self.stop_playback() {
            tracing::warn!(error = %err, "не удалось остановить эфир по таймеру сна");
        }
        self.notify_info(NoticeKey::SleepExpired);
        self.publish_status();
    }

    /// Ближайший срок перерисовки окна ради дедлайна.
    pub(crate) fn sleep_repaint_after(&self) -> Option<Duration> {
        self.sleep_remaining()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_validates_range_and_replaces_deadline() {
        let mut timer = SleepTimer::default();
        let now = Instant::now();

        assert!(timer.set(now, 0).is_err());
        assert!(timer.set(now, MAX_SLEEP_MINUTES + 1).is_err());
        assert_eq!(timer.minutes(), None);
        assert_eq!(timer.deadline(), None);

        timer.set(now, 90).expect("90 минут допустимы");
        assert_eq!(timer.minutes(), Some(90));
        assert_eq!(timer.remaining(now), Some(Duration::from_secs(90 * 60)));
        assert!(!timer.expired(now + Duration::from_secs(90 * 60 - 1)));
        assert!(timer.expired(now + Duration::from_secs(90 * 60)));

        timer
            .set(now, 15)
            .expect("повторная установка заменяет дедлайн");
        assert_eq!(timer.minutes(), Some(15));
        assert_eq!(timer.remaining(now), Some(Duration::from_secs(15 * 60)));
    }

    #[test]
    fn clear_disables_timer_and_remaining_saturates() {
        let mut timer = SleepTimer::default();
        let now = Instant::now();
        timer.set(now, 5).expect("5 минут допустимы");

        assert_eq!(
            timer.remaining(now + Duration::from_secs(10 * 60)),
            Some(Duration::ZERO)
        );
        timer.clear();
        assert_eq!(timer.minutes(), None);
        assert_eq!(timer.remaining(now), None);
        assert!(!timer.expired(now + Duration::from_secs(24 * 60 * 60)));
    }
}
