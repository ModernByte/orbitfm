# Интерфейс: Globe Renderer

```rust
/// Ввод от пользователя для глобуса
#[derive(Debug, Clone, Default)]
pub struct GlobeInput {
    /// Смещение при перетаскивании (ПКМ)
    pub drag_delta: egui::Vec2,
    /// Колесо мыши (zoom)
    pub scroll_delta: f32,
    /// Клик ЛКМ (экранные координаты)
    pub primary_clicked: Option<egui::Pos2>,
}

/// Контракт рендерера
pub trait GlobeRenderer: Send {
    /// Обновление камеры и анимации
    fn update(&mut self, dt: f32, input: &GlobeInput);

    /// Подготовка GPU-ресурсов к кадру
    fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue);

    /// Отрисовка в render pass (вызывается из egui Callback)
    fn paint<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>);

    /// Экранные координаты → точка на сфере (если есть пересечение)
    fn screen_to_sphere(
        &self,
        screen_pos: egui::Pos2,
        viewport: egui::Rect,
    ) -> Option<(f64, f64, f64)>; // (x, y, z)
}
```

Примечание: точная сигнатура `paint` зависит от версии `egui-wgpu` и способа передачи `CallbackResources`.  
При реализации следует опираться на актуальные примеры из `egui-wgpu`.
