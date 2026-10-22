# Модуль: Radio Browser Client

**Путь в коде:** `src/services/radio_browser.rs` (и `discovery.rs`)  
**Ответственность:** Поиск живых серверов API, запросы станций, кэширование, click-tracking.  
**Зависимости:** reqwest, tokio, serde

---

## 1. Назначение

Единственная точка взаимодействия с Radio Browser API.  
Гарантирует соблюдение официальных правил использования API.

## 2. Публичный интерфейс

```rust
pub struct RadioBrowserClient { /* ... */ }

impl RadioBrowserClient {
    /// Создаёт клиент и выполняет начальный DNS-discovery
    pub async fn new() -> Result<Self, RadioError>;

    /// Поиск станций
    pub async fn search(&self, params: SearchParams) -> Result<Vec<Station>, RadioError>;

    /// Получить станцию по UUID
    pub async fn get_station(&self, uuid: &str) -> Result<Station, RadioError>;

    /// Отправить click (обязательно при старте воспроизведения)
    pub async fn send_click(&self, station_uuid: &str) -> Result<(), RadioError>;

    /// Принудительно обновить список серверов
    pub async fn refresh_servers(&mut self) -> Result<(), RadioError>;
}

#[derive(Debug, Clone)]
pub struct SearchParams {
    pub name: Option<String>,
    pub country_code: Option<String>,      // ISO 3166-1 alpha-2
    pub tag: Option<String>,
    pub language: Option<String>,
    pub limit: u32,
    pub offset: u32,
    pub hide_broken: bool,
    // ... другие фильтры по необходимости
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub bitrate: u32,
    pub codec: String,
    // ... остальные поля по необходимости
}
```

## 3. Обязательные правила (api.radio-browser.info)

1. DNS-lookup `all.api.radio-browser.info` → список серверов
2. Рандомизация + failover при ошибках 5xx / таймауте
3. Каждый запрос содержит заголовок:
   ```
   User-Agent: OrbitFM/1.0 (ModernByte; Linux; Rust)
   ```
4. Для воспроизведения всегда использовать `url_resolved`
5. При старте воспроизведения вызывать click-endpoint

## 4. Политика отказоустойчивости

- Таймаут запроса: 5–8 секунд
- При ошибке — следующий сервер из пула
- После N неудач подряд — повторный discovery
- Локальный кэш последних успешных ответов (опционально, in-memory + disk)

## 5. Ошибки

`RadioError` — сеть, парсинг, пустой пул серверов, HTTP-ошибки.

## 6. Связанные ADR

- ADR-0004: Выбор клиента Radio Browser
