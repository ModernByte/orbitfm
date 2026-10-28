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
- Рамка и тень остаются только у Focus: `WindowMode::is_borderless()` — это `self != Focus`,
  поэтому Overlay, Listening и Panel открываются с `with_decorations(false)` и без тени.
- Для borderless-режимов: `ViewportBuilder::with_transparent(true)`, `with_decorations(false)`, `App::clear_color` с альфой 0.
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
- Узкая лента — фиксированные 16 полос и размеры 62×16 px; константы `NARROW_VIZ_BARS`
  и `NARROW_VIZ_SIZE` с `bucket_max` живут в `src/ui/widgets.rs` и они общие у капсулы
  Overlay и шапки выпадающей панели (ADR-0022, ADR-0033). Репейн 33 мс только при
  Playing и ноль аллокаций при выключенном визуализаторе; `viz_bars` на неё не влияет.

## Правила кода

- Состояние камеры и input глобуса держать в `GlobeRenderer` / AppState, не размазывать по UI.
- Тяжёлую подготовку GPU делать в `prepare`, отрисовку — в `paint`.
- Не тянуть domain-типы (Station, AudioEngine) внутрь render-модуля.
- Контролы drawer'а настроек — `src/ui/controls.rs`: `dropdown` (`egui::ComboBox` в стиле чипа
  со своим шевроном), `checkbox` (18×18, галка из двух штрихов, hit-зона вместе с подписью) и
  `row` (подпись слева, контрол в фиксированном слоте справа, подсказка под подписью); ширина
  drawer'а `SETTINGS_WIDTH = 380` px. Наборы `selectable_label` и нативный `ui.checkbox` в
  настройках не использовать.
- Колесо над слайдером громкости (`widgets::volume_slider_width`) — ±5 % за щелчок, нормировка
  по `line_scroll_speed`, коммит в `settings.json` сразу (как стрелки) и потребление дельты,
  чтобы drawer под курсором не прокручивался.

## Чего нельзя

- Рисовать 3D вне PaintCallback
- Уничтожать процесс при закрытии окна
- Хардкодить размеры окна без возможности изменения через WM rules
- Игнорировать ограничения прозрачности wgpu
