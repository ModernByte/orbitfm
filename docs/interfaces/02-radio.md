# Интерфейс: Radio Browser Client

```rust
use serde::{Deserialize, Serialize};

/// Параметры поиска
#[derive(Debug, Clone, Default)]
pub struct SearchParams {
    pub name: Option<String>,
    pub country_code: Option<String>,   // ISO 3166-1 alpha-2
    pub tag: Option<String>,
    pub language: Option<String>,
    pub limit: u32,
    pub offset: u32,
    pub hide_broken: bool,
}

/// Станция (минимальный набор полей)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Station {
    pub stationuuid: String,
    pub name: String,
    pub url: String,
    pub url_resolved: String,
    pub homepage: Option<String>,
    pub favicon: Option<String>,
    pub tags: String,
    pub countrycode: String,
    pub language: String,
    pub bitrate: u32,
    pub codec: String,
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
}

/// Контракт клиента
#[async_trait::async_trait]
pub trait RadioClient: Send + Sync {
    /// Поиск станций
    async fn search(&self, params: SearchParams) -> Result<Vec<Station>, RadioError>;

    /// Получить станцию по UUID
    async fn get_station(&self, uuid: &str) -> Result<Station, RadioError>;

    /// Отправить click (при старте воспроизведения)
    async fn send_click(&self, station_uuid: &str) -> Result<(), RadioError>;

    /// Обновить список серверов (DNS discovery)
    async fn refresh_servers(&mut self) -> Result<(), RadioError>;
}
```
