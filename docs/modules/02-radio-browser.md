# Модуль: Radio Browser Client

**Путь в коде:** `src/services/` (`radio_browser.rs`, `discovery.rs`, `artwork.rs`, `ssrf.rs`)  
**Ответственность:** Поиск живых серверов API, запросы станций, кэширование, click-tracking и безопасная загрузка артворка.  
**Зависимости:** reqwest (rustls), tokio, serde

---

## 1. Назначение

Единственная точка взаимодействия с Radio Browser API.  
Гарантирует соблюдение официальных правил использования API и не блокирует запуск
приложения сетевыми запросами.

## 2. Состав модуля

| Файл | Назначение |
|------|------------|
| `mod.rs` | Публичный фасад (`RadioBrowserClient`, `RadioError`, `SearchParams`, `Station`, `ArtworkFetcher`, `publishable`, `public_verdict`) |
| `discovery.rs` | DNS-discovery `all.api.radio-browser.info`, дедупликация и ротация |
| `radio_browser.rs` | HTTP-клиент, пул зеркал, failover, кэш, click-tracking |
| `artwork.rs` | Хопп-цикл артворка: резолв, припинованные клиенты, лимиты, MIME, `tracing` причин отказа; opt-in HTTP и `public_verdict` для гейта публикации |
| `ssrf.rs` | Ярус 1 (`classify`, `parse_publishable`) и классификатор адреса `is_public_destination` |

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
`has_geo_info=true` добавляется только при `SearchParams::has_geo_info`. Сессия
больше не выставляет его для поиска: список страны полный и пагинируется, а
маркеры — подмножество строк с координатами (ADR-0036, ред. 2026-10-28).
Валидацию координат и группировку маркеров выполняет Application Core (ADR-0012).

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
  требует сеть);
- таблица классификатора адресов (границы диапазонов IPv4/IPv6, v4-mapped и 6to4
  рекурсивно на один уровень);
- хопп-цикл на рукописном loopback-макете (`tokio::net::TcpListener`, без
  `hyper`/`wiremock`): успешный fetch с ограниченным телом, перепроверка каждого
  редирект-хоппа, обрыв на превышении бюджета редиректов, редирект без
  `Location`, не-2xx, отсутствие `Content-Type` + валидное тело, обрезка тела
  за заявленным `Content-Length`, отказ понижения https→http и отклонение
  приватного literal'а **до** всякого соединения;
- all-or-nothing по набору адресов резолва, потолок пула припинованных клиентов
  и узость тестового шва (шов ослабляет ровно предикат адреса);
- ярус 1 и `publishable`: числовые трюки (`0x7f.0.0.1`, `127.1`, `0177.0.0.1`,
  `2130706433`), punycode-канонизация, `router`/`*.local`/`*.internal`,
  userinfo, порт 0 (и разрешение не-дефолтных портов), чужие схемы;
- opt-in HTTP: `Policy::allow_http` переключается на лету (`AtomicBool`) и
  действует на уже существующие клоны фетчера;
- `public_verdict`: приватный literal отклоняется без сети, синтаксически
  отвергнутый URL не порождает резолва.

## 10. Артворк: политика цели в два яруса

Изображения станций — единственный исходящий HTTP-путь, которым управляет не
пользовательский ввод, а чужой каталог, поэтому он вынесен в `artwork.rs` +
`ssrf.rs` и подчиняется ADR-0025 (ред. 3). Кратко, без пересказа ADR:

- **ярус 1** — синтаксический, без DNS: `ssrf::classify` (схема, userinfo, порт
  0, типизированный хост, ярлыковые правила) и `ssrf::parse_publishable` для
  публикаций наружу. Синхронный вызов обязателен: `Session::sync_media`
  исполняется на главном потоке и внутри `runtime.block_on` в `park()`.
- **ярус 2** — разрешённый, асинхронный: `ssrf::is_public_destination` по всем
  адресам резолва (all-or-nothing), припинованный клиент и `public_verdict`
  (тот же резолв + классификация без запроса). Зовут `ArtworkFetcher::fetch` —
  на каждом хоппе заново, и `session::publish_gate` — фоном для публикуемых
  метаданных.
- хопп-цикл ручной (`redirect::Policy::none()`), `MAX_ARTWORK_REDIRECTS = 3`,
  общий wall-clock `TOTAL_TIMEOUT = 6 c`, на запрос 6 c, на DNS 3 c.
- клиенты кэшируются по `host:port` с потолком 16 и TTL 60 c, LRU-вытеснение;
  по истечении TTL хост резолвится и валидируется заново.
- MIME 2xx — только `image/png`, `image/jpeg`, `image/webp` и
  `application/octet-stream` (ADR-0025, п. 4); `text/html`, `image/svg+xml` и
  `image/gif` отклоняются. Тело — не более 512 КиБ.
- `http` как конечная цель отвергается по умолчанию; явный opt-in
  `artwork_allow_http` (тумблер «Разрешить HTTP-обложки», ADR-0025 ред. 3)
  переключает `Policy::allow_http` в `AtomicBool` на лету — общий для всех
  клонов фетчера, без перезапуска. Понижение `https → http` в редиректе
  запрещено при любом значении.
- публикация `artUrl`/`xesam:url` наружу (MPRIS, Now Playing, SMTC) — гейт
  `session::publish_gate` (ADR-0025 ред. 3, п. 12): ярус 1 синхронно, затем
  вердикт яруса 2 из bounded-кэша (64 записи, TTL 60 с, SWR); нет вердикта —
  поле публикуется пустым, резолв идёт фоном и пробуждает GUI.

## 11. Связанные ADR

- ADR-0004: Выбор клиента Radio Browser
- ADR-0012: Маркеры станций на глобусе (гео-фильтр и поля координат)
- ADR-0036: один запрос на поиск; список полный, маркеры — подмножество строк
  с координатами (ред. 2026-10-28)
- ADR-0016 (ред. 2) и ADR-0025 (ред. 3): загрузчик и сетевая политика артворка
