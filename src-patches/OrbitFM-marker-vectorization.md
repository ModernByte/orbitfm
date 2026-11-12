# Векторизация обработки маркеров OrbitFM

| Поле | Значение |
|------|----------|
| **Дата** | 2026-11-12 |
| **Патч** | `marker-batch-vectorization.patch` |

## Конвейер (CPU, один кадр)

```text
GlobeMarker[] ──lat/lon──► SpherePointSoA (unit x/y/z)
        │
        ▼ deform_into (scale_xz / scale_y)
   world SoA x/y/z
        │
        ▼ project_points_on_sphere_soa  (одна view_proj, один eye)
   positions[] + indices[]     // только видимые
        │
        ▼ build_visuals(playing[], selected[])
   MarkerVisual[] ──► paint_markers (egui)
```

## Что даёт

| Шаг | Приём |
|-----|--------|
| Проекция | Батч, матрица не пересчитывается на точку; SoA для плотного доступа |
| Флаги | Параллельные `bool` массивы по **исходному** индексу, не `Option` в project-loop |
| Hit-test | `screen_pairs()` — те же positions/indices |
| Аллокации | `MarkerBatchScratch` можно держать между кадрами |

## API

- `SpherePointSoA` — unit points  
- `GlobeCamera::project_points_on_sphere` / `_soa`  
- `MarkerBatchScratch::project_lat_lons` / `build_visuals` / `screen_pairs`  
- `marker_visuals_batch` в `globe_view` — один проход на кадр  

## Не SIMD-intrinsics

Явного `std::simd` / `wide` нет: цикл SoA в `project_points_on_sphere_soa` написан так, чтобы LLVM мог автовекторизовать mul/add. Ручной `f32x4` — следующий шаг по профилю, если `markers_project` > ~0.5–1 ms.

## Следующие шаги

1. Хранить `MarkerBatchScratch` в UI state (zero realloc).  
2. Кэш unit-SoA при неизменном списке станций.  
3. LOD при малом `projected_radius`.  
4. При необходимости — explicit SIMD в `project_points_on_sphere_soa`.
