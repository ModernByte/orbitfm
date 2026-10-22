# Модуль: Geo / Spatial Raycasting

**Путь в коде:** `src/geo/`  
**Ответственность:** Загрузка границ стран, R-Tree индекс, преобразование координат, Point-in-Polygon.  
**Зависимости:** geo, geojson, rstar

---

## 1. Назначение

По клику на 3D-глобусе определить страну.  
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
}

#[derive(Debug, Clone)]
pub struct Country {
    pub name: String,
    pub iso_a2: String,          // ISO 3166-1 alpha-2
    pub iso_a3: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum GeoError {
    #[error("не удалось загрузить GeoJSON: {0}")]
    Load(String),
    #[error("некорректные данные границ")]
    InvalidGeometry,
}
```

Геометрия (`MultiPolygon<f64>`) хранится внутри `GeoService` и наружу не выходит.
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
- `country_at_sphere_point` согласован с `country_at`.

## 7. Связанные ADR

- ADR-0008: Геопространственный поиск
