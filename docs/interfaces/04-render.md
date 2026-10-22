# Интерфейс: Globe Renderer

Публичная поверхность модуля `render`: `GlobeInput`, `GlobeCamera`, `GlobeRenderer`,
`SpectrumTarget`, `SpectrumOverlay` и функция `paint_globe`. Трейт рендерера не
используется; GPU-ресурсы живут в `CallbackResources` egui-wgpu.

```rust
/// Ввод от пользователя для глобуса.
#[derive(Debug, Clone, Default)]
pub struct GlobeInput {
    /// Смещение при перетаскивании (ПКМ), пункты экрана.
    pub drag_delta: egui::Vec2,
    /// Прокрутка колеса (zoom).
    pub scroll_delta: f32,
    /// Клик ЛКМ в экранных координатах.
    pub primary_clicked: Option<egui::Pos2>,
}

/// Орбитальная камера вокруг глобуса радиусом 1.
#[derive(Debug)]
pub struct GlobeCamera { /* yaw, pitch, distance, velocities */ }

impl Default for GlobeCamera { /* ... */ }

impl GlobeCamera {
    /// Обновляет ориентацию, инерцию и зум по вводу за кадр.
    pub fn update(&mut self, dt: f32, input: &GlobeInput);

    /// Идёт ли инерционное движение (нужен request_repaint).
    pub fn is_animating(&self) -> bool;

    /// Позиция камеры в мировых координатах.
    pub fn eye(&self) -> glam::Vec3;

    /// Матрица вида и проекции для заданного aspect.
    pub fn view_proj(&self, aspect: f32) -> glam::Mat4;

    /// Экранные координаты → точка на единичной сфере.
    pub fn screen_to_sphere(
        &self,
        screen_pos: egui::Pos2,
        viewport: egui::Rect,
    ) -> Option<glam::Vec3>;

    /// Точка единичной сферы → экранные координаты; `None` для обратной полусферы
    /// и выхода за viewport.
    pub fn project_on_sphere(
        &self,
        point: glam::Vec3,
        viewport: egui::Rect,
    ) -> Option<egui::Pos2>;
}

/// GPU-ресурсы: пайплайн, сфера, текстура Земли, uniform-буфер.
pub struct GlobeRenderer { /* ... */ }

impl GlobeRenderer {
    /// Создаётся один раз при старте и кладётся в CallbackResources.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
    ) -> Self;

    /// Записывает униформ-данные камеры в GPU-буфер.
    pub fn prepare(&self, queue: &wgpu::Queue, uniform: &CameraUniform);

    /// Рисует сферу в текущий render pass.
    pub fn paint(&self, render_pass: &mut wgpu::RenderPass<'_>);
}

/// Добавляет paint callback глобуса в painter.
pub fn paint_globe(painter: &egui::Painter, rect: egui::Rect, camera: &GlobeCamera);

/// Экранное представление маркера станции; не содержит доменных типов.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkerVisual {
    pub pos: egui::Pos2,
    /// Маркер соответствует текущей станции.
    pub highlighted: bool,
    /// Число станций, совпавших по координатам.
    pub count: usize,
}

/// Широта/долгота (градусы) → точка единичной сферы по соглашению рендера.
pub fn sphere_point(lat_deg: f32, lon_deg: f32) -> glam::Vec3;

/// Рисует маркеры средствами egui поверх области глобуса.
pub fn paint_markers(
    painter: &egui::Painter,
    bounds: egui::Rect,
    markers: &[MarkerVisual],
    hovered: Option<usize>,
    hovered_label: Option<&str>,
);

/// Срез FFT-метрик для визуализации; не содержит доменных типов.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpectrumTarget {
    pub centroid_hz: f32,
    pub spread_hz: f32,
    pub flux: f32,
    pub flatness: f32,
    pub rms_db: f32,
}

/// Сглаженное состояние спектрального оверлея.
#[derive(Debug, Default)]
pub struct SpectrumOverlay { /* ... */ }

impl SpectrumOverlay {
    /// Обновляет сглаженные значения за кадр; `None` — данных нет.
    pub fn update(&mut self, target: Option<SpectrumTarget>, dt: f32);

    /// Видим ли оверлей (есть данные или ещё не затух).
    pub fn is_visible(&self) -> bool;

    /// Рисует спектральную огибающую средствами egui.
    pub fn paint(&self, painter: &egui::Painter, rect: egui::Rect, time: f64);
}
```

`CameraUniform` — внутренний тип (`pub(crate)`), содержащий `view_proj` и
`light_direction`. `GlobeCallback` реализует `egui_wgpu::CallbackTrait` и не входит
в публичный экспорт: снаружи используется `paint_globe`.

## Семантика

- `GlobeCamera::update` ограничивает `pitch` диапазоном ±85°, дистанцию — `[1.4, 6.0]`,
  `yaw` нормализуется в `[0, 2π)`. Инерция затухает экспоненциально и останавливается
  при скорости ниже порога.
- `screen_to_sphere` возвращает нормализованную точку единичной сферы или `None`,
  если луч не пересекает сферу либо точка вне viewport.
- `project_on_sphere` — обратная операция: отсекает точки задней полусферы
  (`point · eye <= 1`), нечисловые координаты и результат вне viewport. Используется
  для проецирования маркеров станций.
- `paint_globe` не создаёт ресурсов: `GlobeRenderer` должен быть заранее вставлен в
  `callback_resources` (это делает `OrbitApp::new`). Если ресурса нет, callback
  пропускает отрисовку.
- Координатное соглашение сферы: `x = cos φ · sin λ`, `y = sin φ`, `z = cos φ · cos λ`;
  `+Z` — Гринвич, `+Y` — север.
- `SpectrumOverlay::update` на первом кадре принимает значения без сглаживания, затем
  интерполирует их экспоненциально; `None` запускает затухание, неположительный `dt`
  игнорируется. `is_visible` становится `false` после затухания.
- `SpectrumOverlay::paint` рисует только через `egui::Painter` (без wgpu-ресурсов):
  полупрозрачную полосу, градиентную огибающую в логарифмической шкале
  30 Гц–20 кГц, линию уровня и подпись `FFT · <частота> · <RMS> дБ`.
  Модуль не знает о `AudioEngine` и `SpectrumFrame`: App Core передаёт срез
  `SpectrumTarget`.
- `paint_markers` рисует поверх wgpu-callback глобуса: свечение, точку, обводку,
  бейдж количества (`count > 1`), кольцо наведения и подпись в пределах `bounds`.
  Активный маркер выделяется цветом. Модуль не знает о `Station`: App Core передаёт
  готовые `MarkerVisual` и текст подписи.

## Ограничения

- Depth buffer не используется: сфера выпуклая, backface culling достаточен.
- Текстура без mipmap, фильтрация линейная.
- Атмосфера и подписи стран — вне текущего объёма; маркеры станций реализованы
  egui-оверлеем (ADR-0012).
- Спектральный оверлей показывает агрегированные метрики, а не полосы по бинам
  (ADR-0010); это художественная огибающая, а не точный анализатор спектра.
