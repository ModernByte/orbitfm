//! Тестовый бэкенд ингибитора сна (ADR-0054 §1).
//!
//! ОС не участвует: лок — флаг в памяти, поэтому `acquire`/`release`/`is_alive`
//! проверяются обычным `cargo test` без systemd, polkit и прав.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::power::{InhibitionGuard, PowerError};

/// Захватывает лок, который существует только внутри процесса.
///
/// Бэкенд в отчёте — тот, который выбрала бы эта машина: fake заменяет только
/// носитель лока, а не способ диагностики.
pub(crate) fn acquire(_why: &str, _ttl_secs: u64) -> Result<InhibitionGuard, PowerError> {
    let backend = super::PowerInhibit::backend();
    Ok(InhibitionGuard::new_fake(
        backend,
        Arc::new(AtomicBool::new(true)),
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::*;
    use crate::power::Carrier;

    #[test]
    fn acquire_and_release_toggle_the_lock() {
        let mut guard = acquire("unit test", 60).expect("fake всегда доступен");
        assert!(guard.is_alive());
        guard.release();
        assert!(!guard.is_alive());
        // Идемпотентность: повторный release не меняет состояние и не паникует.
        guard.release();
        assert!(!guard.is_alive());
    }

    #[test]
    fn drop_releases_the_fake_lock() {
        let flag = {
            let guard = acquire("unit test", 60).expect("fake всегда доступен");
            let flag = match &guard.carrier {
                Carrier::Fake(flag) => Arc::clone(flag),
                _ => panic!("fake обязан держать флаг в памяти"),
            };
            assert!(flag.load(Ordering::Relaxed));
            drop(guard);
            flag
        };
        assert!(!flag.load(Ordering::Relaxed), "Drop обязан снять лок");
    }
}
