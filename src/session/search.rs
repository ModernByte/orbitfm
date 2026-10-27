//! Поиск станций, выбор страны/жанра и рулетка.
//!
//! Ответы приходят из фонового tokio-задания и маркированы поколением: ответ
//! устаревшего запроса отбрасывается, чтобы быстрый `Esc`-переполск не
//! перерисовывал панель старым списком.

use tokio::sync::mpsc;
use tracing::warn;

use crate::geo::Country;
use crate::services::{RadioError, SearchParams, Station};
use crate::ui::single_line;

use super::LibraryTab;
use super::markers::build_markers;
use super::notice::NoticeKey;

/// Станций в результатах поиска по имени.
const SEARCH_RESULT_LIMIT: u32 = 100;
/// Станций в списке выбранной страны.
const COUNTRY_RESULT_LIMIT: u32 = 100;
/// Станций в глобальном поиске по жанру.
const GENRE_RESULT_LIMIT: u32 = 200;
/// Сколько летит камера к станции, выпавшей в рулетке.
const ROULETTE_FLY_MS: u64 = 900;
/// Таймаут одиночного запроса станции по UUID.
pub(crate) const STATION_LOOKUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

/// Активный режим поиска станций.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum SearchMode {
    /// Страна на глобусе → станции `countrycode`.
    #[default]
    Country,
    /// Глобальный жанр → станции `tag` по всему миру.
    GlobalGenre,
}

/// Активный контекст поиска: страна или глобальный жанр.
#[derive(Debug, Default)]
pub(crate) struct SearchContext {
    pub(crate) mode: SearchMode,
    pub(crate) country: Option<Country>,
    pub(crate) genre: Option<String>,
}

impl SearchContext {
    /// Переключает режим и сбрасывает выбранную страну/жанр.
    fn switch_mode(&mut self, mode: SearchMode) -> bool {
        if self.mode == mode {
            return false;
        }
        self.mode = mode;
        self.clear();
        true
    }

    pub(crate) fn clear(&mut self) {
        self.country = None;
        self.genre = None;
    }

    pub(crate) fn is_active(&self) -> bool {
        self.country.is_some() || self.genre.is_some()
    }

    fn activate_country(&mut self, country: Country) {
        self.mode = SearchMode::Country;
        self.genre = None;
        self.country = Some(country);
    }

    fn activate_genre(&mut self, genre: String) {
        self.mode = SearchMode::GlobalGenre;
        self.country = None;
        self.genre = Some(genre);
    }
}

/// Всё, что связано с поиском: строки запросов, результат, незавершённые задания.
#[derive(Debug, Default)]
pub(crate) struct SearchState {
    pub(crate) query: String,
    pub(crate) country_query: String,
    pub(crate) genre_query: String,
    pub(crate) context: SearchContext,
    pub(crate) results: Vec<Station>,
    pub(crate) generation: u64,
    pub(crate) pending: bool,
    pub(crate) empty: bool,
    pub(crate) rx: Option<mpsc::UnboundedReceiver<SearchResponse>>,
    pub(crate) error: Option<String>,
    pub(crate) roulette_pending: bool,
    pub(crate) roulette_rx: Option<mpsc::UnboundedReceiver<Result<Station, RadioError>>>,
}

/// Две выборки одного запроса: обычная и только со станциями, у которых есть координаты.
struct SearchOutcome {
    stations: Vec<Station>,
    geo_stations: Vec<Station>,
}

/// Результат фонового поиска с номером поколения.
pub(crate) struct SearchResponse {
    generation: u64,
    result: Result<SearchOutcome, RadioError>,
}

impl SearchResponse {
    fn is_current(&self, generation: u64) -> bool {
        self.generation == generation
    }
}

impl super::Session {
    /// Поиск по имени: при активном контексте страны/жанра работает локальным фильтром.
    pub(crate) fn start_search(&mut self) {
        let query = self.search.query.trim().to_owned();
        if query.is_empty() {
            return;
        }

        if self.search.context.is_active() {
            self.country_filter.text = query;
            return;
        }
        if self.search.pending {
            return;
        }

        self.search.context.clear();
        self.begin_search(
            SearchParams {
                name: Some(query),
                limit: SEARCH_RESULT_LIMIT,
                hide_broken: true,
                ..SearchParams::default()
            },
            SEARCH_RESULT_LIMIT,
        );
    }

    /// Повтор последнего запроса из карточки ошибки: контекст страны или жанра
    /// важнее строки имени — именно он сформировал упавший запрос.
    pub(crate) fn retry_search(&mut self) {
        if let Some(country) = self.search.context.country.clone() {
            self.start_country_search(country);
            return;
        }
        if let Some(tag) = self.search.context.genre.clone() {
            self.start_genre_search(tag);
            return;
        }
        if self.search.error.is_some() {
            self.start_search();
        }
    }

    pub(crate) fn start_country_search(&mut self, country: Country) {
        let iso_a2 = country.iso_a2.clone();
        self.search.context.activate_country(country);
        self.country_filter.reset();
        let limit = COUNTRY_RESULT_LIMIT;
        self.begin_search(country_search_params(&iso_a2), limit);
        self.search.country_query.clear();
    }

    pub(crate) fn start_genre_search(&mut self, tag: String) {
        let tag = tag.trim().to_lowercase();
        if tag.is_empty() {
            return;
        }

        self.search.context.activate_genre(tag.clone());
        self.country_filter.reset();
        self.begin_search(
            SearchParams {
                tag: Some(tag),
                limit: GENRE_RESULT_LIMIT,
                hide_broken: true,
                ..SearchParams::default()
            },
            GENRE_RESULT_LIMIT,
        );
        self.search.genre_query.clear();
    }

    pub(crate) fn switch_mode(&mut self, mode: SearchMode) {
        if !self.search.context.switch_mode(mode) {
            return;
        }

        self.invalidate_search();
        self.search.results.clear();
        self.markers.clear();
        self.country_filter.reset();
        self.library_tab = LibraryTab::Results;
    }

    /// Отбрасывает незавершённый поиск: его ответ больше не применится.
    fn invalidate_search(&mut self) {
        self.search.generation = self.search.generation.wrapping_add(1);
        self.search.rx = None;
        self.search.pending = false;
        self.search.empty = false;
        self.search.error = None;
    }

    fn begin_search(&mut self, params: SearchParams, geo_limit: u32) {
        self.search.generation = self.search.generation.wrapping_add(1);
        let generation = self.search.generation;
        let (sender, receiver) = mpsc::unbounded_channel();
        self.search.rx = Some(receiver);
        self.search.pending = true;
        self.search.empty = false;
        self.search.error = None;
        self.search.results.clear();
        self.markers.clear();
        self.library_tab = LibraryTab::Results;

        let mut geo_params = params.clone();
        geo_params.has_geo_info = true;
        geo_params.limit = geo_limit;

        let radio = self.radio.clone();
        let waker = self.wake.waker();
        self.runtime.spawn(async move {
            let (stations, geo_stations) =
                tokio::join!(radio.search(params), radio.search(geo_params));
            let result = match stations {
                Ok(stations) => {
                    let geo_stations = match geo_stations {
                        Ok(stations) => stations,
                        Err(err) => {
                            warn!(error = %err, "гео-поиск станций не удался");
                            Vec::new()
                        }
                    };
                    Ok(SearchOutcome {
                        stations,
                        geo_stations,
                    })
                }
                Err(err) => Err(err),
            };
            let _ = sender.send(SearchResponse { generation, result });
            waker();
        });
    }

    pub(crate) fn poll_search(&mut self) {
        let result = match self.search.rx.as_mut() {
            Some(receiver) => receiver.try_recv(),
            None => return,
        };

        match result {
            Ok(response) if !response.is_current(self.search.generation) => {
                tracing::debug!(
                    generation = response.generation,
                    current = self.search.generation,
                    "устаревший ответ поиска отброшен"
                );
            }
            Ok(response) => match response.result {
                Ok(outcome) => {
                    self.search.empty = outcome.stations.is_empty();
                    self.search.results = outcome.stations;
                    self.markers = build_markers(outcome.geo_stations);
                    self.search.pending = false;
                    self.search.rx = None;
                }
                Err(err) => {
                    self.search.error = Some(err.to_string());
                    self.markers.clear();
                    self.search.pending = false;
                    self.search.rx = None;
                }
            },
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                // Отправитель исчез — тот же путь, что у рулетки: ядро называет
                // причину ключом, строку берёт каталог (ADR-0030 §6).
                self.notify_error(NoticeKey::SearchTaskEnded);
                self.search.pending = false;
                self.search.rx = None;
            }
        }
    }

    pub(crate) fn start_roulette(&mut self) {
        if self.search.roulette_pending {
            return;
        }
        self.search.roulette_pending = true;
        let (sender, receiver) = mpsc::unbounded_channel();
        self.search.roulette_rx = Some(receiver);
        let radio = self.radio.clone();
        let waker = self.wake.waker();
        self.runtime.spawn(async move {
            let result = radio.random_station().await;
            let _ = sender.send(result);
            waker();
        });
    }

    pub(crate) fn poll_roulette(&mut self) {
        let outcome = match self.search.roulette_rx.as_mut() {
            Some(receiver) => match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::error::TryRecvError::Empty) => return,
                Err(mpsc::error::TryRecvError::Disconnected) => None,
            },
            None => return,
        };
        self.apply_roulette(outcome);
    }

    /// Применяет результат задачи рулетки: тот же путь для кадра UI и для
    /// припаркованного цикла, потому что рулетка из трея обязана работать и с
    /// закрытым окном (ADR-0020). `None` — отправитель исчез, станцию не получить.
    pub(crate) fn apply_roulette(&mut self, outcome: Option<Result<Station, RadioError>>) {
        match outcome {
            Some(Ok(station)) => {
                self.search.roulette_pending = false;
                self.search.roulette_rx = None;
                let label = roulette_label(&station);
                let target = station.geo_lat.zip(station.geo_long);
                match self.start_station(station) {
                    Ok(()) => {
                        if let Some((lat, lon)) = target {
                            self.globe_camera.fly_to(
                                lat,
                                lon,
                                std::time::Duration::from_millis(ROULETTE_FLY_MS),
                            );
                        }
                        self.notify_info(NoticeKey::RouletteStarted { station: label });
                    }
                    Err(err) => {
                        self.notify_error_detail(NoticeKey::RouletteFailed, err.to_string())
                    }
                }
            }
            Some(Err(err)) => {
                self.search.roulette_pending = false;
                self.search.roulette_rx = None;
                self.notify_error_detail(NoticeKey::RouletteFailed, err.to_string());
            }
            None => {
                self.search.roulette_pending = false;
                self.search.roulette_rx = None;
                self.notify_error(NoticeKey::RouletteTaskEnded);
            }
        }
    }
}

/// Запрос «только станции выбранной страны» (ТЗ §5.3): фильтр по ISO без примеси
/// других стран и мёртвых потоков.
fn country_search_params(iso_a2: &str) -> SearchParams {
    SearchParams {
        country_code: Some(iso_a2.to_owned()),
        limit: COUNTRY_RESULT_LIMIT,
        hide_broken: true,
        ..SearchParams::default()
    }
}

/// Подпись выпавшей станции: имя и страна, без пустых разделителей.
/// `None` — станция без имени и страны: UI подставит локализованный «Untitled».
fn roulette_label(station: &Station) -> Option<String> {
    let name = single_line(&station.name);
    let country = single_line(&station.country);
    match (name.is_empty(), country.is_empty()) {
        (false, false) => Some(format!("{name} — {country}")),
        (false, true) => Some(name),
        (true, false) => Some(country),
        (true, true) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::markers::sample_station;

    fn germany() -> Country {
        Country {
            name: "Germany".to_owned(),
            iso_a2: "DE".to_owned(),
            iso_a3: Some("DEU".to_owned()),
        }
    }

    #[test]
    fn switching_mode_resets_country_and_genre() {
        let mut context = SearchContext::default();
        assert_eq!(context.mode, SearchMode::Country);
        context.activate_country(germany());
        assert!(context.is_active());

        assert!(context.switch_mode(SearchMode::GlobalGenre));
        assert_eq!(context.mode, SearchMode::GlobalGenre);
        assert!(!context.is_active(), "страна должна сбрасываться");
        assert!(context.country.is_none());
        assert!(!context.switch_mode(SearchMode::GlobalGenre));
    }

    #[test]
    fn activating_genre_clears_country_and_vice_versa() {
        let mut context = SearchContext::default();
        context.activate_country(germany());
        context.activate_genre("jazz".to_owned());

        assert_eq!(context.mode, SearchMode::GlobalGenre);
        assert_eq!(context.genre.as_deref(), Some("jazz"));
        assert!(context.country.is_none(), "жанр сбрасывает страну");

        context.activate_country(germany());
        assert_eq!(context.mode, SearchMode::Country);
        assert!(context.genre.is_none(), "страна сбрасывает жанр");
    }

    #[test]
    fn stale_search_response_is_dropped() {
        let response = SearchResponse {
            generation: 7,
            result: Err(RadioError::NotFound),
        };
        assert!(response.is_current(7));
        assert!(
            !response.is_current(8),
            "ответ прошлого поколения не должен применяться"
        );
    }

    #[test]
    fn roulette_label_includes_country() {
        let mut station = sample_station();
        assert_eq!(
            roulette_label(&station).as_deref(),
            Some("Test Radio — United States")
        );

        station.country.clear();
        assert_eq!(roulette_label(&station).as_deref(), Some("Test Radio"));

        station.name.clear();
        assert_eq!(roulette_label(&station), None);
    }

    #[test]
    fn country_request_lists_only_that_country() {
        let params = country_search_params("de");
        assert_eq!(params.country_code.as_deref(), Some("de"));
        assert!(
            params.name.is_none() && params.tag.is_none() && params.language.is_none(),
            "выбор страны не должен подмешивать другие фильтры"
        );
        assert!(params.hide_broken, "мёртвые потоки страны не показываем");
        assert_eq!(params.limit, COUNTRY_RESULT_LIMIT);
    }
}
