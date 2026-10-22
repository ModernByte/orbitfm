# Интерфейс: Geo Service

Реализация — конкретная структура `GeoService` (`src/geo/`), публичный трейт не
используется. Контракт фиксируется сигнатурами методов и структурой `Country`.

```rust
use std::path::Path;

/// Информация о стране.
#[derive(Debug, Clone)]
pub struct Country {
    /// Название страны (ADMIN из Natural Earth).
    pub name: String,
    /// ISO 3166-1 alpha-2.
    pub iso_a2: String,
    /// ISO 3166-1 alpha-3, если известен.
    pub iso_a3: Option<String>,
}

/// Ошибки геосервиса.
#[derive(Debug, thiserror::Error)]
pub enum GeoError {
    #[error("не удалось загрузить GeoJSON: {0}")]
    Load(String),
    #[error("некорректные данные границ")]
    InvalidGeometry,
}

/// Геосервис с предварительно построенным R-Tree.
pub struct GeoService { /* ... */ }

impl GeoService {
    /// Загрузка из произвольного GeoJSON-файла (альтернативные датасеты).
    pub fn load(path: &Path) -> Result<Self, GeoError>;

    /// Загрузка встроенного Natural Earth 110m.
    pub fn from_embedded() -> Result<Self, GeoError>;

    /// Страна по географическим координатам в градусах.
    pub fn country_at(&self, lat: f64, lon: f64) -> Option<&Country>;

    /// Страна по точке на сфере (x, y, z); точка не обязана быть единичной.
    pub fn country_at_sphere_point(&self, x: f64, y: f64, z: f64) -> Option<&Country>;
}
```

## Семантика

- `country_at` принимает `lat ∈ [-90, 90]`, `lon ∈ [-180, 180]`; выход за диапазон
  даёт `None`.
- Сферические координаты: `x = cos φ · sin λ`, `y = sin φ`, `z = cos φ · cos λ`,
  где `φ` — широта, `λ` — долгота. Направление `+Z` соответствует Гринвичу.
- `country_at_sphere_point` нормализует вектор по длине; нулевой или нечисловой
  вектор даёт `None`.
- Океан, спорные территории без валидного ISO-кода и точки вне полигонов дают `None`.
- `load` читает файл и возвращает `GeoError::Load` при ошибке I/O или разбора;
  `InvalidGeometry` — если после фильтрации не осталось пригодных полигонов.

## Данные

- Встроенный датасет: `assets/geojson/world-110m.json` через `include_str!`.
- Источник: Natural Earth 110m Admin 0 – Countries (public domain).
- `ISO_A2` берётся с fallback на `ISO_A2_EH`; значения `-99` и пустые строки
  отбрасываются. `ISO_A3` может отсутствовать.
