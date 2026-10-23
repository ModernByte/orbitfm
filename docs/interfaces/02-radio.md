# Интерфейс: Radio Browser Client

```rust
use serde::{Deserialize, Serialize};

/// Параметры поиска
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchParams {
    pub name: Option<String>,
    pub country_code: Option<String>,   // ISO 3166-1 alpha-2
    pub tag: Option<String>,
    pub language: Option<String>,
    pub limit: u32,
    pub offset: u32,
    pub hide_broken: bool,
    /// Только станции с известными координатами (`has_geo_info=true`).
    pub has_geo_info: bool,
}

/// Станция (минимальный набор полей)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Station {
    pub stationuuid: String,
    pub name: String,
    pub url: String,
    pub url_resolved: String,           // использовать для воспроизведения
    pub homepage: Option<String>,
    pub favicon: Option<String>,
    pub tags: String,
    pub countrycode: String,
    pub language: String,
    /// Число голосов каталога; отсутствие в JSON → 0.
    #[serde(default)]
    pub votes: u32,
    /// Название страны; отсутствие в JSON → пустая строка.
    #[serde(default)]
    pub country: String,
    /// Штат/регион; отсутствие в JSON → пустая строка.
    #[serde(default)]
    pub state: String,
    pub bitrate: u32,
    pub codec: String,
    /// Широта, если каталог её знает.
    pub geo_lat: Option<f64>,
    /// Долгота, если каталог её знает.
    pub geo_long: Option<f64>,
}

/// Ошибки
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

/// Контракт клиента
#[derive(Clone)]
pub struct RadioBrowserClient { /* Arc<Inner> */ }

impl RadioBrowserClient {
    /// Создаёт клиент без сетевых обращений; discovery выполняется лениво.
    pub fn new() -> Self;

    /// Поиск станций.
    pub async fn search(&self, params: SearchParams) -> Result<Vec<Station>, RadioError>;

    /// Случайная станция с координатами для Roulette (до трёх попыток).
    pub async fn random_station(&self) -> Result<Station, RadioError>;

    /// Получить станцию по UUID (сначала из кэша).
    pub async fn get_station(&self, station_uuid: &str) -> Result<Station, RadioError>;

    /// Отправить click (при старте воспроизведения).
    pub async fn send_click(&self, station_uuid: &str) -> Result<(), RadioError>;

    /// Принудительно повторить DNS-discovery и пересоздать пул серверов.
    pub async fn refresh_servers(&self) -> Result<(), RadioError>;
}
```

## Семантика

- `new()` не выполняет сетевых запросов и не может упасть; DNS-discovery
  `all.api.radio-browser.info` выполняется перед первым запросом (лениво) либо заранее
  фоновым прогревом в `main`.
- `Station` сериализуется в `library.json`; поля `votes`, `country`, `state` имеют
  `serde(default)` (0 и пустые строки), поэтому снимки, записанные до Этапа 8,
  читаются без миграции.
- Клиент дешёво клонируется (`Arc` внутри) и используется из GUI и фоновых задач.
- `search` всегда передаёт `hidebroken`, `order=votes`, `reverse=true`; `limit` и `offset`
  добавляются только при значении больше нуля. Пустые строковые фильтры отбрасываются,
  значения обрезаются по краям. Станции без `url_resolved` в результат не попадают.
- При `has_geo_info = true` добавляется фильтр `has_geo_info=true`; клиент не проверяет
  координаты — это делает вызывающая сторона (`geo_lat`/`geo_long` могут быть `null`).
- `random_station` запрашивает `/json/stations/search` с
  `order=random&limit=1&has_geo_info=true&hidebroken=true` и не передаёт `reverse`.
  Станция принимается только с непустым `url_resolved` и валидными координатами
  (конечные, в диапазонах, не `(0, 0)`); иначе выполняется до трёх попыток, после
  чего возвращается `NotFound`. Найденная станция попадает в кэш.
- `get_station` проверяет формат UUID (`InvalidUuid`) и сначала ищет станцию в кэше;
  при промахе запрашивает `/json/stations/byuuid/<uuid>` и возвращает `NotFound` для
  пустого ответа.
- `send_click` проверяет UUID и выполняет GET `/json/url/<uuid>`.
- `refresh_servers` игнорирует существующий пул и заново выполняет discovery.
- Каждый HTTP-запрос содержит `User-Agent: OrbitFM/1.0 (ModernByte; Linux; Rust)`.

## Отказоустойчивость

- DNS-ответ дедуплицируется по IP; для каждого адреса создаётся отдельный
  `reqwest::Client` с `.resolve(API_HOST, addr)`, поэтому TLS-проверка и SNI остаются
  привязаны к имени `all.api.radio-browser.info`.
- Пул обходится со случайного смещения; таймаут запроса — 5 с, discovery — 3 с.
- Сетевые ошибки, таймауты, `5xx`, `429` и `408` считаются повторяемыми: клиент
  пробует следующий сервер, затем при необходимости обновляет пул и повторяет запрос
  один раз. Остальные `4xx` возвращаются сразу как `Http`.
- Кэш станций — in-memory, до 512 записей; при переполнении очищается.

## Примечание

Трейт `RadioClient` и `async_trait` не используются: подмена реализации в тестах не
требуется, контракт фиксируется сигнатурами конкретной структуры. Официальные правила
API — ADR-0004 и `docs/modules/02-radio-browser.md`.
