//! Планировщик фоновых проверок обновлений (ADR-0067 §5).
//!
//! Чистое решение «когда проверять» без сети и потоков: время передаётся
//! аргументом, поэтому логика полностью тестируема. Проверка уважает профиль
//! сети: `offline-first` её полностью отключает.

use std::time::{Duration, Instant};

use crate::settings::{NetworkProfile, UpdateCheck};

/// Первая проверка после старта — с небольшой задержкой, чтобы не мешать
/// запуску приложения.
const START_GRACE: Duration = Duration::from_secs(60);

/// Когда следующая проверка обновлений.
#[derive(Debug, Clone)]
pub struct UpdateScheduler {
    check: UpdateCheck,
    next_at: Option<Instant>,
}

impl UpdateScheduler {
    /// Новый планировщик: первая проверка через [`START_GRACE`], затем по
    /// интервалу выбранной частоты.
    pub fn new(check: UpdateCheck, now: Instant) -> Self {
        let next_at = check.interval_hours().map(|_| now + START_GRACE);
        Self { check, next_at }
    }

    /// Текущая частота. Потребитель — IPC-команда настроек обновлений
    /// и тесты (runtime-смена частоты — UPD-3b).
    #[allow(dead_code)]
    pub fn check(&self) -> UpdateCheck {
        self.check
    }

    /// Сменить частоту (из настроек): пересчитать ближайший момент.
    /// Потребитель — IPC-команда настроек обновлений (UPD-3b).
    #[allow(dead_code)]
    pub fn set_check(&mut self, check: UpdateCheck, now: Instant) {
        if self.check == check {
            return;
        }
        self.check = check;
        self.next_at = check.interval_hours().map(|_| now + START_GRACE);
    }

    /// Момент, когда планировщик проснётся; `None` — проверка выключена.
    pub fn next_at(&self) -> Option<Instant> {
        self.next_at
    }

    /// Пора проверять? `offline-first` — никогда.
    pub fn due(&self, now: Instant, profile: NetworkProfile) -> bool {
        if !profile.allows_background_network() {
            return false;
        }
        self.next_at.is_some_and(|at| now >= at)
    }

    /// Отметить выполненную проверку (успех или ошибка — одинаково, чтобы не
    /// долбить сеть): следующий раз через интервал.
    pub fn mark_checked(&mut self, now: Instant) {
        self.next_at = self
            .check
            .interval_hours()
            .map(|hours| now + Duration::from_secs(hours * 3600));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_check_is_after_grace() {
        let now = Instant::now();
        let scheduler = UpdateScheduler::new(UpdateCheck::Daily, now);
        assert!(!scheduler.due(now, NetworkProfile::Unlimited));
        assert!(scheduler.due(now + START_GRACE, NetworkProfile::Unlimited));
    }

    #[test]
    fn off_never_checks() {
        let now = Instant::now();
        let scheduler = UpdateScheduler::new(UpdateCheck::Off, now);
        assert!(scheduler.next_at().is_none());
        assert!(!scheduler.due(now + Duration::from_secs(10_000), NetworkProfile::Unlimited));
    }

    #[test]
    fn offline_first_never_checks() {
        let now = Instant::now();
        let scheduler = UpdateScheduler::new(UpdateCheck::Daily, now);
        assert!(!scheduler.due(now + START_GRACE, NetworkProfile::OfflineFirst));
    }

    #[test]
    fn mark_checked_waits_full_interval() {
        let now = Instant::now();
        let mut scheduler = UpdateScheduler::new(UpdateCheck::Weekly, now);
        scheduler.mark_checked(now);
        assert!(!scheduler.due(
            now + Duration::from_secs(24 * 3600),
            NetworkProfile::Unlimited
        ));
        assert!(scheduler.due(
            now + Duration::from_secs(7 * 24 * 3600),
            NetworkProfile::Unlimited
        ));
    }

    #[test]
    fn set_check_reschedules() {
        let now = Instant::now();
        let mut scheduler = UpdateScheduler::new(UpdateCheck::Daily, now);
        scheduler.mark_checked(now);
        scheduler.set_check(UpdateCheck::Weekly, now);
        assert_eq!(scheduler.check(), UpdateCheck::Weekly);
        assert!(scheduler.due(now + START_GRACE, NetworkProfile::Unlimited));
        scheduler.set_check(UpdateCheck::Off, now);
        assert!(scheduler.next_at().is_none());
    }
}
