---
name: orbitfm-gui
description: GUI и 3D-глобус OrbitFM на eframe, egui и egui-wgpu. Использовать при работе с UI, окном, прозрачностью, PaintCallback, камерой глобуса и кастомным wgpu-рендером.
---

# OrbitFM GUI

Ты работаешь с UI-слоем OrbitFM (eframe + egui + egui-wgpu).

Опирайся на ADR-0002, ADR-0010, ADR-0012 и `docs/modules/04-render-globe.md`,
`docs/interfaces/04-render.md`.

## Стек

- `eframe` — жизненный цикл приложения и окно
- `egui` — immediate-mode UI
- `egui-wgpu` — бэкенд + `Callback` / `PaintCallback` для 3D

## Окно и режимы

- Standalone и Overlay — один и тот же процесс.
- Закрытие окна = Hide (`window_visible = false`), процесс продолжает жить.
- Для overlay: `ViewportBuilder::with_transparent(true)`, `with_decorations(false)`, `App::clear_color` с альфой 0.
- Прозрачность с wgpu имеет известные ограничения (особенно Windows). На Linux тестировать в первую очередь. При проблемах допустим fallback на glow для overlay.

## 3D-глобус

- Рендер только через официальный `egui_wgpu::Callback` / `PaintCallback`.
- Не использовать отдельное окно или конфликтующие 3D-движки.
- Камера: ПКМ + drag (с инерцией), колесо — zoom с min/max.
- Клик ЛКМ → `screen_to_sphere` → точка (x, y, z) → передаётся в GeoService.

## Маркеры станций

- Проекция — `GlobeCamera::project_on_sphere`, отсечение задней полусферы и viewport.
- Рисовать egui-оверлеем после `paint_globe` через `paint_markers`; GPU-ресурсы не нужны.
- В render-модуль передавать `MarkerVisual`, не `Station`; группировку и подписи готовит App Core.
- Клик по маркеру приоритетнее выбора страны; радиус — 10 px.

## FFT-оверлей

- Рисовать через `egui::Painter` поверх области глобуса, а не отдельным GPU-пассом.
- В render-модуль передавать `SpectrumTarget`, не `SpectrumFrame`/`AudioEngine`.
- Сглаживание и затухание — в `SpectrumOverlay`; App Core только поставляет данные.
- Пока оверлей виден, запрашивать repaint (~33 мс).

## Правила кода

- Состояние камеры и input глобуса держать в `GlobeRenderer` / AppState, не размазывать по UI.
- Тяжёлую подготовку GPU делать в `prepare`, отрисовку — в `paint`.
- Не тянуть domain-типы (Station, AudioEngine) внутрь render-модуля.

## Чего нельзя

- Рисовать 3D вне PaintCallback
- Уничтожать процесс при закрытии окна
- Хардкодить размеры окна без возможности изменения через WM rules
- Игнорировать ограничения прозрачности wgpu
