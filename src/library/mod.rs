//! Избранное и история станций с локальной персистентностью.

use std::collections::HashSet;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::services::Station;

const DATA_DIR: &str = "orbitfm";
const FILE_NAME: &str = "library.json";
const TMP_FILE_NAME: &str = "library.json.tmp";
/// Сколько последних станций хранит история.
pub const HISTORY_LIMIT: usize = 20;

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
        let favorite = match self
            .data
            .favorites
            .iter()
            .position(|saved| saved.stationuuid == station.stationuuid)
        {
            Some(index) => {
                self.data.favorites.remove(index);
                false
            }
            None => {
                self.data.favorites.push(station.clone());
                true
            }
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

    fn save(&self) -> Result<(), LibraryError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let directory = path
            .parent()
            .ok_or_else(|| LibraryError::Write(format!("нет каталога для {}", path.display())))?;
        fs::create_dir_all(directory)
            .map_err(|err| LibraryError::Write(format!("{}: {err}", directory.display())))?;
        let json = serde_json::to_vec_pretty(&self.data)
            .map_err(|err| LibraryError::Write(err.to_string()))?;
        let tmp = path.with_file_name(TMP_FILE_NAME);
        fs::write(&tmp, json)
            .map_err(|err| LibraryError::Write(format!("{}: {err}", tmp.display())))?;
        fs::rename(&tmp, path).map_err(|err| {
            let _ = fs::remove_file(&tmp);
            LibraryError::Write(format!("{}: {err}", path.display()))
        })
    }
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
            bitrate: 128,
            codec: "MP3".to_owned(),
            geo_lat: None,
            geo_long: None,
        }
    }

    #[test]
    fn default_path_prefers_absolute_xdg_data_home() {
        let path = default_path_from(Some("/tmp/xdg".into()), Some("/home/user".into())).unwrap();
        assert_eq!(path, PathBuf::from("/tmp/xdg/orbitfm/library.json"));
    }

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
}
