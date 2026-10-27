//! Глобальный снимок Radio Browser для карты стран (ADR-0035).
//!
//! «Плотность» считается по счётчикам `/json/countrycodes`: один запрос на
//! ~8 КБ закрывает все страны базы, поэтому карта закрашена сразу, без выбора
//! в панели. Остальным метрикам нужны поля станций, и они берут мировой топ по
//! кликам; когда в панели есть своя выборка, метрики считаются по ней.

use std::collections::HashMap;

use tokio::sync::mpsc;

use crate::services::{CountryCount, RadioError, Station};

/// Станций в мировом снимке: 1000 записей ≈ 1,1 МБ JSON и десятки стран.
const WORLD_SAMPLE_LIMIT: u32 = 1000;
/// Попыток загрузки снимка: сеть могла быть недоступна в момент старта.
const WORLD_ATTEMPTS: usize = 3;

/// Счётчики стран и мировой топ станций, загруженные один раз за сессию.
#[derive(Default)]
pub(crate) struct WorldBaseline {
    counts: HashMap<String, u32>,
    sample: Vec<Station>,
    receiver: Option<mpsc::UnboundedReceiver<Result<World, RadioError>>>,
    attempts: usize,
    /// Номер изменения снимка: по нему пересобирается тепловая карта.
    revision: u64,
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
        &self.sample
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    fn into_counts(rows: Vec<CountryCount>) -> HashMap<String, u32> {
        rows.into_iter()
            .filter_map(|row| {
                let code = row.name.trim().to_ascii_uppercase();
                (code.len() == 2 && code.bytes().all(|byte| byte.is_ascii_alphabetic()))
                    .then_some((code, row.stationcount))
            })
            .collect()
    }
}

impl super::Session {
    /// Запрашивает снимок мира; повторные вызовы бесплатны, пока есть
    /// незавершённая задача, загруженные данные или исчерпаны попытки.
    pub(crate) fn start_world(&mut self) {
        if self.world.receiver.is_some()
            || !self.world.counts.is_empty()
            || self.world.attempts >= WORLD_ATTEMPTS
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
        match receiver.try_recv() {
            Ok(Ok(world)) => {
                let World { counts, sample } = world;
                self.world.counts = WorldBaseline::into_counts(counts);
                self.world.sample = sample;
                self.world.revision = self.world.revision.wrapping_add(1);
                if self.world.counts.is_empty() {
                    tracing::debug!("снимок мира пуст — карта стран останется без заливки");
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
}
