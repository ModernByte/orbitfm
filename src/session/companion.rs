//! Обслуживание companion-попапа браузерных расширений (ADR-0038, ревизия 0.16).
//!
//! Попап ищет и играет станции, не открывая окно. Его выдача живёт отдельно от
//! видимого поиска: запрос попапа не перерисовывает панель и не трогает маркеры
//! глобуса. Ответ уходит кадром `search` в поток `events`; устаревшие поколения
//! отбрасываются, чтобы быстрый ввод не показывал чужой результат.

use tokio::sync::mpsc;
use tracing::warn;

use crate::library::StationLibrary;
use crate::services::{RadioError, SearchParams, Station};

/// Станций в выдаче попапа: больше не помещается в прокручиваемый список.
pub(crate) const COMPANION_RESULT_LIMIT: usize = 20;

const NAME_LIMIT: usize = 96;
const COUNTRY_LIMIT: usize = 48;
const TAGS_LIMIT: usize = 80;

/// Состояние поиска попапа: своё поколение, своя выдача, своя ошибка.
#[derive(Debug, Default)]
pub(crate) struct CompanionSearchState {
    pub(crate) query: String,
    pub(crate) generation: u64,
    pub(crate) results: Vec<Station>,
    pub(crate) rx: Option<mpsc::UnboundedReceiver<CompanionResponse>>,
    pub(crate) error: Option<String>,
}

pub(crate) struct CompanionResponse {
    generation: u64,
    result: Result<Vec<Station>, RadioError>,
}

/// Компактная станция в кадрах попапа и ответе `library`: длинные поля
/// обрезаны, чтобы 20+20 записей гарантированно уложились в 64 KiB.
#[derive(Debug, serde::Serialize)]
pub(crate) struct CompactStation {
    uuid: String,
    name: String,
    country: String,
    countrycode: String,
    tags: String,
    bitrate: u32,
    source: &'static str,
}

impl CompactStation {
    fn new(station: &Station) -> Self {
        Self {
            uuid: clip(&station.stationuuid, 64),
            name: clip(&station.name, NAME_LIMIT),
            country: clip(&station.country, COUNTRY_LIMIT),
            countrycode: clip(&station.countrycode, 8),
            tags: clip(&station.tags, TAGS_LIMIT),
            bitrate: station.bitrate,
            source: station.source.as_str(),
        }
    }
}

#[derive(serde::Serialize)]
struct LibraryPayload {
    favorites: Vec<CompactStation>,
    history: Vec<CompactStation>,
}

impl super::Session {
    /// Запускает поиск попапа и возвращает поколение для ответа `search <id>`.
    pub(crate) fn start_companion_search(&mut self, query: &str) -> u64 {
        self.companion.generation = self.companion.generation.wrapping_add(1);
        let generation = self.companion.generation;
        self.companion.query = query.to_owned();
        self.companion.results.clear();
        self.companion.error = None;

        let (sender, receiver) = mpsc::unbounded_channel();
        self.companion.rx = Some(receiver);

        let radio = self.radio.clone();
        let waker = self.wake.waker();
        let params = SearchParams {
            name: Some(query.to_owned()),
            limit: COMPANION_RESULT_LIMIT as u32,
            hide_broken: true,
            ..SearchParams::default()
        };
        self.runtime.spawn(async move {
            let result = radio.search(params).await;
            if let Err(err) = &result {
                warn!(error = %err, "поиск попапа не удался");
            }
            let _ = sender.send(CompanionResponse { generation, result });
            waker();
        });
        generation
    }

    /// Забирает готовый ответ поиска попапа при открытом окне.
    pub(crate) fn poll_companion_search(&mut self) {
        let received = {
            let Some(receiver) = self.companion.rx.as_mut() else {
                return;
            };
            match receiver.try_recv() {
                Ok(response) => Some(response),
                Err(mpsc::error::TryRecvError::Empty) => return,
                Err(mpsc::error::TryRecvError::Disconnected) => None,
            }
        };
        self.apply_companion_response(received);
    }

    /// Применяет ответ поиска попапа и публикует кадр `search`. Общий путь для
    /// тика окна и припаркованного простоя: поиск обязан завершаться и без окна.
    pub(crate) fn apply_companion_response(&mut self, received: Option<CompanionResponse>) {
        self.companion.rx = None;
        let query = self.companion.query.clone();

        let Some(response) = received else {
            self.companion.results.clear();
            self.companion.error = Some("поиск прерван".to_owned());
            self.publish_search_frame(&query, Some("поиск прерван"));
            return;
        };
        if response.generation != self.companion.generation {
            tracing::debug!(
                generation = response.generation,
                current = self.companion.generation,
                "устаревший ответ поиска попапа отброшен"
            );
            return;
        }

        match response.result {
            Ok(mut stations) => {
                stations.truncate(COMPANION_RESULT_LIMIT);
                self.companion.results = stations;
                self.companion.error = None;
                self.publish_search_frame(&query, None);
            }
            Err(err) => {
                let message = err.to_string();
                self.companion.results.clear();
                self.companion.error = Some(message.clone());
                self.publish_search_frame(&query, Some(&message));
            }
        }
    }

    fn publish_search_frame(&self, query: &str, error: Option<&str>) {
        let frame = search_frame(
            self.companion.generation,
            query,
            &self.companion.results,
            error,
        );
        self.search_feed.publish(frame);
    }

    /// `favorite [uuid]`: без UUID — текущая станция, с UUID — известная
    /// станция из эфира, выдачи попапа, избранного или истории.
    pub(crate) fn favorite_station(&mut self, station_uuid: Option<&str>) -> Result<bool, String> {
        let station = match station_uuid {
            None => self
                .active_station()
                .cloned()
                .ok_or_else(|| "нет активной станции".to_owned())?,
            Some(uuid) => self
                .known_station(uuid)
                .ok_or_else(|| "станция не найдена".to_owned())?,
        };
        self.toggle_favorite(&station)
    }

    /// Играет только строку последней выдачи попапа: произвольный UUID из
    /// расширения не должен порождать сетевой запрос.
    pub(crate) fn play_companion_result(&mut self, station_uuid: &str) -> Result<(), String> {
        let station = self
            .companion
            .results
            .iter()
            .find(|station| station.stationuuid == station_uuid)
            .cloned()
            .ok_or_else(|| "станция не найдена в результатах поиска".to_owned())?;
        self.start_station(station).map_err(|err| err.to_string())
    }

    /// Играет станцию только из избранного или истории — без сети.
    pub(crate) fn play_library_station(&mut self, station_uuid: &str) -> Result<(), String> {
        let station = self
            .library
            .favorites()
            .iter()
            .chain(self.library.history())
            .find(|station| station.stationuuid == station_uuid)
            .cloned()
            .ok_or_else(|| "станция не найдена в библиотеке".to_owned())?;
        self.start_station(station).map_err(|err| err.to_string())
    }

    fn known_station(&self, station_uuid: &str) -> Option<Station> {
        self.current_station
            .as_ref()
            .filter(|station| station.stationuuid == station_uuid)
            .or_else(|| {
                self.companion
                    .results
                    .iter()
                    .find(|station| station.stationuuid == station_uuid)
            })
            .or_else(|| {
                self.library
                    .favorites()
                    .iter()
                    .chain(self.library.history())
                    .find(|station| station.stationuuid == station_uuid)
            })
            .cloned()
    }

    /// Ответ `library`: избранное и история по 20 записей компактными полями.
    pub(crate) fn library_json(&self) -> String {
        serde_json::to_string(&library_payload(&self.library))
            .unwrap_or_else(|_| "{\"favorites\":[],\"history\":[]}".to_owned())
    }
}

fn library_payload(library: &StationLibrary) -> LibraryPayload {
    LibraryPayload {
        favorites: library
            .favorites()
            .iter()
            .take(COMPANION_RESULT_LIMIT)
            .map(CompactStation::new)
            .collect(),
        history: library
            .history()
            .iter()
            .take(COMPANION_RESULT_LIMIT)
            .map(CompactStation::new)
            .collect(),
    }
}

/// Кадр `search` без повторного разбора: поля компактной станции уже
/// сериализуются `serde_json`.
pub(crate) fn search_frame(
    generation: u64,
    query: &str,
    results: &[Station],
    error: Option<&str>,
) -> String {
    let results: Vec<CompactStation> = results
        .iter()
        .take(COMPANION_RESULT_LIMIT)
        .map(CompactStation::new)
        .collect();
    serde_json::json!({
        "ev": 1,
        "type": "search",
        "id": generation,
        "query": query,
        "results": results,
        "error": error,
    })
    .to_string()
}

fn clip(value: &str, limit: usize) -> String {
    let line = crate::ui::single_line(value);
    if line.chars().count() <= limit {
        line
    } else {
        let mut clipped: String = line.chars().take(limit.saturating_sub(1)).collect();
        clipped.push('…');
        clipped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::markers::sample_station;

    #[test]
    fn search_frame_carries_envelope_and_compact_station() {
        let mut station = sample_station();
        station.source = crate::services::StationSource::Stream;
        station.name = format!("{}\nLive", "x".repeat(200));
        let frame = search_frame(7, "jazz", &[station], None);
        let value: serde_json::Value = serde_json::from_str(&frame).expect("json");
        assert_eq!(value["ev"], 1);
        assert_eq!(value["type"], "search");
        assert_eq!(value["id"], 7);
        assert_eq!(value["query"], "jazz");
        assert!(value["error"].is_null());
        let result = &value["results"][0];
        assert_eq!(result["uuid"], "c685c086-b716-4edd-b313-d4a7baf1084c");
        assert_eq!(result["country"], "United States");
        assert_eq!(result["countrycode"], "US");
        assert_eq!(result["tags"], "jazz");
        assert_eq!(result["bitrate"], 128);
        assert_eq!(result["source"], "stream");
        assert!(
            result["name"]
                .as_str()
                .is_some_and(|name| name.chars().count() <= 96),
            "длинное имя обрезано"
        );
        assert!(!frame.contains('\n'), "кадр — одна строка");
    }

    #[test]
    fn search_frame_reports_errors_and_caps_results() {
        let stations: Vec<_> = (0..30)
            .map(|index| {
                let mut station = sample_station();
                station.stationuuid = format!("uuid-{index}");
                station
            })
            .collect();
        let frame = search_frame(3, "rock", &stations, Some("сеть недоступна"));
        let value: serde_json::Value = serde_json::from_str(&frame).expect("json");
        assert_eq!(value["results"].as_array().map(Vec::len), Some(20));
        assert_eq!(value["error"], "сеть недоступна");
    }

    #[test]
    fn library_payload_keeps_twenty_of_each_and_stays_small() {
        let mut library = StationLibrary::ephemeral();
        for index in 0..40 {
            let mut station = sample_station();
            station.stationuuid = format!("uuid-{index}");
            station.name = "Очень длинное название станции ".repeat(8);
            station.tags = "tag, ".repeat(40);
            if index % 2 == 0 {
                library.toggle_favorite(&station).expect("favorite");
            }
            library.record_played(&station).expect("history");
        }
        let payload = library_payload(&library);
        let json = serde_json::to_string(&payload).expect("json");
        assert_eq!(payload.favorites.len(), 20);
        assert_eq!(payload.history.len(), 20);
        assert!(
            json.len() < 64 * 1024,
            "размер ответа library: {}",
            json.len()
        );
    }
}
