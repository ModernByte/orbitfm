//! Локальная статистика прослушивания: счётчики живут только на устройстве.
//!
//! Модуль хранит файл `stats.json` схемы 1 и считает сводки для UI, CLI и IPC.
//! Решение «какая станция активна и сколько секунд наработано» принимает
//! Application Core (`docs/interfaces/10-session.md` § 3.2): здесь нет ни egui,
//! ни IPC, ни настроек. Сетевых путей у статистики нет ни при каком профиле
//! сети (ADR-0048), данные никуда не отправляются (ADR-0049, README §(г)).

use std::collections::{BTreeMap, HashMap};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::services::{Station, StationSource};

const DATA_DIR: &str = "orbitfm";
const FILE_NAME: &str = "stats.json";
const TMP_FILE_NAME: &str = "stats.json.tmp";

/// Потолки длин строк в файле: те же планки, что у библиотеки (ADR-0030).
const UUID_LIMIT: usize = 64;
const NAME_LIMIT: usize = 256;
const COUNTRY_LIMIT: usize = 8;
/// Секунд в UTC-сутках: по ним считается дата бакета `days`.
const SECONDS_PER_DAY: u64 = 86_400;
/// Смещение эпохи civil-from-days Howard Hinnant (3-е марта 2000 года).
const DAYS_TO_EPOCH: i64 = 719_468;
/// Префикс ключа внешнего потока: адрес в файл не пишется (ADR-0025).
const EXTERNAL_PREFIX: &str = "external:";
/// Перец FNV-1a 64-бит и его простое число: ключ потока без новых зависимостей.
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Версия формата файла и ответа (`docs/interfaces/17-stats.md`).
pub const SCHEMA: u32 = 1;
/// Потолок записей по станциям: при переполнении выбывает наименьшая по `seconds`.
pub const STATION_LIMIT: usize = 500;
/// Потолок дат глобального бакета `days`; старые даты выбрасываются первыми.
pub const DAY_LIMIT: usize = 730;
/// Сколько накопленных секунд требует записи на диск.
pub const FLUSH_SECS: u64 = 30;

/// Сколько позиций топ-списка отдаёт снимок (интерфейс 17 § 5).
pub const TOP_LIMIT: usize = 10;
/// Число последних дат в снимке.
pub const DAY_WINDOW_LIMIT: usize = 30;
/// Окно «недели» в UTC-датах, включая сегодня.
pub const WEEK_DAYS: i64 = 7;
/// Окно «месяца» в UTC-датах, включая сегодня.
pub const MONTH_DAYS: i64 = 30;
/// Потолок сырых тегов записи: 128 байт по границе тега (ADR-0049 § 3).
pub const TAGS_BYTE_LIMIT: usize = 128;

/// Ошибки хранилища статистики.
///
/// Чтение ошибкой не является: отсутствующий, неразбираемый или нечитаемый файл
/// даёт пустую статистику (интерфейс 17 § 2), а тост показывает Core.
#[derive(Debug, thiserror::Error)]
pub enum StatsError {
    #[error("не удалось определить путь статистики: {0}")]
    Path(String),
    #[error("не удалось сохранить статистику: {0}")]
    Write(String),
}

/// Счётчики одной станции — запись файла `stats.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StationStat {
    pub uuid: String,
    pub name: String,
    pub countrycode: String,
    pub tags: String,
    pub seconds: u64,
    pub sessions: u64,
    pub last_at: u64,
}

impl StationStat {
    fn new(key: &str, station: &Station) -> Self {
        Self {
            uuid: clean(key, UUID_LIMIT),
            name: clean(&station.name, NAME_LIMIT),
            countrycode: clean(&station.countrycode, COUNTRY_LIMIT),
            tags: clip_tags(&station.tags),
            seconds: 0,
            sessions: 0,
            last_at: 0,
        }
    }
}

/// Содержимое `stats.json` схемы 1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct StatsData {
    schema: u32,
    updated_at: u64,
    stations: Vec<StationStat>,
    days: BTreeMap<String, u64>,
}

impl Default for StatsData {
    /// Пустая статистика схемы 1: `#[serde(default)]` берёт поля отсюда,
    /// поэтому файл без `schema` правится руками и читается как схема 1.
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            updated_at: 0,
            stations: Vec::new(),
            days: BTreeMap::new(),
        }
    }
}

/// Сводка станции для `stats`, `stats --json` и окна настроек.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StationSummary {
    pub uuid: String,
    pub name: String,
    pub seconds: u64,
    pub sessions: u64,
    pub last_at: u64,
}

/// Сводка прослушанного по коду страны.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountrySummary {
    pub code: String,
    pub seconds: u64,
}

/// Сводка прослушанного по грубому тегу.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagSummary {
    pub tag: String,
    pub seconds: u64,
}

/// Секунды одного UTC-дня.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaySummary {
    pub date: String,
    pub seconds: u64,
}

/// Готовая сводка по счётчикам: списки после построения не растут, поэтому
/// держат `Box<[…]>` (`docs/AGENTS.md` § 5.1). Счёт — по запросу, не на кадр.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StatsSnapshot {
    pub total_seconds: u64,
    pub week_seconds: u64,
    pub month_seconds: u64,
    /// Топ `TOP_LIMIT` станций по `seconds`, большие первыми.
    pub stations: Box<[StationSummary]>,
    /// Топ `TOP_LIMIT` стран по `seconds`.
    pub countries: Box<[CountrySummary]>,
    /// Топ `TOP_LIMIT` тегов по `seconds`.
    pub tags: Box<[TagSummary]>,
    /// Не больше `DAY_WINDOW_LIMIT` последних дат, новые первыми.
    pub days: Box<[DaySummary]>,
}

/// Счётчики прослушанного с локальной персистентностью.
///
/// Хранилище без файла (`ephemeral`) копит счётчики в памяти: запись
/// становится no-op, что спасает при недоступном каталоге данных.
#[derive(Debug)]
pub struct StatsStore {
    path: Option<PathBuf>,
    data: StatsData,
    /// Секунды, наработанные с последней успешной записи.
    pending: u64,
    /// Файл не разобрался или не прочитался: счётчики начались с нуля.
    damaged_at_load: bool,
}

impl StatsStore {
    /// Загружает статистику из файла. Отсутствующий, неразбираемый или
    /// нечитаемый файл — пустая статистика, а не ошибка: исходник при этом не
    /// трогается до первой успешной записи (ADR-0049 § 2).
    pub fn load(path: PathBuf) -> Result<Self, StatsError> {
        let (data, damaged) = match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<StatsData>(&bytes) {
                // Неизвестная схема равносильна повреждённому файлу: писать
                // поверх чужого формата без разрешения нельзя.
                Ok(data) if data.schema == SCHEMA => (data, false),
                Ok(_) | Err(_) => (StatsData::default(), true),
            },
            Err(err) if err.kind() == ErrorKind::NotFound => (StatsData::default(), false),
            Err(_) => (StatsData::default(), true),
        };
        let mut store = Self {
            path: Some(path),
            data,
            pending: 0,
            damaged_at_load: damaged,
        };
        store.sanitize();
        Ok(store)
    }

    /// Загружает статистику из стандартного каталога данных XDG.
    pub fn load_default() -> Result<Self, StatsError> {
        Self::load(default_path()?)
    }

    /// Хранилище без файла: счётчики живут до завершения процесса.
    pub fn ephemeral() -> Self {
        Self {
            path: None,
            data: StatsData::default(),
            pending: 0,
            damaged_at_load: false,
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Секунды, накопленные с последней успешной записи: по ним Core решает,
    /// пора ли сбрасывать на диск, и повторяет попытку после сбоя записи.
    pub fn pending(&self) -> u64 {
        self.pending
    }

    /// Файл при старте не разобрался или не прочитался: Core показывает тост,
    /// воспроизведение от этого не зависит (ADR-0049 § 2, интерфейс 17 § 6).
    pub fn damaged_at_load(&self) -> bool {
        self.damaged_at_load
    }

    /// Добавляет секунды активной станции и текущему UTC-дню; метаданные
    /// при этом обновляются, а `seconds` и `sessions` не сбрасываются.
    pub fn record(&mut self, station: &Station, seconds: u64, now: u64) {
        if seconds == 0 {
            return;
        }
        {
            let entry = self.entry(&station_key(station), station);
            entry.seconds = entry.seconds.saturating_add(seconds);
            entry.last_at = entry.last_at.max(now);
        }
        let slot = self.data.days.entry(utc_date_key(now)).or_insert(0);
        *slot = slot.saturating_add(seconds);
        self.pending = self.pending.saturating_add(seconds);
        self.normalize();
    }

    /// Засчитывает запуск станции: `sessions` отличает «один вечер фоном» от
    /// «десять запусков по минуте» (интерфейс 17 § 2).
    pub fn note_start(&mut self, station: &Station, now: u64) {
        {
            let entry = self.entry(&station_key(station), station);
            entry.sessions = entry.sessions.saturating_add(1);
            entry.last_at = entry.last_at.max(now);
        }
        self.normalize();
    }

    /// Пишет файл, если есть непогашенные секунды; без накопленного — no-op,
    /// фоновой записи по таймеру модуль не ведёт (§ 4).
    pub fn flush(&mut self) -> Result<(), StatsError> {
        if self.pending == 0 {
            return Ok(());
        }
        self.write()?;
        self.pending = 0;
        Ok(())
    }

    /// Сбрасывает счётчики и пишет пустую схему 1; идемпотентен, поэтому
    /// `stats clear` при нулевых счётчиках тоже успешен (интерфейс 17 § 6).
    pub fn clear(&mut self) -> Result<(), StatsError> {
        self.data = StatsData::default();
        self.pending = 0;
        self.write()
    }

    /// Сводка для окна настроек, `stats` и `stats --json`: суммарное время,
    /// окна 7 и 30 UTC-дат, топ-10 станций, стран и тегов, последние даты.
    pub fn snapshot(&self) -> StatsSnapshot {
        let today = epoch_day(unix_now());
        let total_seconds = self
            .data
            .stations
            .iter()
            .fold(0u64, |acc, station| acc.saturating_add(station.seconds));

        let mut week_seconds = 0u64;
        let mut month_seconds = 0u64;
        let mut days: Vec<DaySummary> = Vec::new();
        // Ключи ISO-дат идут по возрастанию, обратный обход — от новых к старым:
        // за одно чтение собираются и окна, и список последних дат.
        for (date, seconds) in self.data.days.iter().rev() {
            let Some(index) = day_index(date) else {
                continue;
            };
            let age = today.saturating_sub(index);
            if age >= MONTH_DAYS {
                break;
            }
            if age < WEEK_DAYS {
                week_seconds = week_seconds.saturating_add(*seconds);
            }
            month_seconds = month_seconds.saturating_add(*seconds);
            if days.len() < DAY_WINDOW_LIMIT {
                days.push(DaySummary {
                    date: date.clone(),
                    seconds: *seconds,
                });
            }
        }

        let mut countries: HashMap<String, u64> = HashMap::new();
        let mut tags: HashMap<String, u64> = HashMap::new();
        for station in &self.data.stations {
            let code = station.countrycode.trim().to_ascii_uppercase();
            if !code.is_empty() {
                let slot = countries.entry(code).or_insert(0);
                *slot = slot.saturating_add(station.seconds);
            }
            for tag in station.tags.split(',').map(str::trim) {
                if tag.is_empty() {
                    continue;
                }
                // Регистр не важен: «Pop» и «pop» — один жанр в сводке.
                let slot = tags.entry(tag.to_lowercase()).or_insert(0);
                *slot = slot.saturating_add(station.seconds);
            }
        }

        StatsSnapshot {
            total_seconds,
            week_seconds,
            month_seconds,
            // Станции в файле уже отсортированы от больших `seconds` к меньшим.
            stations: self
                .data
                .stations
                .iter()
                .take(TOP_LIMIT)
                .map(|station| StationSummary {
                    uuid: station.uuid.clone(),
                    name: station.name.clone(),
                    seconds: station.seconds,
                    sessions: station.sessions,
                    last_at: station.last_at,
                })
                .collect(),
            countries: rank_counts(countries)
                .into_iter()
                .map(|(code, seconds)| CountrySummary { code, seconds })
                .collect(),
            tags: rank_counts(tags)
                .into_iter()
                .map(|(tag, seconds)| TagSummary { tag, seconds })
                .collect(),
            days: days.into_boxed_slice(),
        }
    }

    /// Находит запись станции или создаёт её с метаданными из каталога.
    fn entry(&mut self, key: &str, station: &Station) -> &mut StationStat {
        if let Some(index) = self
            .data
            .stations
            .iter()
            .position(|saved| saved.uuid == key)
        {
            let saved = &mut self.data.stations[index];
            refresh_metadata(saved, station);
            return saved;
        }
        self.data.stations.push(StationStat::new(key, station));
        self.data
            .stations
            .last_mut()
            .expect("запись станции только что добавлена")
    }

    /// Инварианты файла: порядок по `seconds`, потолок записей по станциям и
    /// потолок дат. Дешёвая часть — зовётся на каждую мутацию.
    fn normalize(&mut self) {
        self.data.stations.sort_by(|left, right| {
            right
                .seconds
                .cmp(&left.seconds)
                .then_with(|| left.uuid.cmp(&right.uuid))
        });
        self.data.stations.truncate(STATION_LIMIT);
        // Ключи `YYYY-MM-DD` лексикографически совпадают с хронологией,
        // поэтому самые старые даты — первые в порядке `BTreeMap`.
        let stale = self.data.days.len().saturating_sub(DAY_LIMIT);
        if stale > 0 {
            let oldest: Vec<String> = self.data.days.keys().take(stale).cloned().collect();
            for date in oldest {
                self.data.days.remove(&date);
            }
        }
    }

    /// Приводит вручную поправленный файл к инвариантам: без дублей ключей,
    /// без пустых ключей и без дат, которых не бывает. Зовётся только при
    /// чтении — на каждый накопленный секунды такая чистка избыточна.
    fn sanitize(&mut self) {
        merge_duplicates(&mut self.data.stations);
        self.data.days.retain(|date, _| day_index(date).is_some());
        self.normalize();
    }

    /// Единственная точка записи: атомарный `stats.json.tmp` → `rename`.
    fn write(&mut self) -> Result<(), StatsError> {
        self.data.schema = SCHEMA;
        self.data.updated_at = unix_now();
        self.normalize();
        let json = serde_json::to_vec_pretty(&self.data)
            .map_err(|err| StatsError::Write(err.to_string()))?;
        let Some(path) = &self.path else {
            return Ok(());
        };
        crate::storage::write_atomic(path, TMP_FILE_NAME, &json)
            .map_err(|err| StatsError::Write(format!("{}: {err}", path.display())))?;
        private_file(path)
    }
}

/// Порядок сводки по ключу: секунды убывают, при равенстве — ключ по алфавиту.
fn rank_counts(counts: HashMap<String, u64>) -> Vec<(String, u64)> {
    let mut rows: Vec<(String, u64)> = counts.into_iter().collect();
    rows.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    rows.truncate(TOP_LIMIT);
    rows
}

/// Ключ станции в файле: каталожный `stationuuid` либо синтетический ключ
/// внешнего потока. URL в файл не попадает (ADR-0025, ADR-0040).
pub(crate) fn station_key(station: &Station) -> String {
    let uuid = clean(&station.stationuuid, UUID_LIMIT);
    let external = station.source == StationSource::Stream || uuid.starts_with("url:");
    if !external && !uuid.is_empty() {
        return uuid;
    }
    let url = if station.url.is_empty() {
        station.url_resolved.as_str()
    } else {
        station.url.as_str()
    };
    if url.is_empty() {
        // Потока без адреса не бывает: остаётся хотя бы прежний ключ.
        return if uuid.is_empty() {
            format!("{EXTERNAL_PREFIX}unknown")
        } else {
            uuid
        };
    }
    external_key(url)
}

/// `external:<fnv1a64(url)>` — 16 шестнадцатеричных разрядов FNV-1a 64-бит.
/// Ключ детерминирован между запусками и не требует `sha2`/`ring`: он
/// идентифицирует поток, а не защищает данные (ADR-0049 § 3).
pub fn external_key(url: &str) -> String {
    format!("{EXTERNAL_PREFIX}{:016x}", fnv1a_64(url))
}

fn fnv1a_64(value: &str) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Дата бакета `days` в UTC: своего часового пояса в приложении нет, а
/// `chrono`/`time` в зависимостях нет тоже (§ 3).
fn utc_date_key(secs: u64) -> String {
    let (year, month, day) = civil_from_days(epoch_day(secs));
    format!("{year:04}-{month:02}-{day:02}")
}

fn epoch_day(secs: u64) -> i64 {
    // До года за пределами `i64` счётчики не доживают: предел — страховка от
    // вручную перебитого `updated_at`, а не рабочая ветка.
    i64::try_from(secs.div_euclid(SECONDS_PER_DAY)).unwrap_or(i64::MAX / 2)
}

/// Номер UTC-суток из ключа `YYYY-MM-DD`; `None` — ключ не дата. Сверка с
/// прямым преобразованием отбраковывает несуществующие дни вроде `2026-02-30`.
fn day_index(date: &str) -> Option<i64> {
    if date.len() != 10 {
        return None;
    }
    let mut parts = date.split('-');
    let year = parts.next()?.parse::<i64>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() || year < 0 || !(1..=12).contains(&month) || day == 0 {
        return None;
    }
    let days = days_from_civil(year, month, day);
    (civil_from_days(days) == (year, month, day)).then_some(days)
}

/// Civil-from-days по алгоритму Howard Hinnant: сутки от эпохи → год, месяц, день.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + DAYS_TO_EPOCH;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    // Год March-based календаря: январь и февраль принадлежат следующему
    // календарному году.
    (if month <= 2 { y + 1 } else { y }, month as u32, day as u32)
}

/// Обратное преобразование: civil-дата → номер суток от эпохи.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * u64::from(mp) + 2) / 5 + u64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i64 - DAYS_TO_EPOCH
}

/// Усекает сырые теги до 128 байт по границе тега: целый тег не разрывается
/// посередине, поэтому файл не разрастается даже при вручную поправленных
/// метаданных (ADR-0049 § 3).
fn clip_tags(tags: &str) -> String {
    let mut kept: Vec<&str> = Vec::new();
    let mut used = 0usize;
    for tag in tags.split(',') {
        let tag = tag.trim();
        if tag.is_empty() {
            continue;
        }
        let need = tag.len() + usize::from(!kept.is_empty());
        if used + need > TAGS_BYTE_LIMIT {
            break;
        }
        used += need;
        kept.push(tag);
    }
    kept.join(",")
}

/// Заполняет пустое поле более полными данными каталога; существующее значение
/// не перезаписывается, счётчики при этом не трогаются (интерфейс 17 § 2).
fn refresh_metadata(saved: &mut StationStat, station: &Station) {
    fill(&mut saved.name, &clean(&station.name, NAME_LIMIT));
    fill(
        &mut saved.countrycode,
        &clean(&station.countrycode, COUNTRY_LIMIT),
    );
    if saved.tags.is_empty() {
        saved.tags = clip_tags(&station.tags);
    }
}

fn fill(target: &mut String, value: &str) {
    if target.is_empty() && !value.is_empty() {
        *target = value.to_owned();
    }
}

/// Слияние записей с одинаковым ключом: файл правится руками, и дубликаты
/// не должны удваивать топ-списки. Счётчики складываются, метаданные остаются
/// от первой встречи.
fn merge_duplicates(stations: &mut Vec<StationStat>) {
    let mut merged: Vec<StationStat> = Vec::with_capacity(stations.len());
    let mut positions: HashMap<String, usize> = HashMap::new();
    for station in std::mem::take(stations) {
        if let Some(index) = positions.get(&station.uuid).copied() {
            let target = &mut merged[index];
            target.seconds = target.seconds.saturating_add(station.seconds);
            target.sessions = target.sessions.saturating_add(station.sessions);
            target.last_at = target.last_at.max(station.last_at);
        } else {
            positions.insert(station.uuid.clone(), merged.len());
            merged.push(station);
        }
    }
    stations.extend(merged);
    stations.retain(|station| !station.uuid.is_empty());
}

/// Строка в файл: управляющие символы и переводы строк схлопываются, длина
/// ограничивается (`crate::text`, ADR-0030).
fn clean(value: &str, max_chars: usize) -> String {
    crate::text::single_line(value)
        .chars()
        .take(max_chars)
        .collect()
}

pub(crate) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

/// Стандартный путь статистики по XDG Base Directory Specification — как у
/// библиотеки: тот же каталог данных, отдельный приватный файл.
pub fn default_path() -> Result<PathBuf, StatsError> {
    default_path_from(env::var_os("XDG_DATA_HOME"), env::var_os("HOME"))
}

fn default_path_from(
    xdg_data_home: Option<OsString>,
    home: Option<OsString>,
) -> Result<PathBuf, StatsError> {
    if let Some(xdg) = xdg_data_home {
        let path = PathBuf::from(xdg);
        if path.is_absolute() {
            return Ok(path.join(DATA_DIR).join(FILE_NAME));
        }
    }
    let home = home.ok_or_else(|| StatsError::Path("HOME не задан".to_owned()))?;
    let home = PathBuf::from(home);
    if !home.is_absolute() {
        return Err(StatsError::Path(format!(
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

/// Статистика — приватные данные пользователя: на Unix файл доступен только
/// владельцу, как `library.json` и `settings.json`.
#[cfg(unix)]
fn private_file(path: &Path) -> Result<(), StatsError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|err| StatsError::Write(format!("{}: {err}", path.display())))
}

#[cfg(not(unix))]
fn private_file(_path: &Path) -> Result<(), StatsError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "11111111-1111-1111-1111-111111111111";
    const SECOND_UUID: &str = "22222222-2222-2222-2222-222222222222";
    const STREAM_URL: &str = "http://127.0.0.1:8765/tone.wav";

    fn station(uuid: &str) -> Station {
        Station {
            stationuuid: uuid.to_owned(),
            name: format!("Station {uuid}"),
            url: "https://example.com/stream".to_owned(),
            url_resolved: "https://example.com/stream".to_owned(),
            homepage: None,
            favicon: None,
            tags: "jazz".to_owned(),
            countrycode: "DE".to_owned(),
            language: String::new(),
            votes: 0,
            country: "Germany".to_owned(),
            state: String::new(),
            bitrate: 128,
            codec: "MP3".to_owned(),
            geo_lat: None,
            geo_long: None,
            source: StationSource::Radio,
        }
    }

    /// Сутки от эпохи → начало UTC-суток в секундах: бакеты `days` в тестах
    /// привязаны к реальному «сегодня», чтобы окно 7 и 30 дат не плыло.
    fn day_start(index: i64) -> u64 {
        u64::try_from(index)
            .unwrap_or_default()
            .saturating_mul(SECONDS_PER_DAY)
    }

    #[cfg(unix)]
    #[test]
    fn default_path_prefers_absolute_xdg_data_home() {
        let path = default_path_from(Some("/tmp/xdg".into()), Some("/home/user".into())).unwrap();
        assert_eq!(path, PathBuf::from("/tmp/xdg/orbitfm/stats.json"));
    }

    #[cfg(unix)]
    #[test]
    fn default_path_falls_back_to_home_for_relative_xdg() {
        let path = default_path_from(Some("relative".into()), Some("/home/user".into())).unwrap();
        assert_eq!(
            path,
            PathBuf::from("/home/user/.local/share/orbitfm/stats.json")
        );
        assert!(default_path_from(None, None).is_err());
    }

    #[test]
    fn missing_file_loads_empty_stats() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("stats.json");
        let store = StatsStore::load(path.clone()).unwrap();

        assert_eq!(store.snapshot().total_seconds, 0);
        assert_eq!(store.pending(), 0);
        assert!(!store.damaged_at_load());
        assert!(!path.exists(), "чтение не создаёт файл");
    }

    #[test]
    fn corrupt_file_starts_from_zero_and_keeps_original() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("stats.json");
        fs::write(&path, "{ это не json").unwrap();

        let mut store = StatsStore::load(path.clone()).unwrap();
        assert!(store.damaged_at_load());
        assert_eq!(store.snapshot().total_seconds, 0);

        store.record(&station(UUID), FLUSH_SECS, unix_now());
        store.note_start(&station(UUID), unix_now());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "{ это не json",
            "до первой успешной записи исходник нетронут"
        );

        store.flush().unwrap();
        let restored = StatsStore::load(path).unwrap();
        assert!(!restored.damaged_at_load());
        assert_eq!(restored.snapshot().total_seconds, FLUSH_SECS);
        assert_eq!(restored.snapshot().stations[0].sessions, 1);
    }

    #[test]
    fn roundtrip_persists_seconds_sessions_and_days() {
        // Проверка civil-from-days на известных датах, включая високосный год.
        assert_eq!(utc_date_key(0), "1970-01-01");
        assert_eq!(utc_date_key(1_762_156_800), "2025-11-03");
        assert_eq!(utc_date_key(951_782_400), "2000-02-29");
        assert_eq!(day_index("2026-02-30"), None);
        assert_eq!(day_index("2024-02-29"), Some(19782));

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested/stats.json");
        let now = unix_now();
        {
            let mut store = StatsStore::load(path.clone()).unwrap();
            store.note_start(&station(UUID), now);
            store.record(&station(UUID), 1_800, now);
            store.flush().unwrap();
        }

        let store = StatsStore::load(path.clone()).unwrap();
        let snapshot = store.snapshot();
        assert_eq!(snapshot.total_seconds, 1_800);
        assert_eq!(snapshot.stations.len(), 1);
        assert_eq!(snapshot.stations[0].uuid, UUID);
        assert_eq!(snapshot.stations[0].sessions, 1);
        assert_eq!(snapshot.stations[0].name, format!("Station {UUID}"));
        assert_eq!(snapshot.days[0].date, utc_date_key(now));
        assert_eq!(snapshot.days[0].seconds, 1_800);

        let raw: StatsData = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(raw.schema, SCHEMA);
        assert!(raw.updated_at >= now);
        assert_eq!(raw.stations[0].tags, "jazz");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "приватные счётчики доступны только владельцу");
        }
    }

    #[test]
    fn station_eviction_drops_the_smallest_seconds() {
        let mut store = StatsStore::ephemeral();
        let now = unix_now();
        for index in 0..=STATION_LIMIT {
            store.record(
                &station(&format!("station-{index:04}")),
                u64::try_from(index + 1).unwrap() * 10,
                now,
            );
        }

        assert_eq!(store.data.stations.len(), STATION_LIMIT);
        assert_eq!(
            store.data.stations.first().unwrap().seconds,
            u64::try_from(STATION_LIMIT + 1).unwrap() * 10
        );
        assert!(
            store
                .data
                .stations
                .iter()
                .all(|saved| saved.uuid != "station-0000"),
            "наименьшая по seconds запись вытеснена"
        );
    }

    #[test]
    fn day_eviction_drops_the_oldest_dates() {
        let mut store = StatsStore::ephemeral();
        let today = epoch_day(unix_now());
        for age in 0..=(DAY_LIMIT as i64 + 10) {
            store.record(&station(UUID), 60, day_start(today - age));
        }

        assert_eq!(store.data.days.len(), DAY_LIMIT);
        assert!(
            store
                .data
                .days
                .contains_key(&utc_date_key(day_start(today)))
        );
        assert!(
            !store
                .data
                .days
                .contains_key(&utc_date_key(day_start(today - DAY_LIMIT as i64 - 10))),
            "самые старые даты отброшены вместе с началом порядка"
        );
        let oldest = store.data.days.keys().next().unwrap().clone();
        assert!(
            store
                .data
                .days
                .keys()
                .all(|date| date.as_str() >= oldest.as_str()),
            "ключи остаются упорядоченными по возрастанию"
        );
    }

    #[test]
    fn metadata_refresh_keeps_counters() {
        let mut store = StatsStore::ephemeral();
        let now = unix_now();
        let mut thin = station(UUID);
        thin.name = String::new();
        thin.countrycode = String::new();
        thin.tags = String::new();

        store.record(&thin, 300, now);
        store.note_start(&thin, now);
        assert_eq!(store.data.stations[0].name, "");

        let rich = station(UUID);
        store.record(&rich, 60, now + 1);
        let saved = &store.data.stations[0];
        assert_eq!(saved.name, format!("Station {UUID}"));
        assert_eq!(saved.countrycode, "DE");
        assert_eq!(saved.tags, "jazz");
        assert_eq!(saved.seconds, 360);
        assert_eq!(saved.sessions, 1);
        assert_eq!(store.snapshot().total_seconds, 360);
    }

    #[test]
    fn tags_are_clipped_to_128_bytes_on_tag_boundary() {
        let tag = "é".repeat(10); // 20 байт в UTF-8
        let eight = vec![tag.clone(); 8].join(",");
        assert_eq!(clip_tags(&eight), vec![tag.clone(); 6].join(","));
        assert_eq!(clip_tags(&eight).len(), 125);
        assert!(clip_tags(&tag.repeat(20)).is_empty());
        assert!(clip_tags("").is_empty());

        let mut store = StatsStore::ephemeral();
        let mut wide = station(UUID);
        wide.tags = eight;
        store.record(&wide, 60, unix_now());
        let saved = &store.data.stations[0];
        assert!(saved.tags.len() <= TAGS_BYTE_LIMIT);
        assert!(
            saved.tags.split(',').all(|value| value == tag.as_str()),
            "усечение не разрывает тег посередине"
        );
    }

    #[test]
    fn external_key_hides_the_url() {
        // Векторы FNV-1a 64-бит: ключ не зависит от имени и повторяем.
        assert_eq!(fnv1a_64(""), FNV_OFFSET_BASIS);
        assert_eq!(fnv1a_64("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a_64("foobar"), 0x8594_4171_f739_67e8);

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("stats.json");
        let mut stream = station(&format!("url:{:016x}", fnv1a_64(STREAM_URL)));
        stream.url = STREAM_URL.to_owned();
        stream.url_resolved = STREAM_URL.to_owned();
        stream.name = "Камерный зал".to_owned();
        stream.tags = String::new();
        stream.countrycode = String::new();
        stream.source = StationSource::Stream;

        let now = unix_now();
        let mut store = StatsStore::load(path.clone()).unwrap();
        store.note_start(&stream, now);
        store.record(&stream, FLUSH_SECS, now);
        store.flush().unwrap();

        let key = external_key(STREAM_URL);
        assert_eq!(key.len(), EXTERNAL_PREFIX.len() + 16);
        assert_eq!(store.data.stations[0].uuid, key);
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains(&key));
        for secret in ["127.0.0.1", "8765", "tone.wav", STREAM_URL] {
            assert!(!raw.contains(secret), "адрес не попадает в файл: {secret}");
        }

        // Другой процесс с тем же адресом получает тот же ключ.
        let mut second = StatsStore::ephemeral();
        second.record(&stream, 5, now);
        assert_eq!(second.data.stations[0].uuid, key);
    }

    #[test]
    fn flush_writes_once_per_threshold() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("stats.json");
        let mut store = StatsStore::load(path.clone()).unwrap();
        let saved = station(UUID);
        let now = unix_now();

        // Порог `FLUSH_SECS` выбирает Core: хранилище ниже порога не трогается,
        // пока никто не позвал `flush`.
        store.record(&saved, FLUSH_SECS - 1, now);
        assert!(store.pending() < FLUSH_SECS);
        assert!(!path.exists(), "ниже порога записи нет");

        store.record(&saved, 1, now);
        assert_eq!(store.pending(), FLUSH_SECS);
        store.flush().unwrap();
        assert_eq!(store.pending(), 0);
        assert_eq!(
            fs::read_to_string(&path)
                .unwrap()
                .matches("\"seconds\"")
                .count(),
            1
        );
        assert!(
            !directory.path().join(TMP_FILE_NAME).exists(),
            "временный файл убран"
        );
        assert_eq!(
            StatsStore::load(path.clone())
                .unwrap()
                .snapshot()
                .total_seconds,
            30
        );

        // Без накопленного `flush` — no-op: фоновой записи модуль не ведёт.
        fs::remove_file(&path).unwrap();
        store.flush().unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn clear_is_idempotent_and_writes_empty_schema() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("stats.json");
        let mut store = StatsStore::load(path.clone()).unwrap();

        store.clear().unwrap();
        let first = fs::read_to_string(&path).unwrap();
        store.clear().unwrap();
        let second = fs::read_to_string(&path).unwrap();
        for document in [first, second] {
            let value: serde_json::Value = serde_json::from_str(&document).unwrap();
            assert_eq!(value["schema"], SCHEMA);
            assert_eq!(value["stations"], serde_json::Value::Array(Vec::new()));
            assert_eq!(value["days"], serde_json::Value::Object(Default::default()));
        }
        assert!(!directory.path().join(TMP_FILE_NAME).exists());

        store.record(&station(UUID), 600, unix_now());
        store.flush().unwrap();
        store.clear().unwrap();
        assert_eq!(store.snapshot().total_seconds, 0);
        assert_eq!(store.pending(), 0);
        let restored = StatsStore::load(path).unwrap();
        assert_eq!(restored.snapshot().total_seconds, 0);
        assert!(!restored.damaged_at_load());
    }

    #[test]
    fn snapshot_ranks_top_ten_and_windows() {
        let mut store = StatsStore::ephemeral();
        let now = unix_now();
        let today = epoch_day(now);

        let mut totals = [0u64; 5];
        let mut countries = [0u64; 2];
        let count = TOP_LIMIT + 2;
        for index in 0..count {
            let mut saved = station(&format!("station-{index:02}"));
            saved.countrycode = if index % 2 == 0 {
                "DE".into()
            } else {
                "us".into()
            };
            saved.tags = format!("tag{},tag{}", index % 3, index % 5);
            let seconds = u64::try_from(count - index).unwrap() * 10;
            totals[index % 3] += seconds;
            totals[index % 5] += seconds;
            countries[index % 2] += seconds;
            store.record(&saved, seconds, now);
        }
        // Дни с запасом по границам окон: тест не должен зависеть от того,
        // успеет ли UTC-полночь сместить «сегодня» на сутки. Станция для
        // дневных бакетов без страны и тегов — иначе сводки поплыли бы.
        let mut counter = station(SECOND_UUID);
        counter.countrycode = String::new();
        counter.tags = String::new();
        for (age, seconds) in [(2, 100), (10, 200), (25, 300), (35, 400), (45, 500)] {
            store.record(&counter, seconds, day_start(today - age));
        }

        let snapshot = store.snapshot();
        let stations_seconds: u64 =
            u64::try_from((1..=count).map(|value| value * 10).sum::<usize>()).unwrap();
        let total = stations_seconds + 1_500;
        assert_eq!(snapshot.total_seconds, total);
        // Топ-10 по `seconds`: дневный счётчик перевешивает все станции.
        assert_eq!(snapshot.stations.len(), TOP_LIMIT);
        assert_eq!(snapshot.stations[0].uuid, SECOND_UUID);
        assert_eq!(snapshot.stations[0].seconds, 1_500);
        assert_eq!(snapshot.stations[1].uuid, "station-00");
        assert_eq!(
            snapshot.stations[1].seconds,
            u64::try_from(count).unwrap() * 10
        );
        assert_eq!(snapshot.stations[TOP_LIMIT - 1].uuid, "station-08");

        assert_eq!(
            snapshot
                .countries
                .iter()
                .map(|row| (row.code.as_str(), row.seconds))
                .collect::<Vec<_>>(),
            vec![("DE", countries[0]), ("US", countries[1]),],
            "код страны нормализуется, сводка по убыванию секунд"
        );
        let mut expected_tags: Vec<u64> = totals.into_iter().collect();
        expected_tags.sort_unstable_by(|left, right| right.cmp(left));
        assert_eq!(
            snapshot
                .tags
                .iter()
                .map(|row| row.seconds)
                .collect::<Vec<_>>(),
            expected_tags
        );
        assert_eq!(snapshot.tags[0].tag, "tag0");

        assert_eq!(snapshot.week_seconds, stations_seconds + 100);
        assert_eq!(snapshot.month_seconds, stations_seconds + 100 + 200 + 300);
        // Окно — 30 UTC-дат: сегодня плюс бакиеты возрастом 2, 10 и 25 суток;
        // 35 и 45 уже за границей месяца.
        assert_eq!(snapshot.days.len(), 4);
        assert_eq!(snapshot.days[0].date, utc_date_key(now));
        assert_eq!(snapshot.days[0].seconds, stations_seconds);
        assert!(
            snapshot
                .days
                .windows(2)
                .all(|pair| pair[0].date > pair[1].date),
            "дни идут от новых к старым"
        );
    }

    #[test]
    fn ephemeral_store_keeps_counters_without_file() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = StatsStore::ephemeral();
        assert!(store.path().is_none());

        store.note_start(&station(UUID), unix_now());
        store.record(&station(UUID), 120, unix_now());
        store.flush().unwrap();
        store.record(&station(UUID), 60, unix_now());
        store.flush().unwrap();
        assert_eq!(store.pending(), 0);
        assert_eq!(store.snapshot().total_seconds, 180);
        assert!(
            fs::read_dir(directory.path()).unwrap().next().is_none(),
            "хранилище в памяти не создаёт файла"
        );
    }
}
