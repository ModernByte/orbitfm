//! Клиент Radio Browser API с DNS-discovery, failover и кэшем станций.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use reqwest::{Client, Response, StatusCode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use tokio::sync::{Mutex as AsyncMutex, RwLock};

use super::discovery::{API_BASE_URL, API_HOST, discover_servers};

pub(crate) const USER_AGENT: &str = "OrbitFM/1.0 (ModernByte; Linux; Rust)";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const STATION_CACHE_CAPACITY: usize = 512;
const RANDOM_ATTEMPTS: usize = 3;
/// Станций в пачке рулетки: сервер режет выборку сам, клиент добирает
/// разнообразие из пачки.
const ROULETTE_BATCH: u32 = 24;
/// Верхняя граница случайного `offset`: каждый цикл рулетки — новый URL, ответ
/// серверного кэша не переиспользуется.
const ROULETTE_OFFSET_SPAN: u32 = 500;

/// Параметры поиска станций.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchParams {
    pub name: Option<String>,
    /// ISO 3166-1 alpha-2.
    pub country_code: Option<String>,
    pub tag: Option<String>,
    pub language: Option<String>,
    pub limit: u32,
    pub offset: u32,
    pub hide_broken: bool,
    /// Только станции с известными координатами.
    pub has_geo_info: bool,
}

/// Источник станции: каталог Radio Browser или внешний поток, добавленный
/// командой `play-url` (ADR-0040). Поле аддитивно: старые `library.json`
/// читаются как радиостанции.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StationSource {
    #[default]
    Radio,
    Stream,
}

impl StationSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Radio => "radio",
            Self::Stream => "stream",
        }
    }
}

/// Станция в том виде, в котором её отдаёт Radio Browser.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Station {
    pub stationuuid: String,
    pub name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub url_resolved: String,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub homepage: Option<String>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub favicon: Option<String>,
    #[serde(default)]
    pub tags: String,
    #[serde(default)]
    pub countrycode: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub votes: u32,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub bitrate: u32,
    #[serde(default)]
    pub codec: String,
    #[serde(default)]
    pub geo_lat: Option<f64>,
    #[serde(default)]
    pub geo_long: Option<f64>,
    #[serde(default)]
    pub source: StationSource,
}

/// Сколько стран просит `/json/countrycodes`: в базе их ~250, запас на вырост.
const COUNTRY_CODES_LIMIT: u32 = 1000;

/// Станции в стране по данным Radio Browser (карта плотности, ADR-0035).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CountryCount {
    /// ISO 3166-1 alpha-2 в нижнем регистре.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub stationcount: u32,
}

/// Ошибки клиента Radio Browser.
#[derive(Debug, thiserror::Error)]
pub enum RadioError {
    #[error("сеть: {0}")]
    Network(String),
    #[error("нет доступных серверов")]
    NoServers,
    #[error("парсинг ответа: {0}")]
    Parse(String),
    #[error("HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("станция не найдена")]
    NotFound,
    #[error("некорректный UUID станции")]
    InvalidUuid,
}

/// Клиент Radio Browser API.
///
/// DNS-discovery выполняется лениво — перед первым запросом, как рекомендует
/// официальная документация. Клиент дешёво клонируется и может использоваться
/// из нескольких задач.
#[derive(Clone)]
pub struct RadioBrowserClient {
    inner: Arc<Inner>,
}

struct Inner {
    clients: RwLock<Vec<Client>>,
    refresh: AsyncMutex<()>,
    cache: Mutex<HashMap<String, Station>>,
    next_server: AtomicUsize,
}

impl RadioBrowserClient {
    /// Создаёт клиент без сетевых обращений.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                clients: RwLock::new(Vec::new()),
                refresh: AsyncMutex::new(()),
                cache: Mutex::new(HashMap::new()),
                next_server: AtomicUsize::new(0),
            }),
        }
    }

    /// Поиск станций.
    pub async fn search(&self, params: SearchParams) -> Result<Vec<Station>, RadioError> {
        let query = search_query(&params);
        let stations: Vec<Station> = self.get_json("/json/stations/search", &query).await?;
        let stations: Vec<Station> = stations
            .into_iter()
            .filter(|station| !station.url_resolved.trim().is_empty())
            .collect();
        self.remember(&stations);
        Ok(stations)
    }

    /// Счётчики станций по странам: вся база одним маленьким запросом
    /// (ADR-0035). Поле `name` этого endpoint'а содержит ISO-код, а не
    /// название страны — особенность формата Radio Browser.
    pub async fn country_counts(&self) -> Result<Vec<CountryCount>, RadioError> {
        let query = vec![("limit", COUNTRY_CODES_LIMIT.to_string())];
        self.get_json("/json/countrycodes", &query).await
    }

    /// Топ станций по числу прослушиваний — снимок мировой сцены для карты
    /// стран, когда в панели нет своей выборки.
    pub async fn top_stations(&self, limit: u32) -> Result<Vec<Station>, RadioError> {
        let query = vec![
            ("hidebroken", "true".to_owned()),
            ("order", "clickcount".to_owned()),
            ("reverse", "true".to_owned()),
            ("limit", limit.to_string()),
        ];
        let stations: Vec<Station> = self.get_json("/json/stations/search", &query).await?;
        let stations: Vec<Station> = stations
            .into_iter()
            .filter(|station| !station.url_resolved.trim().is_empty())
            .collect();
        self.remember(&stations);
        Ok(stations)
    }

    /// Случайная станция с известными координатами для Roulette.
    ///
    /// Серверный `order=random` не годится как единственный источник
    /// случайности: Radio Browser кэширует ответ по URL, и при `limit=1` каждый
    /// цикл возвращалась одна и та же станция. Поэтому берётся пачка с разным
    /// `offset` (кэш промахивается), а выбор из неё и исключение текущей и
    /// недавних станций делает клиент.
    pub async fn random_station(&self, exclude: &[String]) -> Result<Station, RadioError> {
        for attempt in 0..RANDOM_ATTEMPTS {
            let query = random_query(random_below(ROULETTE_OFFSET_SPAN));
            let stations: Vec<Station> = self.get_json("/json/stations/search", &query).await?;
            let candidates: Vec<Station> = stations
                .into_iter()
                .filter(is_roulette_candidate)
                .filter(|station| !exclude.contains(&station.stationuuid))
                .collect();
            if !candidates.is_empty() {
                let index = random_below(candidates.len() as u32) as usize;
                let station = candidates.into_iter().nth(index).expect("индекс из len");
                self.remember(std::slice::from_ref(&station));
                return Ok(station);
            }
            tracing::debug!(
                attempt,
                "Roulette: в пачке нет станции с потоком и координатами, повтор"
            );
        }
        Err(RadioError::NotFound)
    }

    /// Возвращает станцию по UUID (сначала из кэша).
    pub async fn get_station(&self, station_uuid: &str) -> Result<Station, RadioError> {
        validate_uuid(station_uuid)?;
        if let Some(station) = self.cached(station_uuid) {
            return Ok(station);
        }

        let path = format!("/json/stations/byuuid/{station_uuid}");
        let stations: Vec<Station> = self.get_json(&path, &[]).await?;
        let Some(station) = stations.into_iter().next() else {
            return Err(RadioError::NotFound);
        };
        self.remember(std::slice::from_ref(&station));
        Ok(station)
    }

    /// Отмечает станцию как прослушанную (click-tracking).
    pub async fn send_click(&self, station_uuid: &str) -> Result<(), RadioError> {
        validate_uuid(station_uuid)?;
        let path = format!("/json/url/{station_uuid}");
        self.request(&path, &[]).await?;
        Ok(())
    }

    /// Принудительно повторяет DNS-discovery и пересоздаёт пул серверов.
    pub async fn refresh_servers(&self) -> Result<(), RadioError> {
        self.refresh_servers_inner(true).await
    }

    async fn get_json<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, RadioError> {
        let response = self.request(path, query).await?;
        response
            .json()
            .await
            .map_err(|err| RadioError::Parse(err.to_string()))
    }

    async fn request(&self, path: &str, query: &[(&str, String)]) -> Result<Response, RadioError> {
        let url = format!("{API_BASE_URL}{path}");
        let start = self.inner.next_server.fetch_add(1, Ordering::Relaxed);
        let clients = rotated(self.ensure_clients().await?, start);

        match attempt_pool(&clients, &url, query).await {
            Ok(response) => return Ok(response),
            Err(AttemptError::Fatal(err)) => return Err(err),
            Err(AttemptError::Retry(err)) => {
                tracing::warn!(error = %err, "серверы Radio Browser недоступны, обновляем пул");
            }
        }

        self.refresh_servers_inner(true).await?;
        let clients = rotated(self.inner.clients.read().await.clone(), start);
        match attempt_pool(&clients, &url, query).await {
            Ok(response) => Ok(response),
            Err(AttemptError::Fatal(err) | AttemptError::Retry(err)) => Err(err),
        }
    }

    async fn ensure_clients(&self) -> Result<Vec<Client>, RadioError> {
        let clients = self.inner.clients.read().await.clone();
        if !clients.is_empty() {
            return Ok(clients);
        }

        self.refresh_servers_inner(false).await?;
        let clients = self.inner.clients.read().await.clone();
        if clients.is_empty() {
            Err(RadioError::NoServers)
        } else {
            Ok(clients)
        }
    }

    async fn refresh_servers_inner(&self, force: bool) -> Result<(), RadioError> {
        let _guard = self.inner.refresh.lock().await;
        if !force && !self.inner.clients.read().await.is_empty() {
            return Ok(());
        }

        let servers = discover_servers().await?;
        let clients = servers
            .iter()
            .map(build_client)
            .collect::<Result<Vec<_>, _>>()?;

        tracing::debug!(
            servers = clients.len(),
            "пул серверов Radio Browser обновлён"
        );
        *self.inner.clients.write().await = clients;
        Ok(())
    }

    fn remember(&self, stations: &[Station]) {
        let mut cache = lock(&self.inner.cache);
        for station in stations {
            if cache.len() >= STATION_CACHE_CAPACITY {
                cache.clear();
            }
            cache.insert(station.stationuuid.clone(), station.clone());
        }
    }

    fn cached(&self, station_uuid: &str) -> Option<Station> {
        lock(&self.inner.cache).get(station_uuid).cloned()
    }
}

impl Default for RadioBrowserClient {
    fn default() -> Self {
        Self::new()
    }
}

fn build_client(addr: &SocketAddr) -> Result<Client, RadioError> {
    Client::builder()
        .user_agent(USER_AGENT)
        .timeout(REQUEST_TIMEOUT)
        .resolve(API_HOST, *addr)
        .build()
        .map_err(|err| RadioError::Network(format!("не удалось создать HTTP-клиент: {err}")))
}

fn rotated(mut clients: Vec<Client>, start: usize) -> Vec<Client> {
    let len = clients.len();
    if len > 0 {
        clients.rotate_left(start % len);
    }
    clients
}

async fn attempt_pool(
    clients: &[Client],
    url: &str,
    query: &[(&str, String)],
) -> Result<Response, AttemptError> {
    let mut last_error = None;
    for client in clients {
        match attempt(client, url, query).await {
            Ok(response) => return Ok(response),
            Err(AttemptError::Fatal(err)) => return Err(AttemptError::Fatal(err)),
            Err(AttemptError::Retry(err)) => last_error = Some(err),
        }
    }

    Err(AttemptError::Retry(
        last_error.unwrap_or(RadioError::NoServers),
    ))
}

async fn attempt(
    client: &Client,
    url: &str,
    query: &[(&str, String)],
) -> Result<Response, AttemptError> {
    let response = client
        .get(url)
        .query(query)
        .send()
        .await
        .map_err(|err| AttemptError::Retry(RadioError::Network(err.to_string())))?;

    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }

    let error = http_error(status);
    if status.is_server_error()
        || status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::REQUEST_TIMEOUT
    {
        Err(AttemptError::Retry(error))
    } else {
        Err(AttemptError::Fatal(error))
    }
}

fn http_error(status: StatusCode) -> RadioError {
    RadioError::Http {
        status: status.as_u16(),
        message: status.canonical_reason().unwrap_or("").to_owned(),
    }
}

fn search_query(params: &SearchParams) -> Vec<(&'static str, String)> {
    let mut query = vec![
        ("hidebroken", params.hide_broken.to_string()),
        ("order", "votes".to_owned()),
        ("reverse", "true".to_owned()),
    ];

    if let Some(name) = trimmed(&params.name) {
        query.push(("name", name));
    }
    if let Some(country_code) = trimmed(&params.country_code) {
        query.push(("countrycode", country_code));
    }
    if let Some(tag) = trimmed(&params.tag) {
        query.push(("tag", tag));
    }
    if let Some(language) = trimmed(&params.language) {
        query.push(("language", language));
    }
    if params.limit > 0 {
        query.push(("limit", params.limit.to_string()));
    }
    if params.offset > 0 {
        query.push(("offset", params.offset.to_string()));
    }
    if params.has_geo_info {
        query.push(("has_geo_info", "true".to_owned()));
    }

    query
}

fn random_query(offset: u32) -> Vec<(&'static str, String)> {
    vec![
        ("hidebroken", "true".to_owned()),
        ("order", "random".to_owned()),
        ("limit", ROULETTE_BATCH.to_string()),
        ("offset", offset.to_string()),
        ("has_geo_info", "true".to_owned()),
    ]
}

/// Число из `[0, bound)` без внешних crate'ов: splitmix64 поверх наносекунд и
/// счётчика, чтобы два вызова в одну наносекунду не совпали.
pub(crate) fn random_below(bound: u32) -> u32 {
    if bound == 0 {
        return 0;
    }
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let ticks = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos() as u64);
    let mut x = ticks
        ^ (COUNTER.fetch_add(1, Ordering::Relaxed) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x % u64::from(bound)) as u32
}

fn is_roulette_candidate(station: &Station) -> bool {
    if station.url_resolved.trim().is_empty() {
        return false;
    }
    let (Some(lat), Some(lon)) = (station.geo_lat, station.geo_long) else {
        return false;
    };
    lat.is_finite()
        && lon.is_finite()
        && (-90.0..=90.0).contains(&lat)
        && (-180.0..=180.0).contains(&lon)
        && (lat != 0.0 || lon != 0.0)
}

fn trimmed(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn validate_uuid(uuid: &str) -> Result<(), RadioError> {
    let bytes = uuid.as_bytes();
    if bytes.len() != 36 {
        return Err(RadioError::InvalidUuid);
    }

    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 8 | 13 | 18 | 23) {
            if *byte != b'-' {
                return Err(RadioError::InvalidUuid);
            }
        } else if !byte.is_ascii_hexdigit() {
            return Err(RadioError::InvalidUuid);
        }
    }

    Ok(())
}

fn empty_string_as_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    Ok(value.filter(|value| !value.trim().is_empty()))
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

enum AttemptError {
    Retry(RadioError),
    Fatal(RadioError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_station(uuid: &str) -> Station {
        Station {
            stationuuid: uuid.to_owned(),
            name: "Test Radio".to_owned(),
            url: "http://example.com/live".to_owned(),
            url_resolved: "https://example.com/live".to_owned(),
            homepage: None,
            favicon: None,
            tags: "jazz".to_owned(),
            countrycode: "US".to_owned(),
            language: "english".to_owned(),
            votes: 120,
            country: "United States".to_owned(),
            state: "New York".to_owned(),
            bitrate: 128,
            codec: "MP3".to_owned(),
            geo_lat: None,
            geo_long: None,
            source: StationSource::Radio,
        }
    }

    #[test]
    fn station_treats_empty_optional_fields_as_none() {
        let json = r#"[{
            "stationuuid": "c685c086-b716-4edd-b313-d4a7baf1084c",
            "name": "Test Radio",
            "url": "http://example.com/live",
            "url_resolved": "https://example.com/live",
            "homepage": "",
            "favicon": "",
            "tags": "jazz",
            "countrycode": "US",
            "language": "english",
            "bitrate": 128,
            "codec": "MP3",
            "geo_lat": null,
            "geo_long": null
        }]"#;

        let stations: Vec<Station> = serde_json::from_str(json).expect("фикстура должна парситься");
        assert_eq!(stations.len(), 1);
        assert_eq!(stations[0].homepage, None);
        assert_eq!(stations[0].favicon, None);
        assert_eq!(stations[0].countrycode, "US");
        assert_eq!(stations[0].geo_lat, None);
        assert_eq!(stations[0].geo_long, None);
    }

    #[test]
    fn country_count_reads_iso_code_and_total() {
        let rows: Vec<CountryCount> = serde_json::from_str(
            r#"[{"name":"de","stationcount":4249},{"name":"tv","stationcount":1}]"#,
        )
        .expect("фикстура countrycodes");

        assert_eq!(rows[0].name, "de");
        assert_eq!(rows[0].stationcount, 4249);
    }

    #[test]
    fn station_parses_geo_coordinates() {
        let json = r#"[{
            "stationuuid": "c685c086-b716-4edd-b313-d4a7baf1084c",
            "name": "Test Radio",
            "url": "http://example.com/live",
            "url_resolved": "https://example.com/live",
            "geo_lat": 52.52,
            "geo_long": 13.405
        }]"#;

        let stations: Vec<Station> = serde_json::from_str(json).expect("фикстура должна парситься");
        assert_eq!(stations[0].geo_lat, Some(52.52));
        assert_eq!(stations[0].geo_long, Some(13.405));
    }

    #[test]
    fn station_defaults_missing_library_fields() {
        let json = r#"{
            "stationuuid": "c685c086-b716-4edd-b313-d4a7baf1084c",
            "name": "Legacy Radio"
        }"#;

        let station: Station = serde_json::from_str(json).expect("старый JSON должен читаться");
        assert_eq!(station.votes, 0);
        assert_eq!(station.country, "");
        assert_eq!(station.state, "");
    }

    #[test]
    fn station_parses_popularity_and_location_names() {
        let json = r#"{
            "stationuuid": "c685c086-b716-4edd-b313-d4a7baf1084c",
            "name": "Berlin Jazz",
            "votes": 421,
            "country": "Germany",
            "state": "Berlin",
            "countrycode": "DE"
        }"#;

        let station: Station = serde_json::from_str(json).expect("ответ API должен читаться");
        assert_eq!(station.votes, 421);
        assert_eq!(station.country, "Germany");
        assert_eq!(station.state, "Berlin");
    }

    #[test]
    fn builds_search_query_without_empty_filters() {
        let params = SearchParams {
            name: Some("  jazz  ".to_owned()),
            country_code: Some("US".to_owned()),
            tag: None,
            language: Some("   ".to_owned()),
            limit: 25,
            offset: 10,
            hide_broken: true,
            has_geo_info: true,
        };

        let query = search_query(&params);

        assert!(query.contains(&("name", "jazz".to_owned())));
        assert!(query.contains(&("countrycode", "US".to_owned())));
        assert!(query.contains(&("limit", "25".to_owned())));
        assert!(query.contains(&("offset", "10".to_owned())));
        assert!(query.contains(&("hidebroken", "true".to_owned())));
        assert!(query.contains(&("order", "votes".to_owned())));
        assert!(query.contains(&("has_geo_info", "true".to_owned())));
        assert!(!query.iter().any(|(key, _)| *key == "tag"));
        assert!(!query.iter().any(|(key, _)| *key == "language"));
    }

    #[test]
    fn random_query_takes_a_batch_with_an_offset() {
        let query = random_query(137);

        assert!(query.contains(&("order", "random".to_owned())));
        assert!(query.contains(&("limit", ROULETTE_BATCH.to_string())));
        assert!(query.contains(&("offset", "137".to_owned())));
        assert!(query.contains(&("has_geo_info", "true".to_owned())));
        assert!(query.contains(&("hidebroken", "true".to_owned())));
        assert!(!query.iter().any(|(key, _)| *key == "reverse"));
    }

    #[test]
    fn random_below_stays_inside_the_bound_and_varies() {
        for _ in 0..64 {
            assert!(random_below(7) < 7);
        }
        assert_eq!(random_below(0), 0);

        let mut seen = std::collections::HashSet::new();
        for _ in 0..64 {
            seen.insert(random_below(ROULETTE_OFFSET_SPAN));
        }
        assert!(seen.len() > 1, "последовательность вырождена в константу");
    }

    #[test]
    fn roulette_candidate_requires_stream_and_valid_coordinates() {
        let mut station = sample_station("c685c086-b716-4edd-b313-d4a7baf1084c");
        station.geo_lat = Some(52.52);
        station.geo_long = Some(13.405);
        assert!(is_roulette_candidate(&station));

        station.url_resolved.clear();
        assert!(!is_roulette_candidate(&station));

        let mut station = sample_station("c685c086-b716-4edd-b313-d4a7baf1084c");
        assert!(!is_roulette_candidate(&station), "нет координат");

        station.geo_lat = Some(0.0);
        station.geo_long = Some(0.0);
        assert!(!is_roulette_candidate(&station), "нулевые координаты");

        station.geo_lat = Some(95.0);
        station.geo_long = Some(13.405);
        assert!(!is_roulette_candidate(&station), "широта вне диапазона");

        station.geo_lat = Some(f64::NAN);
        station.geo_long = Some(13.405);
        assert!(!is_roulette_candidate(&station), "нечисловая широта");
    }

    #[test]
    fn omits_geo_filter_by_default() {
        let query = search_query(&SearchParams::default());

        assert!(!query.iter().any(|(key, _)| *key == "has_geo_info"));
    }

    #[test]
    fn validates_station_uuid() {
        assert!(validate_uuid("c685c086-b716-4edd-b313-d4a7baf1084c").is_ok());
        for uuid in [
            "",
            "../../etc/passwd",
            "c685c086b7164eddb313d4a7baf1084c",
            "c685c086-b716-4edd-b313-d4a7baf1084z",
        ] {
            assert!(matches!(validate_uuid(uuid), Err(RadioError::InvalidUuid)));
        }
    }

    #[test]
    fn caches_stations_by_uuid() {
        let client = RadioBrowserClient::new();
        let station = sample_station("c685c086-b716-4edd-b313-d4a7baf1084c");

        client.remember(std::slice::from_ref(&station));

        assert_eq!(client.cached(&station.stationuuid), Some(station.clone()));
    }

    #[tokio::test]
    #[ignore = "требует сеть; запускайте с --ignored"]
    async fn searches_and_fetches_live_station() {
        let client = RadioBrowserClient::new();
        let params = SearchParams {
            name: Some("SomaFM".to_owned()),
            limit: 5,
            hide_broken: true,
            ..SearchParams::default()
        };

        let stations = client.search(params).await.expect("поиск не удался");
        assert!(!stations.is_empty(), "поиск не вернул станций");

        let station = client
            .get_station(&stations[0].stationuuid)
            .await
            .expect("не удалось получить станцию по UUID");
        assert_eq!(station.stationuuid, stations[0].stationuuid);

        let random = client
            .random_station(&[])
            .await
            .expect("Roulette не вернула станцию");
        assert!(!random.url_resolved.trim().is_empty());
        assert!(random.geo_lat.is_some() && random.geo_long.is_some());
    }
}
