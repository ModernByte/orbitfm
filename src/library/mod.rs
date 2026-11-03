//! Избранное и история станций с локальной персистентностью.

use std::collections::HashSet;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::services::{Station, publishable};

const DATA_DIR: &str = "orbitfm";
const FILE_NAME: &str = "library.json";
const TMP_FILE_NAME: &str = "library.json.tmp";
const EXPORT_TMP_NAME: &str = "library-export.json.tmp";
/// Сколько последних станций хранит история.
pub const HISTORY_LIMIT: usize = 20;
/// Версия схемы экспорта/импорта библиотеки (docs/interfaces/15-library-export.md).
pub const EXPORT_SCHEMA: u32 = 1;
/// Потолок импортируемого файла: библиотека в разы меньше.
const IMPORT_FILE_LIMIT: u64 = 2 * 1024 * 1024;
/// Потолок числа станций в одном импорте.
const IMPORT_STATIONS_LIMIT: usize = 5000;

/// Ошибки библиотеки станций.
#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("не удалось определить путь библиотеки: {0}")]
    Path(String),
    #[error("не удалось прочитать библиотеку: {0}")]
    Read(String),
    #[error("не удалось разобрать библиотеку: {0}")]
    Parse(String),
    #[error("не удалось сохранить библиотеку: {0}")]
    Write(String),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct LibraryData {
    favorites: Vec<Station>,
    history: Vec<Station>,
}

/// Переносимый снимок библиотеки для desktop → Android и обратно.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct LibraryExport {
    schema: u32,
    app: String,
    exported_at: u64,
    favorites: Vec<Station>,
    history: Vec<Station>,
}

/// Итог слияния импортированного файла с локальной библиотекой.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportSummary {
    pub favorites_added: usize,
    pub history_added: usize,
    pub skipped: usize,
}

/// Избранные станции и история прослушивания.
///
/// Библиотека с `path = None` работает только в памяти: сохранения
/// становятся no-op, что позволяет пережить повреждённый файл без его потери.
#[derive(Debug)]
pub struct StationLibrary {
    path: Option<PathBuf>,
    data: LibraryData,
}

impl StationLibrary {
    /// Загружает библиотеку из файла. Отсутствующий файл — пустая библиотека.
    pub fn load(path: PathBuf) -> Result<Self, LibraryError> {
        let mut data = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<LibraryData>(&bytes)
                .map_err(|err| LibraryError::Parse(format!("{}: {err}", path.display())))?,
            Err(err) if err.kind() == ErrorKind::NotFound => LibraryData::default(),
            Err(err) => {
                return Err(LibraryError::Read(format!("{}: {err}", path.display())));
            }
        };
        normalize(&mut data);
        Ok(Self {
            path: Some(path),
            data,
        })
    }

    /// Загружает библиотеку из стандартного каталога данных XDG.
    pub fn load_default() -> Result<Self, LibraryError> {
        Self::load(default_path()?)
    }

    /// Библиотека без файла: изменения живут до завершения процесса.
    pub fn ephemeral() -> Self {
        Self {
            path: None,
            data: LibraryData::default(),
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn favorites(&self) -> &[Station] {
        &self.data.favorites
    }

    pub fn history(&self) -> &[Station] {
        &self.data.history
    }

    pub fn is_favorite(&self, station_uuid: &str) -> bool {
        self.data
            .favorites
            .iter()
            .any(|station| station.stationuuid == station_uuid)
    }

    /// Добавляет станцию в избранное или убирает её оттуда.
    ///
    /// Возвращает `true`, если станция теперь в избранном.
    pub fn toggle_favorite(&mut self, station: &Station) -> Result<bool, LibraryError> {
        let favorite = if let Some(index) = self
            .data
            .favorites
            .iter()
            .position(|saved| saved.stationuuid == station.stationuuid)
        {
            self.data.favorites.remove(index);
            false
        } else {
            self.data.favorites.push(station.clone());
            true
        };
        self.save()?;
        Ok(favorite)
    }

    /// Запоминает станцию как последнюю прослушанную.
    pub fn record_played(&mut self, station: &Station) -> Result<(), LibraryError> {
        self.data
            .history
            .retain(|saved| saved.stationuuid != station.stationuuid);
        self.data.history.insert(0, station.clone());
        self.data.history.truncate(HISTORY_LIMIT);
        self.save()
    }

    pub fn clear_history(&mut self) -> Result<(), LibraryError> {
        if self.data.history.is_empty() {
            return Ok(());
        }
        self.data.history.clear();
        self.save()
    }

    /// Переносимый JSON схемы 1: избранное и история без локальных путей.
    pub fn export_json(&self) -> Result<String, LibraryError> {
        let export = LibraryExport {
            schema: EXPORT_SCHEMA,
            app: "orbitfm".to_owned(),
            exported_at: unix_now(),
            favorites: self.data.favorites.iter().map(export_station).collect(),
            history: self.data.history.iter().map(export_station).collect(),
        };
        serde_json::to_string_pretty(&export).map_err(|err| LibraryError::Write(err.to_string()))
    }

    /// Пишет экспорт по выбранному пути атомарно; возвращает число избранных.
    ///
    /// Файл содержит stream URL, поэтому на Unix получает права 0600.
    pub fn export_to(&self, path: &Path) -> Result<usize, LibraryError> {
        let path = absolute_path(path)?;
        let json = self.export_json()?;
        crate::storage::write_atomic(&path, EXPORT_TMP_NAME, json.as_bytes())
            .map_err(|err| LibraryError::Write(format!("{}: {err}", path.display())))?;
        private_file(&path)?;
        Ok(self.data.favorites.len())
    }

    /// Читает файл экспорта и сливает его с текущей библиотекой.
    pub fn import_from(&mut self, path: &Path) -> Result<ImportSummary, LibraryError> {
        let path = absolute_path(path)?;
        let metadata = fs::metadata(&path)
            .map_err(|err| LibraryError::Read(format!("{}: {err}", path.display())))?;
        if metadata.len() > IMPORT_FILE_LIMIT {
            return Err(LibraryError::Parse(format!(
                "{}: файл больше {} байт",
                path.display(),
                IMPORT_FILE_LIMIT
            )));
        }
        let bytes = fs::read(&path)
            .map_err(|err| LibraryError::Read(format!("{}: {err}", path.display())))?;
        self.import_bytes(&bytes)
    }

    /// Слияние без обращения к файлу: единица тестирования и будущий Android.
    ///
    /// Недоверенный файл проходит те же проверки, что и станция из сети:
    /// потолок размера и числа записей, санитизация строк, публичный http(s)
    /// без userinfo (ADR-0025). Некорректные станции пропускаются, а не
    /// отменяют весь импорт.
    pub fn import_bytes(&mut self, bytes: &[u8]) -> Result<ImportSummary, LibraryError> {
        if bytes.len() as u64 > IMPORT_FILE_LIMIT {
            return Err(LibraryError::Parse(format!(
                "файл больше {IMPORT_FILE_LIMIT} байт"
            )));
        }
        let export: LibraryExport = serde_json::from_slice(bytes)
            .map_err(|err| LibraryError::Parse(format!("импорт: {err}")))?;
        if export.schema != EXPORT_SCHEMA {
            return Err(LibraryError::Parse(format!(
                "импорт: поддерживается schema {EXPORT_SCHEMA}, получено {}",
                export.schema
            )));
        }
        if !export.app.is_empty() && export.app != "orbitfm" {
            return Err(LibraryError::Parse(format!(
                "импорт: приложение {} не поддерживается",
                export.app
            )));
        }
        if export.favorites.len() + export.history.len() > IMPORT_STATIONS_LIMIT {
            return Err(LibraryError::Parse(format!(
                "импорт: больше {IMPORT_STATIONS_LIMIT} станций"
            )));
        }

        let mut summary = ImportSummary::default();
        let mut favorite_ids: HashSet<String> = self
            .data
            .favorites
            .iter()
            .map(|station| station.stationuuid.clone())
            .collect();
        for station in export.favorites {
            match sanitize_import_station(&station) {
                Some(station) if favorite_ids.insert(station.stationuuid.clone()) => {
                    self.data.favorites.push(station);
                    summary.favorites_added += 1;
                }
                Some(_) => {}
                None => summary.skipped += 1,
            }
        }

        let mut history_ids: HashSet<String> = self
            .data
            .history
            .iter()
            .map(|station| station.stationuuid.clone())
            .collect();
        for station in export.history {
            match sanitize_import_station(&station) {
                Some(station) if history_ids.insert(station.stationuuid.clone()) => {
                    self.data.history.push(station);
                    summary.history_added += 1;
                }
                Some(_) => {}
                None => summary.skipped += 1,
            }
        }

        normalize(&mut self.data);
        if summary.favorites_added > 0 || summary.history_added > 0 {
            self.save()?;
        }
        Ok(summary)
    }

    fn save(&self) -> Result<(), LibraryError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let json = serde_json::to_vec_pretty(&self.data)
            .map_err(|err| LibraryError::Write(err.to_string()))?;
        crate::storage::write_atomic(path, TMP_FILE_NAME, &json)
            .map_err(|err| LibraryError::Write(format!("{}: {err}", path.display())))?;
        private_file(path)
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

fn absolute_path(path: &Path) -> Result<PathBuf, LibraryError> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    env::current_dir()
        .map(|cwd| cwd.join(path))
        .map_err(|err| LibraryError::Path(err.to_string()))
}

/// Ограничивает длину и убирает управляющие символы: строка идёт в файл,
/// который прочитает другая платформа (ADR-0030, `crate::text`).
fn clean(value: &str, max_chars: usize) -> String {
    crate::text::single_line(value)
        .chars()
        .take(max_chars)
        .collect()
}

fn export_station(station: &Station) -> Station {
    let mut station = station.clone();
    station.stationuuid = clean(&station.stationuuid, 64);
    station.name = clean(&station.name, 256);
    station.url = clean(&station.url, 2048);
    station.url_resolved = clean(&station.url_resolved, 2048);
    station.homepage = station
        .homepage
        .as_deref()
        .map(|value| clean(value, 2048))
        .filter(|value| !value.is_empty());
    station.favicon = station
        .favicon
        .as_deref()
        .map(|value| clean(value, 2048))
        .filter(|value| !value.is_empty());
    station.tags = clean(&station.tags, 512);
    station.countrycode = clean(&station.countrycode, 8);
    station.language = clean(&station.language, 128);
    station.country = clean(&station.country, 128);
    station.state = clean(&station.state, 128);
    station.codec = clean(&station.codec, 64);
    station
}

fn sanitize_import_station(station: &Station) -> Option<Station> {
    let stationuuid = clean(&station.stationuuid, 64);
    if stationuuid.is_empty() {
        return None;
    }
    let mut url = clean(&station.url, 2048);
    let mut url_resolved = clean(&station.url_resolved, 2048);
    if url_resolved.is_empty() {
        url_resolved = url.clone();
    }
    if url.is_empty() {
        url = url_resolved.clone();
    }
    let url = public_stream_url(&url)?;
    let url_resolved = public_stream_url(&url_resolved)?;
    let homepage = station
        .homepage
        .as_deref()
        .and_then(|value| public_stream_url(&clean(value, 2048)));
    let favicon = station
        .favicon
        .as_deref()
        .and_then(|value| public_stream_url(&clean(value, 2048)));
    let (geo_lat, geo_long) = match (station.geo_lat, station.geo_long) {
        (Some(lat), Some(long))
            if lat.is_finite()
                && long.is_finite()
                && (-90.0..=90.0).contains(&lat)
                && (-180.0..=180.0).contains(&long) =>
        {
            (Some(lat), Some(long))
        }
        _ => (None, None),
    };
    Some(Station {
        stationuuid,
        name: clean(&station.name, 256),
        url,
        url_resolved,
        homepage,
        favicon,
        tags: clean(&station.tags, 512),
        countrycode: clean(&station.countrycode, 8),
        language: clean(&station.language, 128),
        votes: station.votes,
        country: clean(&station.country, 128),
        state: clean(&station.state, 128),
        bitrate: station.bitrate,
        codec: clean(&station.codec, 64),
        geo_lat,
        geo_long,
        source: station.source,
    })
}

/// Публичный http(s) без учётных данных: та же планка, что у радиостанции
/// перед запуском (ADR-0025). Stream-станция из локальной сети не переносится.
fn public_stream_url(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    let parsed = url::Url::parse(raw).ok()?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return None;
    }
    publishable(raw).map(str::to_owned)
}

#[cfg(unix)]
fn private_file(path: &Path) -> Result<(), LibraryError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|err| LibraryError::Write(format!("{}: {err}", path.display())))
}

#[cfg(not(unix))]
fn private_file(_path: &Path) -> Result<(), LibraryError> {
    Ok(())
}

/// Стандартный путь библиотеки по XDG Base Directory Specification.
pub fn default_path() -> Result<PathBuf, LibraryError> {
    default_path_from(env::var_os("XDG_DATA_HOME"), env::var_os("HOME"))
}

fn default_path_from(
    xdg_data_home: Option<OsString>,
    home: Option<OsString>,
) -> Result<PathBuf, LibraryError> {
    if let Some(xdg) = xdg_data_home {
        let path = PathBuf::from(xdg);
        if path.is_absolute() {
            return Ok(path.join(DATA_DIR).join(FILE_NAME));
        }
    }
    let home = home.ok_or_else(|| LibraryError::Path("HOME не задан".to_owned()))?;
    let home = PathBuf::from(home);
    if !home.is_absolute() {
        return Err(LibraryError::Path(format!(
            "HOME должен быть абсолютным путём: {}",
            home.display()
        )));
    }
    Ok(home
        .join(".local")
        .join("share")
        .join(DATA_DIR)
        .join(FILE_NAME))
}

fn normalize(data: &mut LibraryData) {
    deduplicate(&mut data.favorites);
    deduplicate(&mut data.history);
    data.history.truncate(HISTORY_LIMIT);
}

fn deduplicate(stations: &mut Vec<Station>) {
    let mut seen = HashSet::new();
    stations.retain(|station| seen.insert(station.stationuuid.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn station(uuid: &str) -> Station {
        Station {
            stationuuid: uuid.to_owned(),
            name: format!("Station {uuid}"),
            url: "https://example.com/stream".to_owned(),
            url_resolved: "https://example.com/stream".to_owned(),
            homepage: None,
            favicon: None,
            tags: String::new(),
            countrycode: "DE".to_owned(),
            language: String::new(),
            votes: 50,
            country: "Germany".to_owned(),
            state: "Berlin".to_owned(),
            bitrate: 128,
            codec: "MP3".to_owned(),
            geo_lat: None,
            geo_long: None,
            source: crate::services::StationSource::Radio,
        }
    }

    #[cfg(unix)]
    #[test]
    fn default_path_prefers_absolute_xdg_data_home() {
        let path = default_path_from(Some("/tmp/xdg".into()), Some("/home/user".into())).unwrap();
        assert_eq!(path, PathBuf::from("/tmp/xdg/orbitfm/library.json"));
    }

    #[cfg(unix)]
    #[test]
    fn default_path_falls_back_to_home_for_relative_xdg() {
        let path = default_path_from(Some("relative".into()), Some("/home/user".into())).unwrap();
        assert_eq!(
            path,
            PathBuf::from("/home/user/.local/share/orbitfm/library.json")
        );
        assert!(default_path_from(None, None).is_err());
    }

    #[test]
    fn missing_file_loads_empty_library() {
        let directory = tempfile::tempdir().unwrap();
        let library = StationLibrary::load(directory.path().join("library.json")).unwrap();
        assert!(library.favorites().is_empty());
        assert!(library.history().is_empty());
    }

    #[test]
    fn roundtrip_persists_favorites_and_history() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested/library.json");
        let first = station("11111111-1111-1111-1111-111111111111");
        let second = station("22222222-2222-2222-2222-222222222222");
        {
            let mut library = StationLibrary::load(path.clone()).unwrap();
            library.toggle_favorite(&first).unwrap();
            library.record_played(&second).unwrap();
        }

        let library = StationLibrary::load(path).unwrap();
        assert_eq!(library.favorites(), &[first]);
        assert_eq!(library.history(), &[second]);
    }

    #[test]
    fn toggle_favorite_adds_and_removes_station() {
        let directory = tempfile::tempdir().unwrap();
        let mut library = StationLibrary::load(directory.path().join("library.json")).unwrap();
        let saved = station("11111111-1111-1111-1111-111111111111");

        assert!(library.toggle_favorite(&saved).unwrap());
        assert!(library.is_favorite(&saved.stationuuid));
        assert!(!library.toggle_favorite(&saved).unwrap());
        assert!(!library.is_favorite(&saved.stationuuid));
    }

    #[test]
    fn history_deduplicates_moves_to_front_and_caps() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("library.json");
        let mut library = StationLibrary::load(path.clone()).unwrap();
        let first = station("11111111-1111-1111-1111-111111111111");
        let second = station("22222222-2222-2222-2222-222222222222");

        library.record_played(&first).unwrap();
        library.record_played(&second).unwrap();
        library.record_played(&first).unwrap();
        assert_eq!(library.history()[0], first);
        assert_eq!(library.history()[1], second);
        assert_eq!(library.history().len(), 2);

        for index in 0..(HISTORY_LIMIT + 5) {
            library
                .record_played(&station(&format!("station-{index}")))
                .unwrap();
        }
        assert_eq!(library.history().len(), HISTORY_LIMIT);
        library.clear_history().unwrap();
        assert!(library.history().is_empty());

        let reloaded = StationLibrary::load(path).unwrap();
        assert!(reloaded.history().is_empty());
        assert_eq!(reloaded.favorites(), library.favorites());
    }

    #[test]
    fn corrupt_file_returns_parse_error() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("library.json");
        fs::write(&path, b"{ not json").unwrap();
        assert!(matches!(
            StationLibrary::load(path),
            Err(LibraryError::Parse(_))
        ));
    }

    #[test]
    fn load_normalizes_duplicates_and_history_limit() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("library.json");
        let saved = station("11111111-1111-1111-1111-111111111111");
        let mut history = vec![saved.clone(); 3];
        for index in 0..(HISTORY_LIMIT + 4) {
            history.push(station(&format!("station-{index}")));
        }
        let data = LibraryData {
            favorites: vec![saved.clone(), saved.clone()],
            history,
        };
        fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();

        let library = StationLibrary::load(path).unwrap();
        assert_eq!(library.favorites(), &[saved]);
        assert_eq!(library.history().len(), HISTORY_LIMIT);
    }

    #[test]
    fn ephemeral_library_keeps_changes_without_file() {
        let mut library = StationLibrary::ephemeral();
        let saved = station("11111111-1111-1111-1111-111111111111");
        library.toggle_favorite(&saved).unwrap();
        library.record_played(&saved).unwrap();
        assert_eq!(library.favorites(), std::slice::from_ref(&saved));
        assert_eq!(library.history(), std::slice::from_ref(&saved));
        assert!(library.path().is_none());
    }

    #[test]
    fn export_import_roundtrip_merges_and_deduplicates() {
        let favorite = station("11111111-1111-1111-1111-111111111111");
        let played = station("22222222-2222-2222-2222-222222222222");
        let mut source = StationLibrary::ephemeral();
        source.toggle_favorite(&favorite).unwrap();
        source.record_played(&played).unwrap();

        let json = source.export_json().unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["schema"], EXPORT_SCHEMA);
        assert_eq!(value["app"], "orbitfm");
        assert!(value["exported_at"].as_u64().unwrap() > 0);

        let mut target = StationLibrary::ephemeral();
        let summary = target.import_bytes(json.as_bytes()).unwrap();
        assert_eq!(
            summary,
            ImportSummary {
                favorites_added: 1,
                history_added: 1,
                skipped: 0,
            }
        );
        assert_eq!(target.favorites(), &[favorite]);
        assert_eq!(target.history(), &[played]);

        let again = target.import_bytes(json.as_bytes()).unwrap();
        assert_eq!(again, ImportSummary::default());
        assert_eq!(target.favorites().len(), 1);
        assert_eq!(target.history().len(), 1);
    }

    #[test]
    fn export_import_skips_local_streams_by_public_url_policy() {
        let public = station("11111111-1111-1111-1111-111111111111");
        let mut local = station("url:16e83889014b9088");
        local.url = "http://127.0.0.1:8765/tone.wav".to_owned();
        local.url_resolved = local.url.clone();
        local.source = crate::services::StationSource::Stream;

        let mut source = StationLibrary::ephemeral();
        source.toggle_favorite(&public).unwrap();
        source.toggle_favorite(&local).unwrap();
        let json = source.export_json().unwrap();

        let mut target = StationLibrary::ephemeral();
        let summary = target.import_bytes(json.as_bytes()).unwrap();
        assert_eq!(
            summary,
            ImportSummary {
                favorites_added: 1,
                history_added: 0,
                skipped: 1,
            }
        );
        assert_eq!(target.favorites(), &[public]);

        let again = target.import_bytes(json.as_bytes()).unwrap();
        assert_eq!(
            again,
            ImportSummary {
                favorites_added: 0,
                history_added: 0,
                skipped: 1,
            }
        );
    }

    #[test]
    fn import_rejects_unknown_schema_and_foreign_app() {
        let mut library = StationLibrary::ephemeral();
        let schema_two = br#"{"schema":2,"app":"orbitfm","favorites":[],"history":[]}"#;
        assert!(library.import_bytes(schema_two).is_err());
        let foreign = br#"{"schema":1,"app":"other","favorites":[],"history":[]}"#;
        assert!(library.import_bytes(foreign).is_err());
        let malformed = b"{not json";
        assert!(library.import_bytes(malformed).is_err());
    }

    #[test]
    fn import_rejects_oversized_payload_and_station_count() {
        let mut library = StationLibrary::ephemeral();
        let oversized = vec![b' '; (IMPORT_FILE_LIMIT + 1) as usize];
        assert!(library.import_bytes(&oversized).is_err());

        let export = LibraryExport {
            schema: EXPORT_SCHEMA,
            app: "orbitfm".to_owned(),
            exported_at: 1,
            favorites: (0..=IMPORT_STATIONS_LIMIT)
                .map(|index| station(&format!("station-{index}")))
                .collect(),
            history: Vec::new(),
        };
        let bytes = serde_json::to_vec(&export).unwrap();
        assert!(bytes.len() as u64 <= IMPORT_FILE_LIMIT);
        assert!(library.import_bytes(&bytes).is_err());
        assert!(library.favorites().is_empty());
    }

    #[test]
    fn import_skips_hostile_stations_and_sanitizes_accepted_ones() {
        let mut valid = station("11111111-1111-1111-1111-111111111111");
        valid.name = "Jazz\u{1B}[2J".to_owned();
        valid.geo_lat = Some(200.0);
        valid.geo_long = Some(10.0);

        let mut private = station("22222222-2222-2222-2222-222222222222");
        private.url = "http://192.168.1.10/live".to_owned();
        private.url_resolved = private.url.clone();

        let mut userinfo = station("33333333-3333-3333-3333-333333333333");
        userinfo.url = "https://user:pass@example.com/live".to_owned();
        userinfo.url_resolved = userinfo.url.clone();

        let mut scheme = station("44444444-4444-4444-4444-444444444444");
        scheme.url = "file:///etc/passwd".to_owned();
        scheme.url_resolved = scheme.url.clone();

        let export = LibraryExport {
            schema: EXPORT_SCHEMA,
            app: "orbitfm".to_owned(),
            exported_at: 1,
            favorites: vec![valid, private, userinfo, scheme],
            history: Vec::new(),
        };
        let bytes = serde_json::to_vec(&export).unwrap();
        let mut library = StationLibrary::ephemeral();
        let summary = library.import_bytes(&bytes).unwrap();

        assert_eq!(
            summary,
            ImportSummary {
                favorites_added: 1,
                history_added: 0,
                skipped: 3,
            }
        );
        let saved = &library.favorites()[0];
        assert_eq!(saved.name, "Jazz [2J");
        assert_eq!(saved.geo_lat, None);
        assert_eq!(saved.geo_long, None);
    }

    #[cfg(unix)]
    #[test]
    fn export_writes_private_file_and_import_reads_it_back() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("out/library-export.json");
        let saved = station("11111111-1111-1111-1111-111111111111");
        let mut source = StationLibrary::ephemeral();
        source.toggle_favorite(&saved).unwrap();

        assert_eq!(source.export_to(&path).unwrap(), 1);
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert!(
            !directory.path().join("out").join(EXPORT_TMP_NAME).exists(),
            "временный файл убран"
        );

        let mut target = StationLibrary::ephemeral();
        assert_eq!(target.import_from(&path).unwrap().favorites_added, 1);
        assert_eq!(target.import_from(&path).unwrap().favorites_added, 0);
    }
}
