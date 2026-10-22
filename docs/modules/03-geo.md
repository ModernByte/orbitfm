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
    /// Загружает GeoJSON и строит R-Tree
    pub fn load(path: &Path) -> Result<Self, GeoError>;

    /// Или из встроенных данных
    pub fn from_embedded() -> Result<Self, GeoError>;

    /// Найти страну по географическим координатам
    pub fn country_at(&self, lat: f64, lon: f64) -> Option<&Country>;

    /// Найти страну по точке на единичной сфере (x, y, z)
    pub fn country_at_sphere_point(&self, x: f64, y: f64, z: f64) -> Option<&Country>;
}

#[derive(Debug, Clone)]
pub struct Country {
    pub name: String,
    pub iso_a2: String,          // ISO 3166-1 alpha-2
    pub iso_a3: Option<String>,
    // геометрия хранится внутри сервиса
}
```

## 3. Алгоритм

1. Луч из экранных координат → через inverse view-projection → пересечение со сферой.
2. Точка пересечения (x, y, z) → географические координаты:
   ```
   φ = arcsin(y / R)
   λ = atan2(z, x)
   ```
3. Быстрый отбор кандидатов через R-Tree по bounding box.
4. Точная проверка `geo::contains` для MultiPolygon.
5. Полигоны, пересекающие ±180°, нормализуются при загрузке.

## 4. Данные

- Источник: Natural Earth 110m (Admin 0 – Countries)
- Формат: GeoJSON
- Расположение: `assets/geojson/world-110m.json` (или embedded через `include_dir` / `rust-embed`)

## 5. Производительность

- Построение R-Tree — один раз при старте
- Запрос `country_at` должен укладываться в единицы миллисекунд

## 6. Связанные ADR

- ADR-0008: Геопространственный поиск
