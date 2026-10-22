# Интерфейс: Globe Renderer

Публичная поверхность модуля `render`: `GlobeInput`, `GlobeCamera`, `GlobeRenderer`
и функция `paint_globe`. Трейт рендерера не используется; GPU-ресурсы живут в
`CallbackResources` egui-wgpu.

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
- `paint_globe` не создаёт ресурсов: `GlobeRenderer` должен быть заранее вставлен в
  `callback_resources` (это делает `OrbitApp::new`). Если ресурса нет, callback
  пропускает отрисовку.
- Координатное соглашение сферы: `x = cos φ · sin λ`, `y = sin φ`, `z = cos φ · cos λ`;
  `+Z` — Гринвич, `+Y` — север.

## Ограничения

- Depth buffer не используется: сфера выпуклая, backface culling достаточен.
- Текстура без mipmap, фильтрация линейная.
- Атмосфера, маркеры станций и подписи стран — вне текущего объёма.
