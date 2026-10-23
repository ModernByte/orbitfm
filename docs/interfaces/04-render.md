# Интерфейс: Globe Renderer

Публичная поверхность модуля `render`: `GlobeInput`, `GlobeCamera`, `GlobeRenderer`,
`CountryStyle`, функции `paint_globe`/`paint_country`, типы маркеров и спектра.
Трейт рендерера не используется; GPU-ресурсы живут в `CallbackResources` egui-wgpu.

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

    /// Плавно переводит камеру к точке `(lat, lon)` за `duration`.
    /// Идёт по кратчайшей дуге долготы; инерция сбрасывается; перетаскивание
    /// отменяет доворот.
    pub fn fly_to(&mut self, lat: f64, lon: f64, duration: std::time::Duration);

    /// Идёт ли доворот или инерционное движение (нужен request_repaint).
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

/// Добавляет paint callback глобуса в painter. `pulse` 0..1 задаёт яркость
/// Fresnel-обода атмосферы; нечисловое значение считается нулём.
pub fn paint_globe(painter: &egui::Painter, rect: egui::Rect, camera: &GlobeCamera, pulse: f32);

/// Стиль страны: заливка и линия контура.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CountryStyle {
    /// Полупрозрачная заливка; `None` — только контур.
    pub fill: Option<egui::Color32>,
    /// Линия контура, включая толщину.
    pub stroke: egui::Stroke,
}

/// Рисует кольца страны (`[lon, lat]`, градусы) поверх области глобуса.
/// Контур обрезается по горизонту; заливка — только для колец, целиком
/// видимых на передней полусфере.
pub fn paint_country(
    painter: &egui::Painter,
    viewport: egui::Rect,
    camera: &GlobeCamera,
    rings: &[Vec<[f64; 2]>],
    style: CountryStyle,
);

/// Цвета маркеров, задаваемые темой приложения.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkerColors {
    /// Обычный маркер; он же accent для станции с неизвестным битрейтом.
    pub marker: egui::Color32,
    /// Маркер станции в эфире.
    pub active: egui::Color32,
}

impl Default for MarkerColors { /* синий маркер, тёплый активный */ }

/// Экранное представление маркера станции; не содержит доменных типов.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkerVisual {
    pub pos: egui::Pos2,
    /// Станция сейчас в эфире: цвет `active`, кольцо и пульс.
    pub playing: bool,
    /// Число станций, совпавших по координатам.
    pub count: usize,
    /// Сигнал популярности (голоса станции/группы): определяет радиус.
    pub popularity: f32,
    /// Битрейт в kbps; 0 — неизвестен, тогда цвет = `colors.marker`.
    pub bitrate: u32,
}

/// Широта/долгота (градусы) → точка единичной сферы по соглашению рендера.
pub fn sphere_point(lat_deg: f32, lon_deg: f32) -> glam::Vec3;

/// Рисует маркеры средствами egui поверх области глобуса.
/// `time` — время интерфейса (`InputState::time`) для пульса играющего маркера.
pub fn paint_markers(
    painter: &egui::Painter,
    bounds: egui::Rect,
    markers: &[MarkerVisual],
    hovered: Option<usize>,
    hovered_label: Option<&str>,
    colors: MarkerColors,
    time: f64,
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

    /// Сигнал эфира 0..1 для Fresnel-атмосферы: `0.75·level + 0.25·flux`.
    /// Без данных и после затухания — 0.
    pub fn pulse(&self) -> f32;

    /// Рисует спектральную огибающую средствами egui.
    pub fn paint(&self, painter: &egui::Painter, rect: egui::Rect, time: f64);
}
```

`CameraUniform` — внутренний тип (`pub(crate)`), содержащий `view_proj`,
`light_direction`, позицию камеры `camera_position` и `pulse`. `GlobeCallback`
реализует `egui_wgpu::CallbackTrait` и не входит в публичный экспорт: снаружи
используется `paint_globe`.

## Семантика

- `GlobeCamera::update` ограничивает `pitch` диапазоном ±85°, дистанцию — `[1.4, 6.0]`,
  `yaw` нормализуется в `[0, 2π)`. Инерция затухает экспоненциально и останавливается
  при скорости ниже порога.
- `GlobeCamera::fly_to` запускает анимацию по кратчайшей дуге; прогресс сглаживается
  smoothstep, минимальная длительность — 50 мс. Целевой `pitch` ограничен ±85°.
  Перетаскивание сбрасывает анимацию; `is_animating` учитывает и доворот, и инерцию.
- `screen_to_sphere` возвращает нормализованную точку единичной сферы или `None`,
  если луч не пересекает сферу либо точка вне viewport.
- `project_on_sphere` — обратная операция: отсекает точки задней полусферы
  (`point · eye <= 1`), нечисловые координаты и результат вне viewport. Используется
  для проецирования маркеров станций.
- `paint_globe` не создаёт ресурсов: `GlobeRenderer` должен быть заранее вставлен в
  `callback_resources` (это делает `OrbitApp::new`). Если ресурса нет, callback
  пропускает отрисовку. `pulse` ограничивается `[0, 1]`, нечисловое значение
  заменяется нулём и передаётся в шейдер через `CameraUniform`.
- Атмосфера — Fresnel-обод в фрагментном шейдере: `pow(1 - dot(normal, view),
  mix(3.2, 1.8, pulse)) · (0.14 + 0.55·pulse)`; цвет сдвигается от голубого к
  бирюзовому с ростом `pulse`. Позиция камеры берётся из `camera_position`.
- Координатное соглашение сферы: `x = cos φ · sin λ`, `y = sin φ`, `z = cos φ · cos λ`;
  `+Z` — Гринвич, `+Y` — север.
- `SpectrumOverlay::update` на первом кадре принимает значения без сглаживания, затем
  интерполирует их экспоненциально; `None` запускает затухание, неположительный `dt`
  игнорируется. `is_visible` становится `false` после затухания.
- `SpectrumOverlay` — только сглаженное состояние: отрисовки в модуле нет
  (старый FFT-strip удалён, ADR-0019). Модуль не знает о `AudioEngine` и
  `SpectrumFrame`: App Core передаёт срез `SpectrumTarget`.
- `SpectrumOverlay::pulse` — смесь сглаженного уровня и flux (`0.75/0.25`),
  ограниченная `[0, 1]`; при невидимом оверлее возвращает 0. App Core передаёт
  значение в `paint_globe`; при выключенном визуализаторе и тишине атмосфера
  затухает.
- `paint_country` рисует кольца страны через `egui::Painter` поверх wgpu-callback:
  контур разбивается на видимые дуги с пересечением горизонта; заливка строится
  только для кольца, целиком видимого на передней полусфере, и триангулируется
  методом уха (кольца с площадью меньше 1 px² не заливаются). Нечисловые
  координаты пропускаются. Стиль контура и заливки задаёт вызывающая сторона.
- `paint_markers` рисует поверх wgpu-callback глобуса: свечение, точку, обводку,
  бейдж количества (`count > 1`), кольцо наведения и подпись в пределах `bounds`.
  Радиус — `3.5 + 9.5·score(popularity)` px, где `score` = log-насыщение голосов
  к 5000 и `smoothstep`; играющая станция не меньше 11 px, получает внешнее кольцо
  и пульс масштаба `1.0 ↔ 1.15` с периодом 1.4 с. Невозможное значение
  популярности (`NaN`, ≤ 0) даёт минимальный радиус, бесконечность — насыщение.
  Цвет — по битрейту: `< 64` `#6B7280`, `64–127` `#3B82F6 → #22D3EE`,
  `128–255` `#34D399 → #A3E635`, `≥ 256` `#FBBF24 → #F97316` (конец тира на 320),
  неизвестный битрейт — `colors.marker`; играющая станция использует `colors.active`.
  Наведение осветляет цвет на 30 %. Модуль не знает о `Station` и теме приложения:
  App Core передаёт готовые `MarkerVisual`, текст подписи, `MarkerColors` и время
  интерфейса. Выбор сигнала популярности (`votes`) — ответственность App Core.

## Ограничения

- Depth buffer не используется: сфера выпуклая, backface culling достаточен.
- Текстура без mipmap, фильтрация линейная.
- Атмосфера — только Fresnel-обод сферы; облака, ночная сторона и свечение
  поверхности не моделируются.
- Подписи стран — вне текущего объёма; маркеры станций и контуры стран
  реализованы egui-оверлеем (ADR-0012), без GPU-ресурсов.
- Спектральный оверлей показывает агрегированные метрики, а не полосы по бинам
  (ADR-0010); это художественная огибающая, а не точный анализатор спектра.
