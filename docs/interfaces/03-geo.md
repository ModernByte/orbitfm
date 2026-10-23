# Интерфейс: Geo Service

Реализация — конкретная структура `GeoService` (`src/geo/`), публичный трейт не
используется. Контракт фиксируется сигнатурами методов и структурой `Country`.

```rust
use std::path::Path;

/// Информация о стране.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Country {
    /// Название страны (ADMIN из Natural Earth).
    pub name: String,
    /// ISO 3166-1 alpha-2.
    pub iso_a2: String,
    /// ISO 3166-1 alpha-3, если известен.
    pub iso_a3: Option<String>,
}

/// Кольца границ страны в градусах `[lon, lat]`.
///
/// Содержит внешние контуры и отверстия; пригодна для проекции в слое render.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CountryShape {
    pub rings: Vec<Vec<[f64; 2]>>,
}

impl CountryShape {
    /// Центр крупнейшего кольца `(lat, lon)` для мягкого доворота камеры.
    /// Долготы раскрываются относительно первой точки, поэтому страны на
    /// антимеридиане получают центр рядом с собой, а не в Африке.
    pub fn focus_point(&self) -> Option<(f64, f64)>;
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

    /// Поиск стран по названию или ISO-коду без учёта регистра.
    /// Пустой запрос или `limit == 0` → пустой список; результаты
    /// отсортированы по названию и обрезаны до `limit`.
    pub fn search_countries(&self, query: &str, limit: usize) -> Vec<&Country>;

    /// Кольца границ страны по ISO 3166-1 alpha-2 без учёта регистра.
    pub fn country_shape(&self, iso_a2: &str) -> Option<&CountryShape>;
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
- `search_countries` нормализует запрос (`trim`, нижний регистр) и сопоставляет его
  с названием, `iso_a2` и `iso_a3`; пустые и пробельные запросы ничего не возвращают.
- `country_shape` возвращает кольца без повторяющейся замыкающей точки и без
  дубликатов; кольца короче трёх точек отбрасываются при загрузке. `None` — для
  неизвестного ISO-кода.
- `CountryShape::focus_point` берёт крупнейшее по числу точек кольцо и возвращает
  среднее широт и долгот; долготы соседних точек раскрываются относительно первой,
  затем результат нормализуется в `[-180, 180]`. Пустая форма → `None`.

## Данные

- Встроенный датасет: `assets/geojson/world-110m.json` через `include_str!`.
- Источник: Natural Earth 110m Admin 0 – Countries (public domain).
- `ISO_A2` берётся с fallback на `ISO_A2_EH`; значения `-99` и пустые строки
  отбрасываются. `ISO_A3` может отсутствовать.
