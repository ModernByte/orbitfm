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
pub struct GlobeCamera { /* yaw, pitch, distance, velocities, fly, idle */ }

impl Default for GlobeCamera { /* ... */ }

impl GlobeCamera {
    /// Обновляет ориентацию, инерцию, зум и авто-вращение по вводу за кадр.
    /// `dt` — сглаженный интервал физики; `elapsed` — фактический интервал
    /// кадра для отсчёта простоя (после сна окна `stable_dt` остаётся малым).
    pub fn advance(&mut self, dt: f32, elapsed: f32, input: &GlobeInput);

    /// Плавно переводит камеру к точке `(lat, lon)` за `duration`.
    /// Идёт по кратчайшей дуге долготы; инерция сбрасывается; перетаскивание
    /// отменяет доворот.
    pub fn fly_to(&mut self, lat: f64, lon: f64, duration: std::time::Duration);

    /// Идёт ли движение, требующее перерисовки: доворот, инерция или
    /// авто-вращение в простое.
    pub fn is_animating(&self) -> bool;

    /// Идёт ли медленное авто-вращение после 4 с без взаимодействия.
    pub fn is_idle_spinning(&self) -> bool;

    /// Секунды до начала авто-вращения; `None`, если уже вращается или идёт
    /// доворот.
    pub fn idle_countdown(&self) -> Option<f32>;

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

/// Добавляет paint callback глобуса в painter. Аудио-пульс не передаётся:
/// Fresnel-обод атмосферы статичен.
pub fn paint_globe(painter: &egui::Painter, rect: egui::Rect, camera: &GlobeCamera);

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

/// Подпись наведённого маркера: имя и вторая строка с местом, качеством и
/// рейтингом. Строки раскладываются разными шрифтами, поэтому это не одна
/// строка с `\n`.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkerLabel {
    pub title: String,
    pub details: Option<String>,
}

/// Экранное представление маркера станции; не содержит доменных типов.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkerVisual {
    pub pos: egui::Pos2,
    /// Станция сейчас в эфире: пульсирующее кольцо того же цвета.
    pub playing: bool,
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
    hovered_label: Option<&MarkerLabel>,
    color: egui::Color32,
    time: f64,
);

/// Нормированные уровни полос `0.0..=1.0`; не зависит от аудио-модуля.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BandTargets {
    pub bass: f32,
    pub mid: f32,
    pub high: f32,
}

impl BandTargets {
    /// Нормирует дБFS: пол −90 дБ, диапазон 80 дБ; нефинитное значение — 0.
    pub fn from_db(bass_db: f32, mid_db: f32, high_db: f32) -> Self;
}

/// Срез FFT-метрик и полос для визуализации; не содержит доменных типов.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpectrumTarget {
    pub centroid_hz: f32,
    pub spread_hz: f32,
    pub flux: f32,
    pub flatness: f32,
    pub rms_db: f32,
    /// `None` — полосы недоступны; потребитель обязан работать в этом режиме.
    pub bands: Option<BandTargets>,
}

/// Сглаженное состояние спектрального оверлея.
#[derive(Debug, Default)]
pub struct SpectrumOverlay { /* ... */ }

impl SpectrumOverlay {
    /// Обновляет сглаженные значения за кадр; `None` — данных нет.
    pub fn update(&mut self, target: Option<SpectrumTarget>, dt: f32);

    /// Видим ли оверлей (есть данные или ещё не затух).
    pub fn is_visible(&self) -> bool;

    /// Сигнал эфира 0..1 для кольца Listening: `0.75·level + 0.25·flux`.
    /// Без данных и после затухания — 0. На глобус не передаётся.
    pub fn pulse(&self) -> f32;

    /// Рисует спектральную огибающую средствами egui.
    pub fn paint(&self, painter: &egui::Painter, rect: egui::Rect, time: f64);
}
```

`CameraUniform` — внутренний тип (`pub(crate)`), содержащий `view_proj`,
`light_direction` и позицию камеры `camera_position`. `GlobeCallback`
реализует `egui_wgpu::CallbackTrait` и не входит в публичный экспорт: снаружи
используется `paint_globe`.

## Семантика

- `GlobeCamera::advance` ограничивает `pitch` диапазоном ±85°, дистанцию —
  `[1.4, 6.0]`, `yaw` нормализуется в `[0, 2π)`. Инерция затухает экспоненциально
  и останавливается при скорости ниже порога. Физика использует сглаженный `dt`
  (0,001–0,05 с), простой — фактический `elapsed` (0–60 с).
- `GlobeCamera::fly_to` запускает анимацию по кратчайшей дуге; прогресс сглаживается
  smoothstep, минимальная длительность — 50 мс. Целевой `pitch` ограничен ±85°.
  Перетаскивание сбрасывает анимацию; `is_animating` учитывает и доворот, и инерцию.
- После 4 с без перетаскивания, прокрутки и клика камера начинает авто-вращение:
  разгон 2 с до 0,006 рад/с, `yaw` уменьшается, поэтому континенты уходят на
  восток. Доворот `fly_to` и любое взаимодействие сбрасывают отсчёт. Наведение
  курсора простоем не считается. `idle_countdown` возвращает остаток паузы,
  `is_idle_spinning` — факт вращения.
- `screen_to_sphere` возвращает нормализованную точку единичной сферы или `None`,
  если луч не пересекает сферу либо точка вне viewport.
- `project_on_sphere` — обратная операция: отсекает точки задней полусферы
  (`point · eye <= 1`), нечисловые координаты и результат вне viewport. Используется
  для проецирования маркеров станций.
- `projected_radius(viewport)` — экранный радиус силуэта единичной сферы:
  `focal = 0.5·height / tan(FOV/2)`, `radius = focal / sqrt(distance² − 1)`.
  Нулевой или нечисловой viewport даёт 0. Центр силуэта совпадает с центром
  viewport'а; радиус питает орбитальное аудиокольцо (ADR-0029).
- `paint_globe` не создаёт ресурсов: `GlobeRenderer` должен быть заранее вставлен в
  `callback_resources` (это делает `OrbitApp::new`). Если ресурса нет, callback
  пропускает отрисовку. Вершинный шейдер не масштабирует радиус; hit-test и
  проекция маркеров остаются на сфере радиуса 1. Всё, что painter рисует до
  вызова `paint_globe`, оказывается за сферой; этим пользуется орбитальное
  аудиокольцо (ADR-0029).
- Атмосфера — статичный Fresnel-обод в фрагментном шейдере:
  `pow(1 - dot(normal, view), 3.2) · 0.12`; поверх лежит узкая кромка
  `pow(1 - facing, 9) · 0.2`. Аудио-пульс удалён 2026-10-24 (ADR-0023,
  ADR-0028); позиция камеры берётся из `camera_position`.
- Координатное соглашение сферы: `x = cos φ · sin λ`, `y = sin φ`, `z = cos φ · cos λ`;
  `+Z` — Гринвич, `+Y` — север.
- `SpectrumOverlay::update` на первом кадре принимает значения без сглаживания, затем
  интерполирует их экспоненциально; `None` запускает затухание, неположительный `dt`
  игнорируется. `is_visible` становится `false` после затухания.
- `SpectrumOverlay` — только сглаженное состояние: отрисовки в модуле нет
  (старый FFT-strip удалён, ADR-0019). Модуль не знает о `AudioEngine` и
  `SpectrumFrame`: App Core передаёт срез `SpectrumTarget`.
- `SpectrumTarget.bands` заполняется App Core из `SpectrumFrame.bands` через
  `BandTargets::from_db` (пол −90 дБ, диапазон 80 дБ); при `None` потребители
  обязаны деградировать к полному RMS+flux, а не исчезать (ADR-0027, ADR-0028).
  В Phase 0 поле только передаётся: орбитальное кольцо и глобус его не читают,
  пока не появятся потребители Phase 1.
- `SpectrumOverlay::pulse` — смесь сглаженного уровня и flux (`0.75/0.25`),
  ограниченная `[0, 1]`; при невидимом оверлее возвращает 0. App Core передаёт
  значение в кольцо Listening; при выключенном визуализаторе и тишине кольцо
  затухает. На `paint_globe` сигнал не влияет.
- `paint_country` рисует кольца страны через `egui::Painter` поверх wgpu-callback:
  контур разбивается на видимые дуги с пересечением горизонта; заливка строится
  только для кольца, целиком видимого на передней полусфере, и триангулируется
  методом уха (кольца с площадью меньше 1 px² не заливаются). Нечисловые
  координаты пропускаются. Стиль контура и заливки задаёт вызывающая сторона.
- `paint_markers` рисует поверх wgpu-callback глобуса: свечение, точку радиусом
  3 px, тонкую белую обводку, пульсирующее кольцо у играющей станции (период
  1,4 с, расходится на 3,5 px и гаснет), белое кольцо наведения и подпись
  `MarkerLabel` в пределах `bounds`. Все маркеры одного цвета (`palette.marker`),
  наведение осветляет его на 28 %. Подпись — две строки: имя белым, вторая
  строка (место, битрейт, кодек, голоса) цветом `#C7D2E4`; фон — тёмный
  скруглённый прямоугольник с рамкой акцента. Модуль не знает о `Station`:
  App Core передаёт готовые `MarkerVisual`, `MarkerLabel`, цвет и время
  интерфейса. Качество и рейтинг станции — ответственность App Core.

## Ограничения

- Depth buffer не используется: сфера выпуклая, backface culling достаточен.
- Текстура без mipmap, фильтрация линейная.
- Атмосфера — только Fresnel-обод сферы; облака, ночная сторона и свечение
  поверхности не моделируются.
- Подписи стран — вне текущего объёма; маркеры станций и контуры стран
  реализованы egui-оверлеем (ADR-0012), без GPU-ресурсов.
- Спектральный оверлей показывает агрегированные метрики, а не полосы по бинам
  (ADR-0010); `BandTargets` — три огибающие, а не точный анализатор спектра.
