# Модуль: Render / 3D Globe

**Путь в коде:** `src/render/`  
**Ответственность:** Отрисовка 3D-сферы, камера, взаимодействие мыши, интеграция с egui через PaintCallback.  
**Зависимости:** egui-wgpu, wgpu, glam (или аналог для матриц)

---

## 1. Назначение

Визуализация интерактивного глобуса внутри окна egui.  
Рендеринг выполняется кастомным wgpu-пассом.

## 2. Публичный интерфейс

```rust
pub struct GlobeRenderer { /* ... */ }

impl GlobeRenderer {
    /// Инициализация пайплайна, текстур, буферов
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self;

    /// Обновление состояния камеры и анимации
    pub fn update(&mut self, dt: f32, input: &GlobeInput);

    /// Подготовка данных для кадра
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue);

    /// Выполнение рендера в переданный pass (вызывается из Callback)
    pub fn paint<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>);

    /// Преобразование экранных координат в луч / точку на сфере
    pub fn screen_to_sphere(&self, screen_pos: egui::Pos2, viewport: egui::Rect) -> Option<glam::Vec3>;
}

#[derive(Default)]
pub struct GlobeInput {
    pub drag_delta: egui::Vec2,       // ПКМ + drag
    pub scroll_delta: f32,            // zoom
    pub primary_clicked: Option<egui::Pos2>,
}
```

## 3. Интеграция с egui

Используется официальный механизм `egui_wgpu::Callback` / `PaintCallback`:

- В `App::logic` обновляется камера и ввод; в `App::ui` создаётся `egui::PaintCallback` с `CallbackTrait`
- В `paint` вызывается `GlobeRenderer::paint`
- Ресурсы живут в `CallbackResources` или в состоянии приложения

## 4. Камера

- Вращение: ПКМ + drag с инерцией (damping)
- Zoom: колесо мыши, ограничение min/max дистанции
- Матрицы: perspective + view (look-at)

## 5. Визуал (MVP)

- UV-сфера
- Диффузная текстура Земли
- Простой ambient + directional light
- (Позже) Fresnel-атмосфера, markers станций

## 6. Связанные ADR

- ADR-0002: Выбор GUI-стека
