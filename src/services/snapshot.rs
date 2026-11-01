//! Офлайн-снимок мирового топа станций (ADR-0043).
//!
//! Каталог Radio Browser недоступен без сети, поэтому последний удачный снимок
//! остаётся на диске: `$XDG_DATA_HOME/orbitfm/cache/stations-snapshot.json`.
//! Файл — кэш, а не конфиг: ошибки чтения и разбора молча отбрасываются, запись
//! атомарная, размер ограничен. Ссылки проверяются тем же ярусом SSRF, что и
//! эфир: снимок не должен воскрешать приватный адрес (ADR-0025).

use std::collections::HashSet;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::artwork::publishable;
use super::radio_browser::{Station, StationSource};
use crate::text::single_line;

const SCHEMA: u32 = 1;
const DATA_DIR: &str = "orbitfm";
const CACHE_DIR: &str = "cache";
const FILE_NAME: &str = "stations-snapshot.json";
const TMP_FILE_NAME: &str = "stations-snapshot.json.tmp";

/// Станций в снимке: 500 записей с координатами покрывают карту.
pub(crate) const STATIONS_LIMIT: usize = 500;
/// Жёсткий потолок файла: снимок — кэш, а не второй каталог.
pub(crate) const FILE_LIMIT: usize = 512 * 1024;
/// Снимок старше суток устарел, но остаётся пригодным для показа.
pub(crate) const TTL_SECONDS: u64 = 24 * 60 * 60;

const NAME_LIMIT: usize = 120;
const TAG_LIMIT: usize = 120;
const PLACE_LIMIT: usize = 64;
const URL_LIMIT: usize = 512;
const CODEC_LIMIT: usize = 32;

#[derive(Debug, Serialize, Deserialize)]
struct SnapshotFile {
    schema: u32,
    #[serde(default)]
    saved_at: u64,
    #[serde(default)]
    stations: Vec<SnapshotStation>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SnapshotStation {
    uuid: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    countrycode: String,
    #[serde(default)]
    country: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    tags: String,
    #[serde(default)]
    bitrate: u32,
    #[serde(default)]
    codec: String,
    #[serde(default)]
    votes: u32,
    lat: f64,
    lon: f64,
}

/// Прочитанный снимок: станции и время записи.
pub(crate) struct LoadedSnapshot {
    pub(crate) stations: Vec<Station>,
    pub(crate) saved_at: u64,
}

impl LoadedSnapshot {
    pub(crate) fn stale_at(&self, now: u64) -> bool {
        now.saturating_sub(self.saved_at) > TTL_SECONDS
    }
}

pub(crate) fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// Отбирает записи для снимка: публичный поток, координаты и голоса.
/// Сортировка по голосам, чтобы топ-500 не зависел от порядка ответа сервера.
pub(crate) fn select(stations: &[Station]) -> Vec<Station> {
    let mut seen = HashSet::new();
    let mut selected: Vec<Station> = stations
        .iter()
        .filter(|station| station.votes > 0)
        .filter(|station| seen.insert(station.stationuuid.as_str()))
        .filter(|station| usable(station))
        .cloned()
        .collect();
    selected.sort_by(|left, right| {
        right
            .votes
            .cmp(&left.votes)
            .then_with(|| left.stationuuid.cmp(&right.stationuuid))
    });
    selected.truncate(STATIONS_LIMIT);
    selected
}

fn usable(station: &Station) -> bool {
    let url = station.url_resolved.trim();
    !url.is_empty()
        && publishable(url).is_some()
        && coords(station.geo_lat, station.geo_long).is_some()
}

/// Координаты в допустимом диапазоне; `(0, 0)` — заглушка Radio Browser.
fn coords(lat: Option<f64>, lon: Option<f64>) -> Option<(f64, f64)> {
    let (lat, lon) = (lat?, lon?);
    if !lat.is_finite() || !lon.is_finite() {
        return None;
    }
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    if lat == 0.0 && lon == 0.0 {
        return None;
    }
    Some((lat, lon))
}

/// Сохраняет снимок из мирового топа. Ошибки пути и записи возвращаются
/// вызывающему: снимок необязателен, но сбой должен попасть в лог.
pub(crate) fn save_default(stations: &[Station]) -> io::Result<()> {
    let path =
        default_path().ok_or_else(|| io::Error::other("не удалось определить путь снимка"))?;
    save(&path, stations)
}

pub(crate) fn save(path: &Path, stations: &[Station]) -> io::Result<()> {
    let selected = select(stations);
    if selected.is_empty() {
        return Err(io::Error::other("в снимке нет пригодных станций"));
    }

    let mut payload = SnapshotFile {
        schema: SCHEMA,
        saved_at: now_seconds(),
        stations: selected.iter().filter_map(to_record).collect(),
    };
    let mut bytes = serialize(&payload)?;
    // Имена и теги обрезаны, но экранирование JSON всё равно может раздуть
    // файл: режем выборку, пока она не влезет в потолок.
    while bytes.len() > FILE_LIMIT && payload.stations.len() > 1 {
        let keep = payload.stations.len() * 3 / 4;
        payload.stations.truncate(keep);
        bytes = serialize(&payload)?;
    }
    if bytes.len() > FILE_LIMIT {
        return Err(io::Error::other("снимок не влезает в потолок файла"));
    }
    crate::storage::write_atomic(path, TMP_FILE_NAME, &bytes)
}

fn serialize(payload: &SnapshotFile) -> io::Result<Vec<u8>> {
    serde_json::to_vec(payload).map_err(io::Error::other)
}

fn to_record(station: &Station) -> Option<SnapshotStation> {
    let (lat, lon) = coords(station.geo_lat, station.geo_long)?;
    let url = station.url_resolved.trim();
    if url.is_empty() || publishable(url).is_none() {
        return None;
    }
    Some(SnapshotStation {
        uuid: field(&station.stationuuid, 64),
        name: field(&station.name, NAME_LIMIT),
        url: clip(url, URL_LIMIT).to_owned(),
        countrycode: field(&station.countrycode, 2),
        country: field(&station.country, PLACE_LIMIT),
        state: field(&station.state, PLACE_LIMIT),
        tags: field(&station.tags, TAG_LIMIT),
        bitrate: station.bitrate,
        codec: field(&station.codec, CODEC_LIMIT),
        votes: station.votes,
        lat,
        lon,
    })
}

/// Однострочное поле с потолком длины: управляющие символы чужих метаданных
/// не должны попадать даже в локальный кэш.
fn field(value: &str, limit: usize) -> String {
    clip(single_line(value).trim(), limit).to_owned()
}

fn to_station(record: &SnapshotStation) -> Option<Station> {
    let uuid = record.uuid.trim();
    let url = record.url.trim();
    if uuid.is_empty() || publishable(url).is_none() {
        return None;
    }
    let (lat, lon) = coords(Some(record.lat), Some(record.lon))?;
    Some(Station {
        stationuuid: uuid.to_owned(),
        name: record.name.clone(),
        url: url.to_owned(),
        url_resolved: url.to_owned(),
        homepage: None,
        favicon: None,
        tags: record.tags.clone(),
        countrycode: record.countrycode.trim().to_ascii_uppercase(),
        language: String::new(),
        votes: record.votes,
        country: record.country.clone(),
        state: record.state.clone(),
        bitrate: record.bitrate,
        codec: record.codec.clone(),
        geo_lat: Some(lat),
        geo_long: Some(lon),
        source: StationSource::Radio,
    })
}

/// Читает снимок; любая ошибка означает «снимка нет», а не отказ приложения.
pub(crate) fn load_default() -> Option<LoadedSnapshot> {
    load(&default_path()?)
}

pub(crate) fn load(path: &Path) -> Option<LoadedSnapshot> {
    let metadata = fs::metadata(path).ok()?;
    if metadata.len() > FILE_LIMIT as u64 {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    let payload: SnapshotFile = serde_json::from_slice(&bytes).ok()?;
    if payload.schema != SCHEMA {
        return None;
    }

    let mut seen = HashSet::new();
    let mut stations: Vec<Station> = payload
        .stations
        .iter()
        .filter_map(to_station)
        .filter(|station| seen.insert(station.stationuuid.clone()))
        .collect();
    stations.sort_by(|left, right| {
        right
            .votes
            .cmp(&left.votes)
            .then_with(|| left.stationuuid.cmp(&right.stationuuid))
    });
    stations.truncate(STATIONS_LIMIT);
    if stations.is_empty() {
        return None;
    }
    Some(LoadedSnapshot {
        stations,
        saved_at: payload.saved_at,
    })
}

/// Удаляет снимок по стандартному пути; отсутствующий файл — успех.
pub(crate) fn clear_default() -> io::Result<()> {
    let path =
        default_path().ok_or_else(|| io::Error::other("не удалось определить путь снимка"))?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

/// `$XDG_DATA_HOME/orbitfm/cache/stations-snapshot.json` с откатом на `HOME`.
pub(crate) fn default_path() -> Option<PathBuf> {
    default_path_from(env::var_os("XDG_DATA_HOME"), env::var_os("HOME"))
}

fn default_path_from(xdg_data_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    if let Some(xdg) = xdg_data_home {
        let path = PathBuf::from(xdg);
        if path.is_absolute() {
            return Some(path.join(DATA_DIR).join(CACHE_DIR).join(FILE_NAME));
        }
    }
    let home = PathBuf::from(home?);
    if !home.is_absolute() {
        return None;
    }
    Some(
        home.join(".local")
            .join("share")
            .join(DATA_DIR)
            .join(CACHE_DIR)
            .join(FILE_NAME),
    )
}

fn clip(value: &str, limit: usize) -> &str {
    value
        .char_indices()
        .nth(limit)
        .map_or(value, |(index, _)| &value[..index])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn station(uuid: &str, code: &str, votes: u32, lat: f64, lon: f64) -> Station {
        Station {
            stationuuid: uuid.to_owned(),
            name: format!("Station {uuid}"),
            url: "https://example.com/live".to_owned(),
            url_resolved: "https://example.com/live".to_owned(),
            homepage: None,
            favicon: None,
            tags: "jazz".to_owned(),
            countrycode: code.to_owned(),
            language: "english".to_owned(),
            votes,
            country: "Germany".to_owned(),
            state: "Berlin".to_owned(),
            bitrate: 128,
            codec: "MP3".to_owned(),
            geo_lat: Some(lat),
            geo_long: Some(lon),
            source: StationSource::Radio,
        }
    }

    #[cfg(unix)]
    #[test]
    fn path_prefers_absolute_xdg_data_home() {
        let path = default_path_from(Some("/tmp/xdg".into()), Some("/home/user".into())).unwrap();
        assert_eq!(
            path,
            PathBuf::from("/tmp/xdg/orbitfm/cache/stations-snapshot.json")
        );
    }

    #[cfg(unix)]
    #[test]
    fn path_falls_back_to_home_for_relative_xdg() {
        let path = default_path_from(Some("relative".into()), Some("/home/user".into())).unwrap();
        assert_eq!(
            path,
            PathBuf::from("/home/user/.local/share/orbitfm/cache/stations-snapshot.json")
        );
        assert!(default_path_from(None, None).is_none());
        assert!(default_path_from(None, Some("relative".into())).is_none());
    }

    #[test]
    fn roundtrip_keeps_the_top_voted_public_stations() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cache/stations-snapshot.json");
        let mut private = station("private", "DE", 900, 52.52, 13.405);
        private.url_resolved = "http://127.0.0.1/stream".to_owned();
        let mut groundless = station("groundless", "DE", 800, 52.52, 13.405);
        groundless.geo_lat = None;
        let mut silent = station("silent", "DE", 0, 52.52, 13.405);
        silent.votes = 0;
        let first = station("first", "DE", 500, 52.52, 13.405);
        let second = station("second", "FR", 700, 48.85, 2.35);

        save(&path, &[first, private, groundless, silent, second.clone()]).expect("снимок записан");

        let loaded = load(&path).expect("снимок читается");
        assert_eq!(loaded.stations.len(), 2);
        assert_eq!(loaded.stations[0].stationuuid, "second");
        assert_eq!(loaded.stations[1].stationuuid, "first");
        assert_eq!(loaded.stations[0].geo_lat, Some(48.85));
        assert_eq!(loaded.stations[0].url_resolved, second.url_resolved);
        assert!(loaded.saved_at > 0);
    }

    #[test]
    fn snapshot_is_capped_at_five_hundred_and_keeps_the_most_voted() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snapshot.json");
        let stations: Vec<Station> = (0..(STATIONS_LIMIT + 20))
            .map(|index| {
                station(
                    &format!("uuid-{index}"),
                    "DE",
                    index as u32 + 1,
                    52.52,
                    13.405,
                )
            })
            .collect();

        save(&path, &stations).expect("снимок записан");
        let loaded = load(&path).expect("снимок читается");

        assert_eq!(loaded.stations.len(), STATIONS_LIMIT);
        assert_eq!(loaded.stations[0].votes, (STATIONS_LIMIT + 20) as u32);
        assert_eq!(
            loaded.stations[0].stationuuid,
            format!("uuid-{}", STATIONS_LIMIT + 19)
        );
    }

    #[test]
    fn corrupt_or_foreign_snapshots_are_ignored() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snapshot.json");

        fs::write(&path, b"{ not json").unwrap();
        assert!(load(&path).is_none());

        fs::write(&path, br#"{"schema":2,"saved_at":1,"stations":[]}"#).unwrap();
        assert!(load(&path).is_none());

        assert!(load(&directory.path().join("missing.json")).is_none());
    }

    #[test]
    fn snapshot_rechecks_urls_when_loading() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snapshot.json");
        let json = r#"{
            "schema": 1,
            "saved_at": 100,
            "stations": [
                {"uuid":"bad","name":"Bad","url":"http://127.0.0.1/x","lat":1.0,"lon":2.0},
                {"uuid":"ok","name":"Ok","url":"https://example.com/live","lat":1.0,"lon":2.0}
            ]
        }"#;
        fs::write(&path, json).unwrap();

        let loaded = load(&path).expect("снимок читается");
        assert_eq!(loaded.stations.len(), 1);
        assert_eq!(loaded.stations[0].stationuuid, "ok");
    }

    #[test]
    fn records_are_clipped_to_keep_the_cache_small() {
        let mut long = station("long", "DE", 10, 52.52, 13.405);
        long.name = "Я".repeat(400);
        long.tags = "tag,".repeat(200);
        let record = to_record(&long).expect("запись строится");

        assert_eq!(record.name.chars().count(), NAME_LIMIT);
        assert_eq!(record.tags.chars().count(), TAG_LIMIT);
    }

    #[test]
    fn stale_snapshot_is_reported_but_still_loaded() {
        let loaded = LoadedSnapshot {
            stations: vec![station("a", "DE", 1, 52.52, 13.405)],
            saved_at: 100,
        };
        assert!(!loaded.stale_at(100 + TTL_SECONDS));
        assert!(loaded.stale_at(100 + TTL_SECONDS + 1));
    }
}
