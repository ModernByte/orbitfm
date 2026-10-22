# Интерфейс: Geo Service

```rust
use std::path::Path;

/// Информация о стране
#[derive(Debug, Clone)]
pub struct Country {
    pub name: String,
    pub iso_a2: String,
    pub iso_a3: Option<String>,
}

/// Ошибки
#[derive(Debug, thiserror::Error)]
pub enum GeoError {
    #[error("не удалось загрузить GeoJSON: {0}")]
    Load(String),
    #[error("некорректные данные границ")]
    InvalidGeometry,
}

/// Контракт геосервиса
pub trait GeoService: Send + Sync {
    /// Загрузка из файла
    fn load(path: &Path) -> Result<Self, GeoError>
    where
        Self: Sized;

    /// Загрузка из встроенных данных
    fn from_embedded() -> Result<Self, GeoError>
    where
        Self: Sized;

    /// Страна по географическим координатам (lat, lon)
    fn country_at(&self, lat: f64, lon: f64) -> Option<&Country>;

    /// Страна по точке на единичной сфере (x, y, z)
    fn country_at_sphere_point(&self, x: f64, y: f64, z: f64) -> Option<&Country>;
}
```
