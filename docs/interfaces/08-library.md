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

pub struct StationLibrary {
    /* path: Option<PathBuf>, data: LibraryData,
       damaged_at_load: bool, quarantined_path: Option<PathBuf> */
}

/// Версия схемы переноса (docs/interfaces/15-library-export.md).
pub const EXPORT_SCHEMA: u32 = 1;

/// Итог слияния импортированного файла с локальной библиотекой.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportSummary {
    pub favorites_added: usize,
    pub history_added: usize,
    pub skipped: usize,
}

impl StationLibrary {
    /// Загружает библиотеку из файла. Отсутствующий файл — пустая библиотека.
    pub fn load(path: PathBuf) -> Result<Self, LibraryError>;

    /// Загружает библиотеку из $XDG_DATA_HOME/orbitfm/library.json.
    pub fn load_default() -> Result<Self, LibraryError>;

    /// Библиотека без файла: изменения живут до завершения процесса.
    pub fn ephemeral() -> Self;

    /// Путь к файлу; `None` у эфемерной библиотеки.
    pub fn path(&self) -> Option<&Path>;

    /// Файл не разобрался и был отодвинут в карантин (ADR-0062).
    pub fn damaged_at_load(&self) -> bool;

    /// Куда переехал повреждённый файл; `None`, если карантин не удался.
    pub fn quarantined_path(&self) -> Option<&Path>;

    pub fn favorites(&self) -> &[Station];
    pub fn history(&self) -> &[Station];
    pub fn is_favorite(&self, station_uuid: &str) -> bool;

    /// Добавляет станцию в избранное или убирает её оттуда.
    /// Возвращает `true`, если станция теперь в избранном.
    pub fn toggle_favorite(&mut self, station: &Station) -> Result<bool, LibraryError>;

    /// Запоминает станцию как последнюю прослушанную.
    pub fn record_played(&mut self, station: &Station) -> Result<(), LibraryError>;

    pub fn clear_history(&mut self) -> Result<(), LibraryError>;

    /// Переносимый JSON схемы 1 (ADR-0046).
    pub fn export_json(&self) -> Result<String, LibraryError>;

    /// Атомарная запись экспорта; возвращает число избранных станций.
    pub fn export_to(&self, path: &Path) -> Result<usize, LibraryError>;

    /// Чтение файла и слияние с локальной библиотекой.
    pub fn import_from(&mut self, path: &Path) -> Result<ImportSummary, LibraryError>;

    /// Слияние байтов без файла: единица тестирования и будущий Android.
    pub fn import_bytes(&mut self, bytes: &[u8]) -> Result<ImportSummary, LibraryError>;
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
| `load` | Читает JSON; `NotFound` → пустая библиотека; повреждённый JSON уезжает в `<файл>.corrupt*`, библиотека начинается с нуля, путь остаётся рабочим (ADR-0062) |
| `load_default` | `load(default_path()?)` |
| `ephemeral` | Библиотека в памяти; `save` — no-op, файл не создаётся и не перезаписывается |
| `favorites` / `history` | Срезы в порядке хранения: избранное — порядок добавления, история — от новых к старым |
| `is_favorite` | Сравнение по `stationuuid` |
| `toggle_favorite` | Нет станции — добавить в конец избранного; есть — удалить; затем сохранить |
| `record_played` | Удалить прежнее вхождение по `stationuuid`, вставить в начало, обрезать до `HISTORY_LIMIT`, сохранить |
| `clear_history` | Очистить историю и сохранить; пустая история — успех без записи |
| `export_json` | Снимок схемы 1: избранное и история в порядке хранения |
| `export_to` | `export_json` → атомарная запись; на Unix права `0600` |
| `import_from` / `import_bytes` | Слияние по `stationuuid`; локальная запись побеждает, некорректные станции пропускаются; станции `source = stream` с локальным адресом не проходят политику публичных URL и попадают в `skipped` |

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

## 5. Экспорт и импорт

Перенос библиотеки между desktop и Android описан отдельным контрактом
`docs/interfaces/15-library-export.md` (схема 1, ADR-0046). Ключевые
инварианты:

- импорт не заменяет библиотеку, а сливает её по `stationuuid`;
- локальная запись при совпадении UUID не перезаписывается;
- `url`/`url_resolved` обязаны быть публичным `http(s)` без userinfo
  (ADR-0025); homepage/favicon проверяются отдельно и могут стать `null`;
- файл ≤ 2 МиБ и ≤ 5000 станций; строки санитизируются;
- экспорт на Unix получает права `0600` и пишется атомарно.

Экспорт отражает библиотеку как есть, включая синтетические станции `play-url`
(`source = stream`) с локальными адресами вроде `http://127.0.0.1:…`. Импорт
проверяет `url`/`url_resolved` по ADR-0025, поэтому такие станции при переносе
пропускаются и учитываются в `skipped`; повторный импорт того же файла снова
вернёт их в `skipped`, а для файла только с публичными станциями — нули.

## 6. Тесты

Модульные тесты покрывают: выбор XDG-пути и fallback на `HOME`, загрузку без файла,
roundtrip избранного и истории, переключение избранного, дедупликацию/перемещение
в начало и лимит истории, карантин повреждённого файла и свежую запись после
него, нормализацию при
загрузке, работу эфемерной библиотеки без файла, roundtrip экспорт → импорт,
отказ неизвестной схемы и чужого `app`, потолки размера и числа станций,
пропуск приватных/некорректных URL, пропуск локальных `stream`-станций при
экспорте → импорте, санитизацию и Unix-права `0600`.

## 7. Связанные документы

- ADR-0013: Избранное и история станций
- ADR-0046: Ритуалы слушания и перенос библиотеки
- `docs/interfaces/15-library-export.md`
- `docs/modules/09-library.md`
