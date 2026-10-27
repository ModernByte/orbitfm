# Модуль: Radio Browser Client

**Путь в коде:** `src/services/` (`radio_browser.rs`, `discovery.rs`)  
**Ответственность:** Поиск живых серверов API, запросы станций, кэширование, click-tracking.  
**Зависимости:** reqwest (rustls), tokio, serde

---

## 1. Назначение

Единственная точка взаимодействия с Radio Browser API.  
Гарантирует соблюдение официальных правил использования API и не блокирует запуск
приложения сетевыми запросами.

## 2. Состав модуля

| Файл | Назначение |
|------|------------|
| `mod.rs` | Публичный фасад (`RadioBrowserClient`, `RadioError`, `SearchParams`, `Station`) |
| `discovery.rs` | DNS-discovery `all.api.radio-browser.info`, дедупликация и ротация |
| `radio_browser.rs` | HTTP-клиент, пул зеркал, failover, кэш, click-tracking |

## 3. Публичный интерфейс

```rust
#[derive(Clone)]
pub struct RadioBrowserClient { /* Arc<Inner> */ }

impl RadioBrowserClient {
    /// Создаёт клиент без сетевых обращений.
    pub fn new() -> Self;

    pub async fn search(&self, params: SearchParams) -> Result<Vec<Station>, RadioError>;

    /// Случайная станция с координатами для Roulette (до трёх попыток).
    /// `exclude` — UUID, которые нельзя выдавать (текущий эфир и недавняя
    /// история): сервер кэширует ответ по URL, поэтому случайность
    /// обеспечивает клиент.
    pub async fn random_station(&self, exclude: &[String]) -> Result<Station, RadioError>;

    pub async fn get_station(&self, station_uuid: &str) -> Result<Station, RadioError>;

    /// Обязательно при старте воспроизведения.
    pub async fn send_click(&self, station_uuid: &str) -> Result<(), RadioError>;

    pub async fn refresh_servers(&self) -> Result<(), RadioError>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchParams {
    pub name: Option<String>,
    pub country_code: Option<String>,      // ISO 3166-1 alpha-2
    pub tag: Option<String>,
    pub language: Option<String>,
    pub limit: u32,
    pub offset: u32,
    pub hide_broken: bool,
    pub has_geo_info: bool,                // только станции с координатами
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Station {
    pub stationuuid: String,
    pub name: String,
    pub url: String,
    pub url_resolved: String,              // использовать для play
    pub homepage: Option<String>,
    pub favicon: Option<String>,
    pub tags: String,
    pub countrycode: String,
    pub language: String,
    pub votes: u32,                        // serde(default) → 0
    pub country: String,                   // serde(default) → ""
    pub state: String,                     // serde(default) → ""
    pub bitrate: u32,
    pub codec: String,
    pub geo_lat: Option<f64>,              // широта, если известна
    pub geo_long: Option<f64>,             // долгота, если известна
}
```

`votes`, `country` и `state` используются панелью страны для сортировки и карточек;
благодаря `serde(default)` снимки `library.json` прежних версий читаются без миграции.
Полная семантика — `docs/interfaces/02-radio.md`.

## 4. Discovery и пул серверов

1. `lookup_host(("all.api.radio-browser.info", 443))` с таймаутом 3 с.
2. IP-адреса дедуплицируются; пул ротируется случайным образом.
3. Для каждого IP создаётся `reqwest::Client` с:
   - `user_agent("OrbitFM/1.0 (ModernByte; Linux; Rust)")`;
   - `timeout(5 с)`;
   - `.resolve(API_HOST, addr)` — соединение идёт на конкретный IP, но TLS-хост и SNI
     остаются `all.api.radio-browser.info`.
4. Discovery ленивый: `new()` не делает сети. В `main` пул прогревается фоновой задачей
   `refresh_servers()`; ошибка прогрева логируется и не мешает запуску.
5. Запросы обходят пул со случайного смещения (`AtomicUsize`), что распределяет нагрузку
   между зеркалами.

## 5. Failover и повторы

- Повторяемые ошибки: сеть/таймаут, `5xx`, `429`, `408`.
- При повторяемой ошибке клиент пробует следующий сервер из пула.
- Если весь пул исчерпан — `refresh_servers` (принудительный discovery), затем один
  повторный проход по новому пулу.
- Прочие `4xx` возвращаются сразу как `RadioError::Http` без обновления пула.
- Параллельные обновления пула сериализуются `tokio::sync::Mutex`.

## 6. Эндпоинты

| Метод | Эндпоинт | Назначение |
|-------|----------|------------|
| GET | `/json/stations/search` | Поиск; `hidebroken`, `order=votes`, `reverse=true`, фильтры `name`, `countrycode`, `tag`, `language`, `has_geo_info`, `limit`, `offset` |
| GET | `/json/stations/search` (Roulette) | `order=random&limit=24&offset=<0..499>&has_geo_info=true&hidebroken=true`, без `reverse`; выбор станции и исключения — на клиенте, до трёх попыток |
| GET | `/json/stations/byuuid/<uuid>` | Станция по UUID |
| GET | `/json/url/<uuid>` | Click-tracking при старте воспроизведения |

Станции без `url_resolved` отбрасываются при поиске. Воспроизведение всегда использует
`url_resolved`. `geo_lat`/`geo_long` передаются как есть (`Option<f64>`); фильтр
`has_geo_info=true` включается только при `SearchParams::has_geo_info`. Валидацию
координат и группировку маркеров выполняет Application Core (ADR-0012).

`random_station` берёт пачку из 24 станций со случайным `offset` в `0..499`:
кэш Radio Browser отвечает по URL, и при `limit=1` каждый цикл возвращалась
одна и та же станция. Из пачки отбираются станции с непустым `url_resolved`,
конечными координатами в допустимых диапазонах и не `(0, 0)`, не входящие в
`exclude`; одна выбирается случайно (splitmix64 без внешнего crate'а). Если
подходящих нет, запрос повторяется с новым `offset` (до трёх раз), затем
возвращается `NotFound`. Случайность и исключения — клиентские: серверный
`order=random` остаётся только способом получить разнообразную пачку.

## 7. Кэш и валидация

- In-memory кэш `HashMap<String, Station>` до 512 записей; при переполнении очищается.
- `search` запоминает все найденные станции, `get_station` и `random_station` —
  полученную станцию.
- `get_station` и `send_click` требуют UUID формата `8-4-4-4-12` (hex); иначе
  `RadioError::InvalidUuid`. Это защищает URL от подстановки произвольного пути.

## 8. Ошибки

`RadioError` — `Network`, `NoServers`, `Parse`, `Http { status, message }`, `NotFound`,
`InvalidUuid`.

## 9. Тесты

- разбор станции с пустыми `homepage`/`favicon` → `None`;
- разбор `geo_lat`/`geo_long`, включая `null`;
- разбор `votes`/`country`/`state` и значения по умолчанию для старого JSON;
- построение query без пустых фильтров и с `has_geo_info=true`;
- query Roulette: `order=random`, `limit=24`, случайный `offset`, `has_geo_info`, без `reverse`;
- отбор станции Roulette по URL и координатам (включая `(0, 0)` и NaN);
- исключение текущей и недавних станций из пачки;
- `random_below` не выходит за границу и не вырождается в константу;
- валидация UUID (включая попытку path traversal);
- попадание в кэш по UUID;
- ротация пула сохраняет все серверы;
- живой тест `searches_and_fetches_live_station` (SomaFM + Roulette, `--ignored`,
  требует сеть).

## 10. Связанные ADR

- ADR-0004: Выбор клиента Radio Browser
- ADR-0012: Маркеры станций на глобусе (гео-фильтр и поля координат)
