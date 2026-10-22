# Интерфейс: Библиотека станций

```rust
use crate::services::Station;

/// Сколько последних станций хранит история.
pub const HISTORY_LIMIT: usize = 20;

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

pub struct StationLibrary { /* path: Option<PathBuf>, data: LibraryData */ }

impl StationLibrary {
    /// Загружает библиотеку из файла. Отсутствующий файл — пустая библиотека.
    pub fn load(path: PathBuf) -> Result<Self, LibraryError>;

    /// Загружает библиотеку из $XDG_DATA_HOME/orbitfm/library.json.
    pub fn load_default() -> Result<Self, LibraryError>;

    /// Библиотека без файла: изменения живут до завершения процесса.
    pub fn ephemeral() -> Self;

    /// Путь к файлу; `None` у эфемерной библиотеки.
    pub fn path(&self) -> Option<&Path>;

    pub fn favorites(&self) -> &[Station];
    pub fn history(&self) -> &[Station];
    pub fn is_favorite(&self, station_uuid: &str) -> bool;

    /// Добавляет станцию в избранное или убирает её оттуда.
    /// Возвращает `true`, если станция теперь в избранном.
    pub fn toggle_favorite(&mut self, station: &Station) -> Result<bool, LibraryError>;

    /// Запоминает станцию как последнюю прослушанную.
    pub fn record_played(&mut self, station: &Station) -> Result<(), LibraryError>;

    pub fn clear_history(&mut self) -> Result<(), LibraryError>;
}

/// Стандартный путь по XDG Base Directory Specification.
pub fn default_path() -> Result<PathBuf, LibraryError>;
```

## 1. Назначение

Контракт локального хранилища избранных станций и истории прослушивания.
Модуль не знает о GUI, аудио и IPC: Application Core вызывает методы и сам решает,
как показывать ошибки.

## 2. Семантика операций

| Метод | Поведение |
|-------|-----------|
| `load` | Читает JSON; `NotFound` → пустая библиотека; повреждённый JSON → `Parse` |
| `load_default` | `load(default_path()?)` |
| `ephemeral` | Библиотека в памяти; `save` — no-op, файл не создаётся и не перезаписывается |
| `favorites` / `history` | Срезы в порядке хранения: избранное — порядок добавления, история — от новых к старым |
| `is_favorite` | Сравнение по `stationuuid` |
| `toggle_favorite` | Нет станции — добавить в конец избранного; есть — удалить; затем сохранить |
| `record_played` | Удалить прежнее вхождение по `stationuuid`, вставить в начало, обрезать до `HISTORY_LIMIT`, сохранить |
| `clear_history` | Очистить историю и сохранить; пустая история — успех без записи |

Станции хранятся снимком `Station` целиком (включая `url_resolved`, `favicon`,
`tags`), поэтому сохранённую станцию можно запустить без обращения к каталогу.

## 3. Расположение и формат

1. Если `XDG_DATA_HOME` задан и абсолютен — `$XDG_DATA_HOME/orbitfm/library.json`.
2. Иначе — `$HOME/.local/share/orbitfm/library.json`.
3. Относительный `XDG_DATA_HOME` игнорируется; неабсолютный или отсутствующий `HOME`
   даёт `LibraryError::Path`.

Формат файла:

```json
{
  "favorites": [Station, ...],
  "history": [Station, ...]
}
```

Отсутствующие поля при разборе считаются пустыми списками (`#[serde(default)]`).
При загрузке дубликаты по `stationuuid` удаляются (остаётся первое вхождение),
история обрезается до `HISTORY_LIMIT`.

## 4. Запись и отказоустойчивость

- Запись синхронная и выполняется только по действию пользователя: файл мал,
  операции редки (ADR-0013).
- JSON пишется в `library.json.tmp` рядом с целевым файлом, затем `rename`
  поверх; при ошибке rename временный файл удаляется, старый файл остаётся.
- Каталог создаётся при первой записи (`create_dir_all`).
- Ошибки `Read`/`Parse` не приводят к перезаписи файла: Application Core
  переключается на `ephemeral()` и показывает предупреждение.
- Ошибки `Write` возвращаются вызывающему; состояние в памяти остаётся изменённым.

## 5. Тесты

Модульные тесты покрывают: выбор XDG-пути и fallback на `HOME`, загрузку без файла,
roundtrip избранного и истории, переключение избранного, дедупликацию/перемещение
в начало и лимит истории, ошибку разбора повреждённого файла, нормализацию при
загрузке и работу эфемерной библиотеки без файла.

## 6. Связанные документы

- ADR-0013: Избранное и история станций
- `docs/modules/09-library.md`
