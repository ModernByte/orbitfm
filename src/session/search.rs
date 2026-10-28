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
    /// Есть ли ещё страницы в текущей выборке: страна догружается по
    /// `offset`, пока не достигнут мировой счётчик (жалоба 2026-10-28).
    pub(crate) has_more: bool,
    /// Идёт догрузка следующей страницы: список остаётся на экране.
    pub(crate) loading_more: bool,
    /// Параметры текущей выборки: с них строится следующий запрос страницы.
    pub(crate) last_params: Option<SearchParams>,
    /// Сколько строк уже запрошено у API: offset следующей страницы.
    pub(crate) next_offset: u32,
    /// Клик по пустому месту глобуса снял выделение страны, но список станций
    /// остаётся: контекст поиска не трогаем, гаснет только подсветка.
    pub(crate) highlight_hidden: bool,
    /// Номер изменения выборки: тепловая карта стран пересобирается по нему,
    /// а не по указателю `Vec`, который может переиспользоваться.
    pub(crate) revision: u64,
}

/// Результат фонового поиска с номером поколения. Список и маркеры строятся
/// из одной выборки: всё, что видно в панели, обязано быть и на глобусе (ADR-0036).
pub(crate) struct SearchResponse {
    generation: u64,
    result: Result<Vec<Station>, RadioError>,
    /// `true` — следующая страница: строки дополняют список, а не заменяют.
    append: bool,
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
        self.begin_search(SearchParams {
            name: Some(query),
            limit: SEARCH_RESULT_LIMIT,
            hide_broken: true,
            ..SearchParams::default()
        });
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
        self.search.highlight_hidden = false;
        self.country_filter.reset();
        self.begin_search(country_search_params(&iso_a2));
        self.search.country_query.clear();
    }

    /// Снимает подсветку выбранной страны по клику мимо: контекст поиска и
    /// список станций остаются до выбора другой страны.
    pub(crate) fn hide_country_highlight(&mut self) {
        if self.search.context.country.is_some() {
            self.search.highlight_hidden = true;
        }
    }

    pub(crate) fn start_genre_search(&mut self, tag: String) {
        let tag = tag.trim().to_lowercase();
        if tag.is_empty() {
            return;
        }

        self.search.context.activate_genre(tag.clone());
        self.country_filter.reset();
        self.begin_search(SearchParams {
            tag: Some(tag),
            limit: GENRE_RESULT_LIMIT,
            hide_broken: true,
            ..SearchParams::default()
        });
        self.search.genre_query.clear();
    }

    pub(crate) fn switch_mode(&mut self, mode: SearchMode) {
        if !self.search.context.switch_mode(mode) {
            return;
        }

        self.invalidate_search();
        self.search.results.clear();
        self.markers.clear();
        self.search.revision = self.search.revision.wrapping_add(1);
        self.search.highlight_hidden = false;
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
        self.search.has_more = false;
        self.search.loading_more = false;
        self.search.last_params = None;
        self.search.next_offset = 0;
    }

    fn begin_search(&mut self, params: SearchParams) {
        self.search.generation = self.search.generation.wrapping_add(1);
        let generation = self.search.generation;
        let (sender, receiver) = mpsc::unbounded_channel();
        self.search.rx = Some(receiver);
        self.search.pending = true;
        self.search.empty = false;
        self.search.error = None;
        self.search.has_more = false;
        self.search.loading_more = false;
        self.search.results.clear();
        self.markers.clear();
        self.search.revision = self.search.revision.wrapping_add(1);
        self.library_tab = LibraryTab::Results;

        // Список — полная выдача страны с пагинацией; маркеры строятся из тех
        // же строк, но только с координатами (ADR-0036, ревизия 2026-10-28).
        // `has_geo_info=true` здесь отрезал бы 87 % базы и сделал «100 из 1244»
        // недостижимым.
        self.search.next_offset = params.offset;
        self.search.last_params = Some(params.clone());

        let radio = self.radio.clone();
        let waker = self.wake.waker();
        self.runtime.spawn(async move {
            let result = radio.search(params).await;
            if let Err(err) = &result {
                warn!(error = %err, "поиск станций не удался");
            }
            let _ = sender.send(SearchResponse {
                generation,
                result,
                append: false,
            });
            waker();
        });
    }

    /// Догружает следующую страницу текущей выборки. Список и маркеры при этом
    /// не гаснут: новые строки дополняют уже показанные (ADR-0036, ревизия
    /// 2026-10-28).
    pub(crate) fn load_more(&mut self) {
        if self.search.pending
            || self.search.loading_more
            || self.search.rx.is_some()
            || !self.search.has_more
        {
            return;
        }
        let Some(mut params) = self.search.last_params.clone() else {
            return;
        };
        params.offset = self.search.next_offset;
        let generation = self.search.generation;
        let (sender, receiver) = mpsc::unbounded_channel();
        self.search.rx = Some(receiver);
        self.search.loading_more = true;

        let radio = self.radio.clone();
        let waker = self.wake.waker();
        self.runtime.spawn(async move {
            let result = radio.search(params).await;
            if let Err(err) = &result {
                warn!(error = %err, "догрузка страницы станций не удалась");
            }
            let _ = sender.send(SearchResponse {
                generation,
                result,
                append: true,
            });
            waker();
        });
    }

    /// Счётчик станций выбранной страны из мирового кэша: по нему панель
    /// показывает «100 из 1244» и решает, есть ли ещё страницы.
    pub(crate) fn country_total(&self) -> Option<u32> {
        let iso_a2 = self.search.context.country.as_ref()?.iso_a2.to_uppercase();
        self.world.counts().get(&iso_a2).copied()
    }

    /// Обновляет признак «есть ещё страницы» по размеру пришедшей страницы и
    /// мировому счётчику страны, если он уже прогрет.
    fn update_has_more(&mut self, page_len: u32) {
        let limit = self
            .search
            .last_params
            .as_ref()
            .map_or(0, |params| params.limit);
        let total = self.country_total();
        self.search.has_more = page_has_more(page_len, limit, self.search.results.len(), total);
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
            Ok(response) => {
                let append = response.append;
                match response.result {
                    Ok(stations) if append => {
                        let page_len = stations.len() as u32;
                        append_unique(&mut self.search.results, stations);
                        self.search.next_offset = self.search.next_offset.saturating_add(page_len);
                        self.markers = build_markers(self.search.results.clone());
                        self.update_has_more(page_len);
                        self.search.revision = self.search.revision.wrapping_add(1);
                        self.search.loading_more = false;
                        self.search.rx = None;
                    }
                    Ok(stations) => {
                        let page_len = stations.len() as u32;
                        self.search.next_offset = self.search.next_offset.saturating_add(page_len);
                        self.search.empty = stations.is_empty();
                        self.markers = build_markers(stations.clone());
                        self.search.results = stations;
                        self.update_has_more(page_len);
                        self.search.revision = self.search.revision.wrapping_add(1);
                        self.search.pending = false;
                        self.search.rx = None;
                    }
                    Err(err) if append => {
                        // Список уже на экране: ошибка догрузки не должна его
                        // стирать, достаточно тоста с причиной.
                        self.notify_error_detail(NoticeKey::LoadMoreFailed, err.to_string());
                        self.search.loading_more = false;
                        self.search.rx = None;
                    }
                    Err(err) => {
                        self.search.error = Some(err.to_string());
                        self.markers.clear();
                        self.search.revision = self.search.revision.wrapping_add(1);
                        self.search.pending = false;
                        self.search.rx = None;
                    }
                }
            }
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                // Отправитель исчез — тот же путь, что у рулетки: ядро называет
                // причину ключом, строку берёт каталог (ADR-0030 §6).
                if self.search.loading_more {
                    self.notify_error(NoticeKey::LoadMoreFailed);
                    self.search.loading_more = false;
                } else {
                    self.notify_error(NoticeKey::SearchTaskEnded);
                    self.search.pending = false;
                }
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
        let exclude = self.roulette_exclusions();
        let waker = self.wake.waker();
        self.runtime.spawn(async move {
            let result = radio.random_station(&exclude).await;
            let _ = sender.send(result);
            waker();
        });
    }

    /// UUID, которые рулетка не должна выдать: текущий эфир и недавняя история.
    /// Без этого кубик «крутится», а станция остаётся прежней.
    fn roulette_exclusions(&self) -> Vec<String> {
        const RECENT: usize = 12;
        let mut exclude: Vec<String> = self
            .library
            .history()
            .iter()
            .take(RECENT)
            .map(|station| station.stationuuid.clone())
            .collect();
        if let Some(station) = &self.current_station {
            exclude.push(station.stationuuid.clone());
        }
        exclude
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

/// Есть ли смысл просить следующую страницу. При известном мировом счётчике
/// ориентир — он: сервер может отдать страницу короче лимита (мёртвые потоки
/// отсечены `hide_broken`), но это не значит, что выборка исчерпана. Пустая
/// страница закрывает пагинацию всегда; без счётчика — только полная страница
/// обещает продолжение.
fn page_has_more(page_len: u32, limit: u32, loaded: usize, total: Option<u32>) -> bool {
    if page_len == 0 {
        return false;
    }
    match total {
        Some(total) => loaded < total as usize,
        None => limit > 0 && page_len >= limit,
    }
}

/// Дополняет список страницей без повторных UUID: Radio Browser может
/// сдвинуть окно выдачи, пока пользователь листает.
fn append_unique(results: &mut Vec<Station>, page: Vec<Station>) {
    let mut known: std::collections::HashSet<String> = results
        .iter()
        .map(|station| station.stationuuid.clone())
        .collect();
    for station in page {
        if known.insert(station.stationuuid.clone()) {
            results.push(station);
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
            append: false,
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
        assert!(
            !params.has_geo_info,
            "список страны — полная выдача, маркеры лишь подмножество"
        );
        assert_eq!(params.limit, COUNTRY_RESULT_LIMIT);
    }

    #[test]
    fn pagination_follows_the_world_counter_until_exhausted() {
        assert!(page_has_more(100, 100, 100, Some(1244)), "100 из 1244");
        assert!(
            page_has_more(66, 100, 66, Some(1244)),
            "короткая страница не закрывает выборку при известном счётчике"
        );
        assert!(
            !page_has_more(100, 100, 1244, Some(1244)),
            "счётчик достигнут"
        );
        assert!(
            !page_has_more(0, 100, 300, Some(1244)),
            "пустая страница — конец"
        );
        assert!(
            page_has_more(100, 100, 100, None),
            "без счётчика — полная страница"
        );
        assert!(
            !page_has_more(66, 100, 66, None),
            "без счётчика — короткая страница"
        );
        assert!(
            !page_has_more(100, 0, 100, None),
            "нулевой лимит не зацикливает"
        );
    }

    #[test]
    fn appended_page_keeps_the_existing_list_and_skips_duplicates() {
        let mut station = sample_station();
        let mut results = vec![station.clone()];
        let mut duplicate = sample_station();
        duplicate.name = "Duplicate".to_owned();
        station.stationuuid = "uuid-2".to_owned();
        station.name = "Second".to_owned();
        append_unique(&mut results, vec![duplicate, station]);

        assert_eq!(results.len(), 2, "дубликат по UUID отброшен");
        assert_eq!(results[0].name, "Test Radio");
        assert_eq!(results[1].stationuuid, "uuid-2");
    }
}
