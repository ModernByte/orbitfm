# Модуль: Geo / Spatial Raycasting

**Путь в коде:** `src/geo/`  
**Ответственность:** Загрузка границ стран, R-Tree индекс, преобразование координат, Point-in-Polygon, поиск стран и кольца для контуров.  
**Зависимости:** geo, geojson, rstar

---

## 1. Назначение

По клику или наведению на 3D-глобус определить страну, найти страну по названию
или ISO-коду и отдать её кольца для контура.  
Работает полностью offline.

## 2. Публичный интерфейс

```rust
pub struct GeoService { /* ... */ }

impl GeoService {
    /// Загружает GeoJSON из файла и строит R-Tree.
    pub fn load(path: &Path) -> Result<Self, GeoError>;

    /// Загружает встроенный Natural Earth 110m.
    pub fn from_embedded() -> Result<Self, GeoError>;

    /// Находит страну по географическим координатам (градусы).
    pub fn country_at(&self, lat: f64, lon: f64) -> Option<&Country>;

    /// Находит страну по точке на сфере (x, y, z).
    pub fn country_at_sphere_point(&self, x: f64, y: f64, z: f64) -> Option<&Country>;

    /// Ищет страны по названию или ISO-коду (без учёта регистра).
    pub fn search_countries(&self, query: &str, limit: usize) -> Vec<&Country>;

    /// Кольца границ страны по ISO 3166-1 alpha-2.
    pub fn country_shape(&self, iso_a2: &str) -> Option<&CountryShape>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Country {
    pub name: String,
    pub iso_a2: String,          // ISO 3166-1 alpha-2
    pub iso_a3: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CountryShape {
    pub rings: Vec<Vec<[f64; 2]>>,   // [lon, lat], градусы
}

impl CountryShape {
    /// Центр крупнейшего кольца (lat, lon) для доворота камеры.
    pub fn focus_point(&self) -> Option<(f64, f64)>;
}

#[derive(Debug, thiserror::Error)]
pub enum GeoError {
    #[error("не удалось загрузить GeoJSON: {0}")]
    Load(String),
    #[error("некорректные данные границ")]
    InvalidGeometry,
}
```

Геометрия (`MultiPolygon<f64>`) и R-Tree хранятся внутри `GeoService`; наружу
выходят только `Country` и кольца `CountryShape` без доменных типов рендера.
Дополнительно строится индекс `HashMap<iso_a2, index>` для `country_shape`.
Для приложения используется `from_embedded`; `load` остаётся контрактом модуля для
альтернативных датасетов.

## 3. Алгоритм

1. Рендер-модуль преобразует клик в точку на сфере (`GlobeCamera::screen_to_sphere`).
2. Точка `(x, y, z)` переводится в географические координаты:
   ```
   R = sqrt(x² + y² + z²)
   φ = arcsin(y / R)
   λ = atan2(x, z)
   ```
   Соглашение: `x = cos φ · sin λ`, `y = sin φ`, `z = cos φ · cos λ`; `+Z` — Гринвич.
3. Быстрый отбор кандидатов через R-Tree по bounding box `[lon, lat]`.
4. Точная проверка `geo::Contains<Point>` для `MultiPolygon`.
5. Полигоны антимеридиана уже разделены в датасете Natural Earth; отдельная
   нормализация при загрузке не выполняется. Это проверено тестом для Fiji.
6. При загрузке для каждой страны сохраняются кольца (`shape_from_geometry`):
   замыкающая точка и дубликаты удаляются, кольца короче трёх точек отбрасываются.
7. `search_countries` фильтрует страны по вхождению запроса в название либо по
   точному совпадению с `iso_a2`/`iso_a3`, сортирует по названию и обрезает до
   `limit`; пустой запрос даёт пустой список.
8. `CountryShape::focus_point` выбирает крупнейшее кольцо, раскрывает долготы
   относительно первой точки (антимеридиан), усредняет координаты и нормализует
   долготу в `[-180, 180]`.

## 4. Данные

- Источник: [Natural Earth 110m](https://www.naturalearthdata.com/) (Admin 0 – Countries), public domain.
- Формат: GeoJSON FeatureCollection.
- Расположение: `assets/geojson/world-110m.json`, встроен в бинарник через `include_str!`.
- Название: `ADMIN` (fallback — `NAME`).
- ISO-коды: `ISO_A2`, при `-99`/пустом значении — `ISO_A2_EH`; невалидные пропускаются.
  `ISO_A3` сохраняется как `Option`.
- Учитываются только `Polygon` и `MultiPolygon`; кольца короче 4 точек и нечисловые
  координаты отбрасываются. Feature без валидного ISO alpha-2 пропускается.

## 5. Производительность

- Построение R-Tree — один раз при старте.
- Запрос `country_at` — один обход R-Tree плюс Point-in-Polygon по кандидатам,
  укладывается в единицы миллисекунд.

## 6. Тесты

- загрузка минимального GeoJSON из файла;
- встроенный датасет содержит более 170 стран;
- DE/RU/AU/AQ определяются по координатам;
- fallback `ISO_A2_EH` для FR/NO;
- Fiji через антимеридиан;
- океан, невалидные координаты и NaN → `None`;
- `country_at_sphere_point` согласован с `country_at`;
- поиск по названию и ISO, пустой запрос, сортировка и лимит;
- выдача колец страны, валидность координат и `None` для неизвестного ISO;
- точка фокуса лежит внутри страны, Фиджи остаётся у антимеридиана, пустая
  форма даёт `None`.

## 7. Связанные ADR

- ADR-0008: Геопространственный поиск
- ADR-0015: UX-редизайн «Orbital Desk» (поиск страны, контур, доворот камеры)
