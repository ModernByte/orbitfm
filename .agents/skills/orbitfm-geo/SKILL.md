---
name: orbitfm-geo
description: Геопространственный модуль OrbitFM (geo, geojson, rstar). Использовать при работе с границами стран, R-Tree, raycasting и Point-in-Polygon.
---

# OrbitFM Geo

Ты работаешь с геосервисом OrbitFM.

Опирайся на ADR-0008, `docs/modules/03-geo.md`, `docs/interfaces/03-geo.md`.

## Стек

- `geo` + `geojson` — геометрия и contains
- `rstar` — R-Tree по bounding box
- Датасет: Natural Earth 110m (Admin 0), offline в `assets/geojson/` или embedded

## Алгоритм

1. Точка на сфере (x, y, z) → lat/lon:
   ```
   φ = arcsin(y / R)
   λ = atan2(z, x)
   ```
2. Быстрый отбор кандидатов через R-Tree.
3. Точная проверка `geo::contains` по MultiPolygon.
4. Полигоны, пересекающие ±180°, нормализуются при загрузке.

## Контракт

Реализуй `GeoService`:

- `load` / `from_embedded`
- `country_at(lat, lon)`
- `country_at_sphere_point(x, y, z)`

## Производительность

- R-Tree строится один раз при старте.
- Запрос `country_at` — единицы миллисекунд.

## Чего нельзя

- Ходить в сеть за границами стран
- Делать полный перебор полигонов без R-Tree
- Смешивать render-логику с geo
- Игнорировать нормализацию антимеридиана
