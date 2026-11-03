//! Локальная статистика прослушивания: накопление секунд активной станции и
//! ответы `stats` / `stats --json` / `stats clear` (ADR-0049, интерфейс 17).

use std::time::{Duration, Instant};

use serde::Serialize;

use crate::audio::PlaybackState;
use crate::i18n::catalog;
use crate::services::Station;
use crate::settings::Language;
use crate::stats::{self, FLUSH_SECS, StatsSnapshot};

use super::Session;
use super::notice::NoticeKey;

/// Сколько строк каждого списка печатает человекочитаемый `stats`.
const TEXT_TOP: usize = 5;

/// Верхние списки ответа `stats --json` (интерфейс 17 § 5).
const JSON_TOP: usize = 10;

/// Отрезок эфира одной станции: кому принадлежат секунды и от каких часов они
/// считаются. Выделен из `Session`, потому что `Instant` здесь подставляется
/// извне — как часы `sleep::SleepTimer` (интерфейс 10 § 10).
#[derive(Debug, Default)]
pub(crate) struct Segment {
    station: Option<Station>,
    /// Момент последних засчитанных секунд; дробный остаток живёт тут же.
    clock: Option<Instant>,
}

impl Segment {
    fn station(&self) -> Option<&Station> {
        self.station.as_ref()
    }

    /// Отрезок принадлежит другой станции, чем `key`: нужен внеочередной сброс.
    fn is_other(&self, key: &str) -> bool {
        self.station
            .as_ref()
            .is_some_and(|station| stats::station_key(station) != key)
    }

    /// Открывает отрезок станции: часы стартуют `now`, если отрезка ещё нет.
    fn begin(&mut self, station: Station, now: Instant) {
        self.station = Some(station);
        self.clock.get_or_insert(now);
    }

    /// Снимает целые секунды с последней отметки и возвращает станцию отрезка.
    /// Остаток меньше секунды остаётся на часах, поэтому долгий эфир не теряет
    /// время на каждом кадре, а короткий отрезок не засчитывается вовсе.
    fn take_seconds(&mut self, now: Instant) -> (Option<Station>, u64) {
        let Some(started) = self.clock else {
            return (None, 0);
        };
        let elapsed = now.saturating_duration_since(started).as_secs();
        if elapsed > 0 {
            self.clock = Some(started + Duration::from_secs(elapsed));
        }
        (self.station.clone(), elapsed)
    }

    fn end(&mut self) {
        self.station = None;
        self.clock = None;
    }
}

/// Засчитывается ли время в этом состоянии аудио. Пауза, загрузка, реконнект,
/// ошибка и стоп — не считаются; громкость не важна (ADR-0049 § 1).
fn counts(state: &PlaybackState) -> bool {
    matches!(state, PlaybackState::Playing)
}

impl Session {
    /// Считает секунды состояния `Playing` активной станции и пишет их на диск
    /// пограничными событиями: порог `FLUSH_SECS`, уход из эфира, смена станции.
    pub(crate) fn poll_stats(&mut self) {
        if !self.settings.settings().stats_enabled {
            self.stats_segment.end();
            return;
        }
        let now = Instant::now();
        let active = counts(&self.audio.state())
            .then(|| self.active_station().cloned())
            .flatten();

        let Some(station) = active else {
            if self.stats_segment.station().is_some() {
                // Уход из `Playing` — внеочередная запись: пауза и стоп не
                // должны оставлять счётчики только в памяти (§ 4).
                self.credit_stats(now);
                self.stats_segment.end();
                self.flush_stats();
            }
            return;
        };

        if self.stats_segment.is_other(&stats::station_key(&station)) {
            self.credit_stats(now);
            self.stats_segment.end();
            self.flush_stats();
        }
        self.stats_segment.begin(station, now);
        self.credit_stats(now);

        if self.stats.pending() >= FLUSH_SECS {
            self.flush_stats();
        }
    }

    /// Ближайшая пробудка простоя ради статистики: пока играет станция и учёт
    /// включён, раз в интервал сброса на диск (§ 4). Иначе — `None`, и пустой
    /// процесс не просыпается вхолостую.
    pub(crate) fn stats_deadline(&self) -> Option<Instant> {
        if !self.settings.settings().stats_enabled || !counts(&self.audio.state()) {
            return None;
        }
        Some(Instant::now() + Duration::from_secs(FLUSH_SECS))
    }

    /// Запуск станции засчитывается в `sessions`: счётчик отличает один вечер
    /// фоном от десяти включений по минуте (интерфейс 17 § 2).
    pub(crate) fn note_stats_start(&mut self, station: &Station) {
        if !self.settings.settings().stats_enabled {
            return;
        }
        self.stats.note_start(station, stats::unix_now());
    }

    /// Включение и выключение учёта (интерфейс 09). При выключении отрезок
    /// закрывается и накопленное уходит на диск, но файл не удаляется:
    /// счётчики стирает только `stats clear`.
    pub(crate) fn set_stats_enabled(&mut self, enabled: bool) -> Result<(), String> {
        if !enabled {
            self.credit_stats(Instant::now());
        }
        let result = self
            .settings
            .set_stats_enabled(enabled)
            .map_err(|err| err.to_string());
        if !enabled {
            self.stats_segment.end();
            self.flush_stats();
        }
        result
    }

    /// `stats clear`: пустая схема 1 на диске, идемпотентно (интерфейс 17 § 6).
    pub(crate) fn clear_stats(&mut self) -> Result<(), String> {
        self.stats_segment.end();
        self.stats.clear().map_err(|err| err.to_string())
    }

    /// Снимок для окна настроек: раздел рисует готовые сводки, а не пересобирает
    /// их из файла на кадр.
    pub(crate) fn stats_snapshot(&self) -> StatsSnapshot {
        self.stats.snapshot()
    }

    /// Сброс статистики при завершении процесса (§ 4).
    pub(crate) fn flush_stats_on_exit(&mut self) {
        self.credit_stats(Instant::now());
        self.flush_stats();
    }

    /// Повреждённый при старте файл: счётчики начались с нуля, и это надо
    /// показать один раз (ADR-0049 § 2).
    pub(crate) fn stats_damaged(&self) -> bool {
        self.stats.damaged_at_load()
    }

    /// Отдаёт прошедшие секунды станции текущего отрезка хранилищу.
    fn credit_stats(&mut self, now: Instant) {
        let (station, seconds) = self.stats_segment.take_seconds(now);
        if let Some(station) = station {
            self.stats.record(&station, seconds, stats::unix_now());
        }
    }

    /// Ошибка записи не мешает слушать: счётчики остаются в памяти, тост
    /// говорит, что не сохранилось (ADR-0049 § 2).
    fn flush_stats(&mut self) {
        if let Err(err) = self.stats.flush() {
            self.notify_error_detail(NoticeKey::SettingNotSaved, err.to_string());
        }
    }
}

/// Ответ `stats`: суммарное время, признак учёта и три списка топ-5
/// (интерфейс 05).
pub(crate) fn format_stats_text(snapshot: &StatsSnapshot, enabled: bool) -> String {
    let mut lines = vec![
        format!("stats total {}", snapshot.total_seconds),
        format!("stats enabled {enabled}"),
    ];
    for station in snapshot.stations.iter().take(TEXT_TOP) {
        lines.push(format!(
            "stats station {} {}",
            station.seconds,
            crate::text::single_line(&station.name)
        ));
    }
    for country in snapshot.countries.iter().take(TEXT_TOP) {
        lines.push(format!(
            "stats country {} {}",
            country.seconds, country.code
        ));
    }
    for tag in snapshot.tags.iter().take(TEXT_TOP) {
        lines.push(format!("stats tag {} {}", tag.seconds, tag.tag));
    }
    lines.join("\n")
}

/// Версия схемы ответа `stats --json` (ADR-0049 § 6).
pub(crate) const STATS_SCHEMA: u32 = 1;

#[derive(Debug, Serialize)]
struct StatsPayload {
    schema: u32,
    app: &'static str,
    version: &'static str,
    enabled: bool,
    total_seconds: u64,
    week_seconds: u64,
    month_seconds: u64,
    stations: Vec<StatsStation>,
    countries: Vec<StatsCountry>,
    tags: Vec<StatsTag>,
    days: Vec<StatsDay>,
    ui: StatsUi,
}

#[derive(Debug, Serialize)]
struct StatsStation {
    uuid: String,
    name: String,
    seconds: u64,
    sessions: u64,
    last_at: u64,
}

#[derive(Debug, Serialize)]
struct StatsCountry {
    code: String,
    seconds: u64,
}

#[derive(Debug, Serialize)]
struct StatsTag {
    tag: String,
    seconds: u64,
}

#[derive(Debug, Serialize)]
struct StatsDay {
    date: String,
    seconds: u64,
}

/// Подписи раздела на языке приложения: потребитель не пересобирает перевод
/// сам (интерфейс 12).
#[derive(Debug, Serialize)]
struct StatsUi {
    total: &'static str,
    stations: &'static str,
    countries: &'static str,
    tags: &'static str,
    clear: &'static str,
}

impl StatsUi {
    fn new(language: Language) -> Self {
        let settings = &catalog(language).settings;
        Self {
            total: settings.stats_total,
            stations: settings.stats_stations,
            countries: settings.stats_countries,
            tags: settings.stats_tags,
            clear: settings.stats_clear,
        }
    }
}

/// Сборка ответа `stats --json` (интерфейс 17 § 5): сводки считает приложение,
/// читатель только печатает.
pub(crate) fn build_stats_payload(
    snapshot: &StatsSnapshot,
    enabled: bool,
    language: Language,
) -> String {
    let payload = StatsPayload {
        schema: STATS_SCHEMA,
        app: "orbitfm",
        version: env!("CARGO_PKG_VERSION"),
        enabled,
        total_seconds: snapshot.total_seconds,
        week_seconds: snapshot.week_seconds,
        month_seconds: snapshot.month_seconds,
        stations: snapshot
            .stations
            .iter()
            .take(JSON_TOP)
            .map(|station| StatsStation {
                uuid: station.uuid.clone(),
                name: station.name.clone(),
                seconds: station.seconds,
                sessions: station.sessions,
                last_at: station.last_at,
            })
            .collect(),
        countries: snapshot
            .countries
            .iter()
            .take(JSON_TOP)
            .map(|country| StatsCountry {
                code: country.code.clone(),
                seconds: country.seconds,
            })
            .collect(),
        tags: snapshot
            .tags
            .iter()
            .take(JSON_TOP)
            .map(|tag| StatsTag {
                tag: tag.tag.clone(),
                seconds: tag.seconds,
            })
            .collect(),
        days: snapshot
            .days
            .iter()
            .map(|day| StatsDay {
                date: day.date.clone(),
                seconds: day.seconds,
            })
            .collect(),
        ui: StatsUi::new(language),
    };
    serde_json::to_string(&payload).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use crate::stats::{CountrySummary, DaySummary, StationSummary, StatsStore, TagSummary};

    use super::*;

    /// Станция каталога: для отрезка важны `stationuuid` и имя, остальное —
    /// заполнитель, чтобы конструктор был честным.
    fn radio_station(uuid: &str, name: &str) -> Station {
        Station {
            stationuuid: uuid.to_owned(),
            name: name.to_owned(),
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
            source: crate::services::StationSource::Radio,
        }
    }

    fn station(index: usize, seconds: u64, name: &str) -> StationSummary {
        StationSummary {
            uuid: format!("uuid-{index}"),
            name: name.to_owned(),
            seconds,
            sessions: u64::try_from(index).unwrap_or(0) + 1,
            last_at: 1_762_150_000,
        }
    }

    /// Снимок с `count` станциями: секунды убывают, поэтому лидер всегда первый.
    fn snapshot(count: usize) -> StatsSnapshot {
        StatsSnapshot {
            total_seconds: 12_345,
            week_seconds: 600,
            month_seconds: 4_500,
            stations: (0..count)
                .map(|index| station(index, 9_000 - u64::try_from(index).unwrap_or(0), "Radio"))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            countries: vec![
                CountrySummary {
                    code: "CH".to_owned(),
                    seconds: 8_000,
                },
                CountrySummary {
                    code: "DE".to_owned(),
                    seconds: 2_000,
                },
            ]
            .into_boxed_slice(),
            tags: vec![TagSummary {
                tag: "jazz".to_owned(),
                seconds: 300,
            }]
            .into_boxed_slice(),
            days: (0..count)
                .map(|index| DaySummary {
                    date: format!("2026-11-{:02}", index + 1),
                    seconds: u64::try_from(index).unwrap_or(0) * 60,
                })
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        }
    }

    #[test]
    fn text_report_leads_with_totals_and_enabled_flag() {
        let report = format_stats_text(&snapshot(3), true);
        let lines: Vec<&str> = report.lines().collect();
        assert_eq!(lines[0], "stats total 12345");
        assert_eq!(lines[1], "stats enabled true");
        assert!(lines.contains(&"stats station 9000 Radio"));
        assert!(lines.contains(&"stats country 8000 CH"));
        assert!(lines.contains(&"stats tag 300 jazz"));
    }

    #[test]
    fn text_report_caps_each_list_at_five() {
        let report = format_stats_text(&snapshot(9), false);
        assert_eq!(report.matches("stats station ").count(), TEXT_TOP);
        assert_eq!(report.matches("stats country ").count(), 2);
        assert!(!report.contains("stats enabled true"));
    }

    #[test]
    fn text_report_keeps_line_structure_of_multiline_names() {
        let mut snap = snapshot(1);
        snap.stations = vec![station(0, 60, "Первая\nвторая")].into_boxed_slice();
        let report = format_stats_text(&snap, true);
        // Строка протокола — одна строка: перенос в имени не плодит лишнего ответа.
        assert_eq!(report.lines().count(), 2 + 1 + 2 + 1);
        assert!(report.contains("stats station 60 "));
    }

    #[test]
    fn json_payload_keeps_schema_one_fields() {
        let raw = build_stats_payload(&snapshot(3), true, Language::English);
        let value: serde_json::Value =
            serde_json::from_str(&raw).expect("stats --json должен разбираться");
        assert_eq!(value["schema"], STATS_SCHEMA);
        assert_eq!(value["app"], "orbitfm");
        assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(value["enabled"], true);
        assert_eq!(value["total_seconds"], 12_345);
        assert_eq!(value["week_seconds"], 600);
        assert_eq!(value["month_seconds"], 4_500);
        assert_eq!(value["stations"][0]["uuid"], "uuid-0");
        assert_eq!(value["countries"][0]["code"], "CH");
        assert_eq!(value["tags"][0]["tag"], "jazz");
        assert_eq!(value["days"][0]["date"], "2026-11-01");
        assert_ne!(value["ui"]["total"], serde_json::Value::Null);
    }

    #[test]
    fn json_payload_caps_stations_and_passes_days_through() {
        let raw = build_stats_payload(&snapshot(40), false, Language::English);
        let value: serde_json::Value = serde_json::from_str(&raw).expect("статистика схемы 1");
        assert_eq!(value["stations"].as_array().map(Vec::len), Some(JSON_TOP));
        // Дат снимок уже принёс не больше `DAY_WINDOW_LIMIT`; ответ их не режет.
        assert_eq!(value["days"].as_array().map(Vec::len), Some(40));
        assert_eq!(value["enabled"], false);
    }

    #[test]
    fn json_payload_labels_follow_catalog_language() {
        let english = build_stats_payload(&snapshot(1), true, Language::English);
        let russian = build_stats_payload(&snapshot(1), true, Language::Russian);
        let ui = |raw: &str| -> String {
            let value: serde_json::Value = serde_json::from_str(raw).unwrap();
            value["ui"]["stations"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        };
        assert!(!ui(&english).is_empty());
        assert!(!ui(&russian).is_empty());
        assert_ne!(ui(&english), ui(&russian));
    }

    #[test]
    fn payload_is_single_line() {
        // Ответ IPC — одна строка: переносы поломали бы протокол.
        let raw = build_stats_payload(&snapshot(3), true, Language::English);
        assert!(!raw.contains('\n'));
    }

    #[test]
    fn only_playing_state_counts() {
        // Пауза, загрузка, реконнект, ошибка и стоп не двигают счётчик
        // (интерфейс 10 § 3.2).
        assert!(counts(&PlaybackState::Playing));
        for state in [
            PlaybackState::Stopped,
            PlaybackState::Paused,
            PlaybackState::Buffering,
            PlaybackState::Reconnecting,
            PlaybackState::Error("нет сети".to_owned()),
        ] {
            assert!(!counts(&state), "{state:?} не считается");
        }
    }

    #[test]
    fn segment_credits_whole_seconds_and_keeps_remainder() {
        let started = Instant::now();
        let mut segment = Segment::default();
        segment.begin(radio_station("aaaa", "Первая"), started);

        // Кадр раньше секунды — времени не засчитано, часы стоят на месте.
        let (station, seconds) = segment.take_seconds(started + Duration::from_millis(400));
        assert_eq!(seconds, 0);
        assert_eq!(station.unwrap().stationuuid, "aaaa");
        assert_eq!(segment.clock, Some(started));

        // Прошло 2.9 с: снимаются целые 2 с, остаток 0.9 живёт на часах.
        let (_, seconds) = segment.take_seconds(started + Duration::from_millis(2_900));
        assert_eq!(seconds, 2);
        assert_eq!(segment.clock, Some(started + Duration::from_secs(2)));
        // Следующий кадр через 0.3 с добивает остаток до секунды.
        let (_, seconds) = segment.take_seconds(started + Duration::from_millis(3_200));
        assert_eq!(seconds, 1);
        assert_eq!(segment.clock, Some(started + Duration::from_secs(3)));
    }

    #[test]
    fn closed_segment_credits_nothing() {
        let mut segment = Segment::default();
        let (station, seconds) = segment.take_seconds(Instant::now());
        assert!(station.is_none());
        assert_eq!(seconds, 0);
        segment.begin(radio_station("bbbb", "Вторая"), Instant::now());
        segment.end();
        assert!(segment.station().is_none());
        assert!(segment.clock.is_none());
    }

    #[test]
    fn other_station_is_detected_by_stats_key() {
        let mut segment = Segment::default();
        segment.begin(radio_station("cccc", "Та же"), Instant::now());
        assert!(!segment.is_other(&stats::station_key(&radio_station(
            "cccc",
            "Переименованная"
        ))));
        assert!(segment.is_other(&stats::station_key(&radio_station("dddd", "Другая"))));
        // Пустой отрезок «другой» не считается: сбрасывать на диск нечего.
        assert!(!Segment::default().is_other("любой ключ"));
    }

    #[test]
    fn thirty_seconds_of_air_reach_the_flush_threshold() {
        let mut store = StatsStore::ephemeral();
        let mut segment = Segment::default();
        let started = Instant::now();
        segment.begin(radio_station("eeee", "Эфир"), started);
        // Пионерский цикл `poll_stats`: по кадру в секунду, 30 секунд эфира.
        for frame in 1..=FLUSH_SECS {
            let (station, seconds) = segment.take_seconds(started + Duration::from_secs(frame));
            let station = station.expect("отрезок открыт");
            store.record(&station, seconds, stats::unix_now());
        }
        assert_eq!(store.pending(), FLUSH_SECS);
        // Ровно на пороге Core пишет файл; у хранилища без файла это no-op,
        // который гасит `pending`.
        store.flush().expect("запись ephemeral не падает");
        assert_eq!(store.pending(), 0);
    }
}
