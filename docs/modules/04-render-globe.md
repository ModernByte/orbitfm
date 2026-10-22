# Модуль: Render / 3D Globe

**Путь в коде:** `src/render/`  
**Ответственность:** Отрисовка 3D-сферы, камера, взаимодействие мыши, интеграция с egui через PaintCallback, спектральный оверлей.
**Зависимости:** egui-wgpu, wgpu, glam, bytemuck, image

---

## 1. Назначение

Визуализация интерактивного глобуса внутри окна egui.  
Рендеринг выполняется кастомным wgpu-пассом через `egui_wgpu::Callback`.

## 2. Публичный интерфейс

```rust
pub use camera::GlobeCamera;
pub use globe::{GlobeRenderer, paint_globe};
pub use markers::{MarkerVisual, paint_markers, sphere_point};
pub use spectrum::{SpectrumOverlay, SpectrumTarget};

pub struct GlobeCamera { /* yaw, pitch, distance, velocities */ }

impl GlobeCamera {
    pub fn update(&mut self, dt: f32, input: &GlobeInput);
    pub fn is_animating(&self) -> bool;
    pub fn eye(&self) -> glam::Vec3;
    pub fn view_proj(&self, aspect: f32) -> glam::Mat4;
    pub fn screen_to_sphere(&self, screen_pos: egui::Pos2, viewport: egui::Rect) -> Option<glam::Vec3>;
    pub fn project_on_sphere(&self, point: glam::Vec3, viewport: egui::Rect) -> Option<egui::Pos2>;
}

pub struct GlobeRenderer { /* pipeline, buffers, texture, bind group */ }

impl GlobeRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, target_format: wgpu::TextureFormat) -> Self;
    pub fn prepare(&self, queue: &wgpu::Queue, uniform: &CameraUniform);
    pub fn paint(&self, render_pass: &mut wgpu::RenderPass<'_>);
}

pub fn paint_globe(painter: &egui::Painter, rect: egui::Rect, camera: &GlobeCamera);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkerVisual {
    pub pos: egui::Pos2,
    pub highlighted: bool,
    pub count: usize,
}

pub fn sphere_point(lat_deg: f32, lon_deg: f32) -> glam::Vec3;

pub fn paint_markers(
    painter: &egui::Painter,
    bounds: egui::Rect,
    markers: &[MarkerVisual],
    hovered: Option<usize>,
    hovered_label: Option<&str>,
);

pub struct GlobeInput {
    pub drag_delta: egui::Vec2,
    pub scroll_delta: f32,
    pub primary_clicked: Option<egui::Pos2>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpectrumTarget {
    pub centroid_hz: f32,
    pub spread_hz: f32,
    pub flux: f32,
    pub flatness: f32,
    pub rms_db: f32,
}

pub struct SpectrumOverlay { /* сглаженные значения */ }

impl SpectrumOverlay {
    pub fn update(&mut self, target: Option<SpectrumTarget>, dt: f32);
    pub fn is_visible(&self) -> bool;
    pub fn paint(&self, painter: &egui::Painter, rect: egui::Rect, time: f64);
}
```

`CameraUniform` (`view_proj`, `light_direction`) и `GlobeCallback` — внутренние типы
модуля. Полный контракт — `docs/interfaces/04-render.md`.

## 3. Интеграция с egui

- GPU-ресурсы `GlobeRenderer` создаются в `OrbitApp::new` из
  `CreationContext::wgpu_render_state` и вставляются в
  `render_state.renderer.write().callback_resources`.
- Каждый кадр `paint_globe` добавляет `egui_wgpu::Callback::new_paint_callback`;
  `GlobeCallback::prepare` пишет uniform камеры, `paint` вызывает
  `GlobeRenderer::paint`.
- Камера и ввод живут в `OrbitApp`, не в `GlobeRenderer`: рендер-модуль не знает о
  состоянии приложения.
- При отсутствии `wgpu_render_state` глобус отключается, UI показывает заглушку
  «3D-глобус недоступен».

## 4. Камера

- Орбитальная камера вокруг сферы радиусом 1; `eye` вычисляется из yaw/pitch/distance.
- Вращение: ПКМ + drag; по горизонтали `yaw -= dx·0.006`, по вертикали
  `pitch += dy·0.006`; после отпускания — инерция с экспоненциальным затуханием
  (damping 6.0, порог остановки 0.01).
- Zoom: колесо, мультипликативный шаг `exp(-scroll·0.002)`, дистанция `[1.4, 6.0]`,
  по умолчанию 2.8.
- `pitch` ограничен ±85°; `yaw` нормализуется в `[0, 2π)`.
- Проекция: FOV 45°, near 0.1, far 50, DirectX-стиль NDC (wgpu): `glam::camera::rh`.
- `screen_to_sphere`: NDC → unproject (z=0 и z=1) → луч из `eye` → ближайшее
  пересечение с единичной сферой.
- `project_on_sphere`: точка единичной сферы → view-projection → экранные координаты;
  `None` для задней полусферы (`point · eye <= 1`), нечисловых значений и выхода
  за viewport.
- Начальная ориентация: yaw=0, pitch=0 — перед камерой Гринвич/Африка, север сверху.

## 5. Визуал (MVP)

- UV-сфера: 64 сегмента × 32 стека, треугольники с CCW-обходом, backface culling,
  без depth buffer (сфера выпуклая).
- Текстура Земли: `assets/textures/earth.jpg` (2048×1024, равнопромежуточная
  проекция, public domain), формат `Rgba8UnormSrgb`, загружается через `image` и
  встроена в бинарник `include_bytes!`.
- Сэмплер: `Repeat` по U, `ClampToEdge` по V, линейная фильтрация, без mipmap.
- Освещение: фиксированное направление `(0.5, 0.35, 0.8)`, `ambient 0.25 + diffuse 0.75`.
- Шейдер: `src/render/globe.wgsl`.
- (Позже) Fresnel-атмосфера и подписи стран.

## 6. Маркеры станций

- `render/markers.rs` не создаёт GPU-ресурсов и не зависит от доменных типов.
  Application Core валидирует координаты, группирует станции и передаёт
  `MarkerVisual` с экранной позицией.
- `sphere_point(lat, lon)` переводит градусы в точку единичной сферы по общему
  соглашению (`x = cos φ · sin λ`, `y = sin φ`, `z = cos φ · cos λ`).
- `paint_markers` рисует поверх wgpu-callback глобуса через `egui::Painter`:
  свечение, точку радиусом 4 px, белую обводку, бейдж количества при `count > 1`,
  кольцо наведения и подпись с фоном, прижатую к границам области.
- Активный маркер (`highlighted`) выделяется тёплым цветом. Отсечение задней
  полусферы и viewport выполняет `project_on_sphere`.
- Поведение взаимодействия (радиус 10 px, приоритет над страной) определяет
  Application Core; см. ADR-0012.

## 7. Спектральный оверлей

- `SpectrumTarget` — срез метрик (`centroid_hz`, `spread_hz`, `flux`, `flatness`,
  `rms_db`) без зависимости от аудио-модуля.
- `SpectrumOverlay` сглаживает значения экспоненциально; первый кадр принимается
  как есть, `None` запускает затухание. Состояние живёт в `OrbitApp`.
- Рисуется в `egui::Painter` поверх области глобуса: полупрозрачная полоса,
  градиентная огибающая в лог-шкале 30 Гц–20 кГц, линия уровня и подпись
  `FFT · <частота> · <RMS> дБ`.
- Оверлей не создаёт GPU-ресурсов и не участвует в wgpu-пайплайне глобуса.
- Метрики агрегированные, поэтому огибающая — художественная интерпретация,
  а не покадровый спектр по бинам (ADR-0010).

## 8. Тесты

- все треугольники меша обращены наружу;
- вершины лежат на единичной сфере, UV в `[0, 1]`;
- центральный луч попадает в видимую полусферу;
- центральный луч соответствует yaw/pitch камеры (Берлин);
- луч из угла viewport не пересекает сферу;
- drag меняет ориентацию и сохраняет инерцию;
- зум и pitch ограничены;
- `project_on_sphere` проецирует видимую точку в центр, учитывает широту/долготу и
  отсекает заднюю полусферу, пустой viewport и нечисловые координаты;
- `sphere_point` следует соглашению сферы и возвращает нормализованные векторы;
- спектральный оверлей принимает первый кадр без сглаживания, затухает без данных,
  ограничивает частоты и игнорирует неположительный `dt`.

## 9. Связанные ADR

- ADR-0002: Выбор GUI-стека
- ADR-0008: Геопространственный поиск
- ADR-0010: Экспериментальный FFT-визуализатор
- ADR-0012: Маркеры станций на глобусе
