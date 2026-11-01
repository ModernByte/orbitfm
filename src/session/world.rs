//! Глобальный снимок Radio Browser для карты стран (ADR-0035).
//!
//! «Плотность» считается по счётчикам `/json/countrycodes`: один запрос на
//! ~8 КБ закрывает все страны базы, поэтому карта закрашена сразу, без выбора
//! в панели. Остальным метрикам нужны поля станций, и они берут мировой топ по
//! кликам; когда в панели есть своя выборка, метрики считаются по ней.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::services::snapshot::{self, LoadedSnapshot};
use crate::services::{CountryCount, RadioError, Station};

use super::notice::NoticeKey;

/// Станций в мировом снимке: 1000 записей ≈ 1,1 МБ JSON и десятки стран.
const WORLD_SAMPLE_LIMIT: u32 = 1000;
/// Попыток загрузки снимка: сеть могла быть недоступна в момент старта.
const WORLD_ATTEMPTS: usize = 3;

/// Счётчики стран и мировой топ станций, загруженные один раз за сессию.
#[derive(Default)]
pub(crate) struct WorldBaseline {
    counts: HashMap<String, u32>,
    sample: Arc<Vec<Station>>,
    receiver: Option<mpsc::UnboundedReceiver<Result<World, RadioError>>>,
    attempts: usize,
    /// Номер изменения снимка: по нему пересобирается тепловая карта.
    revision: u64,
    /// Живой ответ Radio Browser уже принят: снимок больше не подставляется.
    live: bool,
    /// На карте сейчас данные локального снимка (ADR-0043).
    offline: bool,
    /// Время записи снимка: свежий файл не переписывается на каждом старте.
    saved_at: Option<u64>,
    /// Пользователь очистил снимок: до конца сеанса не сохраняем его заново.
    snapshot_disabled: bool,
}

/// Ответ двух независимых запросов: любая половина могла не прийти.
struct World {
    counts: Vec<CountryCount>,
    sample: Vec<Station>,
}

impl WorldBaseline {
    pub(crate) fn counts(&self) -> &HashMap<String, u32> {
        &self.counts
    }

    pub(crate) fn sample(&self) -> &[Station] {
        self.sample.as_slice()
    }

    /// Дешёвый клон для фонового поиска: снимок не копируется по станциям.
    pub(crate) fn sample_shared(&self) -> Arc<Vec<Station>> {
        Arc::clone(&self.sample)
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    /// Карта показывает локальный снимок, а не живой каталог (ADR-0043).
    pub(crate) fn offline(&self) -> bool {
        self.offline
    }

    fn into_counts(rows: Vec<CountryCount>) -> HashMap<String, u32> {
        rows.into_iter()
            .filter_map(|row| iso_code(&row.name).map(|code| (code, row.stationcount)))
            .collect()
    }

    /// Счётчики офлайн-снимка: это число станций в снимке, а не во всей базе.
    fn counts_from_stations(stations: &[Station]) -> HashMap<String, u32> {
        let mut counts = HashMap::new();
        for station in stations {
            if let Some(code) = iso_code(&station.countrycode) {
                *counts.entry(code).or_insert(0) += 1;
            }
        }
        counts
    }

    fn adopt_snapshot(&mut self, loaded: LoadedSnapshot) {
        self.counts = WorldBaseline::counts_from_stations(&loaded.stations);
        self.sample = Arc::new(loaded.stations);
        self.saved_at = Some(loaded.saved_at);
        self.offline = true;
        self.revision = self.revision.wrapping_add(1);
    }
}

fn iso_code(raw: &str) -> Option<String> {
    let code = raw.trim().to_ascii_uppercase();
    (code.len() == 2 && code.bytes().all(|byte| byte.is_ascii_alphabetic())).then_some(code)
}

impl super::Session {
    /// Подставляет последний удачный снимок до первого сетевого запроса
    /// (ADR-0043): карта и маркеры живут даже без сети.
    pub(crate) fn load_snapshot(&mut self) {
        if self.world.live || !self.world.sample.is_empty() {
            return;
        }
        let Some(loaded) = snapshot::load_default() else {
            return;
        };
        let age = snapshot::now_seconds().saturating_sub(loaded.saved_at);
        let stations = loaded.stations.len();
        let stale = loaded.stale_at(snapshot::now_seconds());
        self.world.adopt_snapshot(loaded);
        tracing::debug!(stations, age, stale, "офлайн-снимок загружен");
    }

    /// Запрашивает снимок мира; повторные вызовы бесплатны, пока есть
    /// незавершённая задача, живые данные или исчерпаны попытки.
    pub(crate) fn start_world(&mut self) {
        if self.world.receiver.is_some() || self.world.live || self.world.attempts >= WORLD_ATTEMPTS
        {
            return;
        }

        let (sender, receiver) = mpsc::unbounded_channel();
        self.world.receiver = Some(receiver);
        self.world.attempts += 1;

        let radio = self.radio.clone();
        let waker = self.wake.waker();
        self.runtime.spawn(async move {
            let counts = radio.country_counts();
            let sample = radio.top_stations(WORLD_SAMPLE_LIMIT);
            let (counts, sample) = tokio::join!(counts, sample);
            let counts = counts.unwrap_or_else(|err| {
                tracing::warn!(error = %err, "счётчики стран не пришли");
                Vec::new()
            });
            let sample = sample.unwrap_or_else(|err| {
                tracing::warn!(error = %err, "мировой топ станций не пришёл");
                Vec::new()
            });
            let _ = sender.send(Ok(World { counts, sample }));
            waker();
        });
    }

    /// Забирает готовый снимок; пустой ответ оставляет попытку на следующий кадр.
    pub(crate) fn poll_world(&mut self) {
        let Some(receiver) = self.world.receiver.as_mut() else {
            return;
        };
        let received = receiver.try_recv();
        match received {
            Ok(Ok(world)) => {
                let World { counts, sample } = world;
                let live_counts = WorldBaseline::into_counts(counts);
                let has_live_sample = !sample.is_empty();
                let mut changed = false;
                if !live_counts.is_empty() {
                    self.world.counts = live_counts;
                    changed = true;
                }
                if has_live_sample {
                    self.world.sample = Arc::new(sample);
                    changed = true;
                }
                if changed {
                    self.world.live = true;
                    self.world.offline = false;
                    self.world.revision = self.world.revision.wrapping_add(1);
                }
                if self.world.sample.is_empty() {
                    tracing::debug!("снимок мира пуст — карта стран останется без заливки");
                }
                if has_live_sample {
                    self.save_world_snapshot();
                }
                self.world.receiver = None;
            }
            Ok(Err(err)) => {
                tracing::warn!(error = %err, "снимок мира не загружен");
                self.world.receiver = None;
            }
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                tracing::warn!("задача снимка мира исчезла");
                self.world.receiver = None;
            }
        }
    }

    /// Фоновая запись снимка: сеть уже ответила, диск не тормозит кадр.
    fn save_world_snapshot(&mut self) {
        if self.world.snapshot_disabled {
            return;
        }
        let now = snapshot::now_seconds();
        if let Some(saved_at) = self.world.saved_at
            && now.saturating_sub(saved_at) < snapshot::TTL_SECONDS
        {
            tracing::debug!("офлайн-снимок свежий, перезапись не нужна");
            return;
        }
        let sample = self.world.sample_shared();
        self.world.saved_at = Some(now);
        self.runtime.spawn_blocking(move || {
            if let Err(err) = snapshot::save_default(&sample) {
                tracing::warn!(error = %err, "не удалось сохранить офлайн-снимок");
            }
        });
    }

    /// Удаляет снимок с диска; живые данные сеанса остаются на месте.
    pub(crate) fn clear_offline_snapshot(&mut self) {
        if let Err(err) = snapshot::clear_default() {
            tracing::warn!(error = %err, "не удалось удалить офлайн-снимок");
            self.notify_error_detail(NoticeKey::SnapshotClearFailed, err.to_string());
            return;
        }
        self.world.snapshot_disabled = true;
        self.world.saved_at = None;
        if self.world.offline {
            self.world.counts.clear();
            self.world.sample = Arc::new(Vec::new());
            self.world.offline = false;
            self.world.revision = self.world.revision.wrapping_add(1);
        }
        self.notify_info(NoticeKey::SnapshotCleared);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, stationcount: u32) -> CountryCount {
        CountryCount {
            name: name.to_owned(),
            stationcount,
        }
    }

    #[test]
    fn counts_are_uppercased_and_keyed_by_iso() {
        let counts = WorldBaseline::into_counts(vec![row("de", 4249), row(" ru ", 1700)]);

        assert_eq!(counts.get("DE"), Some(&4249));
        assert_eq!(counts.get("RU"), Some(&1700));
    }

    #[test]
    fn counts_skip_codes_that_are_not_iso_a2() {
        let counts = WorldBaseline::into_counts(vec![
            row("deu", 10),
            row("", 3),
            row("-9", 1),
            row("ok", 7),
        ]);

        assert_eq!(counts.len(), 1);
        assert_eq!(counts.get("OK"), Some(&7));
    }

    #[test]
    fn counts_from_snapshot_follow_station_country_codes() {
        let counts = WorldBaseline::counts_from_stations(&[
            station("a", "de"),
            station("b", "DE"),
            station("c", "fra"),
            station("d", ""),
        ]);

        assert_eq!(counts.get("DE"), Some(&2));
        assert_eq!(counts.len(), 1);
    }

    #[test]
    fn snapshot_seeds_counts_and_marks_the_map_offline() {
        let mut world = WorldBaseline::default();
        world.adopt_snapshot(LoadedSnapshot {
            stations: vec![station("a", "DE"), station("b", "FR")],
            saved_at: 42,
        });

        assert!(world.offline());
        assert_eq!(world.counts().get("DE"), Some(&1));
        assert_eq!(world.sample().len(), 2);
        assert_eq!(world.sample_shared().len(), 2);
        assert_eq!(world.revision(), 1);
        assert_eq!(world.saved_at, Some(42));
    }

    fn station(uuid: &str, code: &str) -> Station {
        let mut station = super::super::markers::sample_station();
        station.stationuuid = uuid.to_owned();
        station.countrycode = code.to_owned();
        station
    }
}
