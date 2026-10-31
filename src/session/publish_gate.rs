//! Вердикт яруса 2 для ссылок, уходящих в MPRIS/SMTC/Now Playing
//! (ADR-0025, п. 12).
//!
//! `sync_media` исполняется на главном потоке и внутри `block_on` в
//! `park()`, поэтому резолвить имя там нельзя. Гейт держит кэш фоновых
//! вердиктов: первый кадр после смены станции публикует пустое поле, задача
//! резолва обновляет вердикт и будит UI. Истёкший вердикт продолжает
//! действовать, пока идёт повторный резолв (stale-while-revalidate) — иначе
//! `artUrl` мигал бы каждую минуту.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::services::{public_verdict, publishable};

use super::WakeHandle;

/// Срок жизни вердикта: по истечении адрес резолвится заново.
const VERDICT_TTL: Duration = Duration::from_secs(60);
/// Потолок кэша: станции меняются, но старые вердикты копиться не должны.
const MAX_VERDICTS: usize = 64;

/// Разделяемый гейт публикации: дешёвый `Clone`.
#[derive(Clone)]
pub(crate) struct PublishGate {
    verdicts: Arc<Mutex<HashMap<String, Verdict>>>,
    runtime: tokio::runtime::Handle,
    wake: WakeHandle,
}

struct Verdict {
    /// `None` — резолв уже идёт, публиковать пока нечего.
    public: Option<bool>,
    checked_at: Instant,
}

impl PublishGate {
    pub(crate) fn new(runtime: tokio::runtime::Handle, wake: WakeHandle) -> Self {
        Self {
            verdicts: Arc::new(Mutex::new(HashMap::new())),
            runtime,
            wake,
        }
    }

    /// `true` — ссылка прошла ярус 1 и подтверждённый ярус 2. Неизвестная,
    /// протухшая без прошлого решения или синтаксически отклонённая ссылка в
    /// этот кадр не публикуется; вердикт считается в фоне.
    pub(crate) fn permits(&self, reference: &str) -> bool {
        if publishable(reference).is_none() {
            return false;
        }
        let mut verdicts = lock(&self.verdicts);
        let (publish, refresh) = match verdicts.get(reference) {
            Some(entry) if entry.checked_at.elapsed() < VERDICT_TTL => match entry.public {
                Some(public) => (public, false),
                None => (false, false),
            },
            // Протухло: прошлое решение действует, пока идёт повторный резолв.
            Some(entry) => (entry.public == Some(true), true),
            None => (false, true),
        };
        if refresh {
            let previous = verdicts.get(reference).and_then(|entry| entry.public);
            if previous.is_none() && verdicts.len() >= MAX_VERDICTS {
                evict(&mut verdicts);
            }
            verdicts.insert(
                reference.to_owned(),
                Verdict {
                    public: previous,
                    checked_at: Instant::now(),
                },
            );
        }
        drop(verdicts);
        if refresh {
            self.spawn(reference.to_owned());
        }
        publish
    }

    /// Фоновый резолв: обновляет вердикт и будит UI, чтобы `sync_media`
    /// опубликовал поле в следующем кадре.
    fn spawn(&self, reference: String) {
        let verdicts = Arc::clone(&self.verdicts);
        let wake = self.wake.waker();
        self.runtime.spawn(async move {
            let public = public_verdict(&reference).await;
            if let Ok(mut map) = verdicts.lock() {
                map.insert(
                    reference,
                    Verdict {
                        public: Some(public),
                        checked_at: Instant::now(),
                    },
                );
            }
            wake();
        });
    }
}

fn evict(verdicts: &mut HashMap<String, Verdict>) {
    let oldest = verdicts
        .iter()
        .min_by_key(|(_, entry)| entry.checked_at)
        .map(|(reference, _)| reference.clone());
    if let Some(reference) = oldest {
        verdicts.remove(&reference);
    }
}

fn lock<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate() -> PublishGate {
        PublishGate::new(tokio::runtime::Handle::current(), WakeHandle::default())
    }

    /// Первый кадр никогда не ждёт резолва: публикуется пусто, вердикт
    /// приходит в следующем кадре и только тогда разрешает поле.
    #[tokio::test]
    async fn verdict_arrives_on_a_later_frame() {
        let gate = gate();
        let reference = "https://93.184.216.34/icon.png";
        assert!(
            !gate.permits(reference),
            "резолв не должен останавливать главный поток"
        );
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while !gate.permits(reference) {
            assert!(
                tokio::time::Instant::now() < deadline,
                "фоновый вердикт не пришёл"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Приватная цель не публикуется никогда: и первый кадр, и после вердикта.
    #[tokio::test]
    async fn private_target_stays_unpublished() {
        let gate = gate();
        for reference in [
            "http://127.0.0.1/icon.png",
            "https://192.168.1.10/icon.png",
            "https://169.254.169.254/latest/meta-data/",
            "https://localhost/icon.png",
        ] {
            assert!(!gate.permits(reference), "{reference}");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        for reference in [
            "http://127.0.0.1/icon.png",
            "https://192.168.1.10/icon.png",
            "https://169.254.169.254/latest/meta-data/",
            "https://localhost/icon.png",
        ] {
            assert!(!gate.permits(reference), "{reference} после вердикта");
        }
    }

    #[tokio::test]
    async fn tier_one_rejection_does_not_spawn_a_resolution() {
        let gate = gate();
        assert!(!gate.permits("file:///etc/passwd"));
        assert!(!gate.permits("не url"));
        assert!(!gate.permits(""));
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(lock(&gate.verdicts).is_empty(), "ярус 1 не дошёл до кэша");
    }
}
