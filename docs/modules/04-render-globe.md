# Модуль: Render / 3D Globe

**Путь в коде:** `src/render/`  
**Ответственность:** Отрисовка 3D-сферы, камера, взаимодействие мыши, интеграция с egui через PaintCallback, контуры стран, спектральный оверлей.
**Зависимости:** egui-wgpu, wgpu, glam, bytemuck, image

---

## 1. Назначение

Визуализация интерактивного глобуса внутри окна egui.  
Рендеринг выполняется кастомным wgpu-пассом через `egui_wgpu::Callback`.

## 2. Публичный интерфейс

```rust
pub use camera::GlobeCamera;
pub use countries::{CountryStyle, paint_country};
pub use globe::{GlobeRenderer, paint_globe};
pub use markers::{MarkerLabel, MarkerVisual, paint_markers, sphere_point};
pub use motion::{GlobeDeform, GlobeMotion, MotionMixer};
pub use spectrum::{SpectrumOverlay, SpectrumTarget};
pub use visualizer::VisualizerModel;

pub struct GlobeCamera { /* yaw, pitch, distance, velocities, fly, idle */ }

impl GlobeCamera {
    pub fn advance(&mut self, dt: f32, elapsed: f32, input: &GlobeInput);
    pub fn fly_to(&mut self, lat: f64, lon: f64, duration: std::time::Duration);
    pub fn is_animating(&self) -> bool;
    pub fn is_idle_spinning(&self) -> bool;
    pub fn idle_countdown(&self) -> Option<f32>;
    pub fn eye(&self) -> glam::Vec3;
    pub fn view_proj(&self, aspect: f32) -> glam::Mat4;
    pub fn screen_to_sphere_deformed(
        &self,
        screen_pos: egui::Pos2,
        viewport: egui::Rect,
        deform: &GlobeDeform,
    ) -> Option<glam::Vec3>;
    pub fn project_on_sphere(&self, point: glam::Vec3, viewport: egui::Rect) -> Option<egui::Pos2>;
}

pub struct GlobeRenderer { /* pipeline, buffers, texture, bind group */ }

impl GlobeRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, target_format: wgpu::TextureFormat) -> Self;
    pub fn prepare(&self, queue: &wgpu::Queue, uniform: &CameraUniform);
    pub fn paint(&self, render_pass: &mut wgpu::RenderPass<'_>);
}

pub fn paint_globe(
    painter: &egui::Painter,
    rect: egui::Rect,
    camera: &GlobeCamera,
    motion: &GlobeMotion,
);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CountryStyle {
    pub fill: Option<egui::Color32>,
    pub stroke: egui::Stroke,
}

pub fn paint_country(
    painter: &egui::Painter,
    viewport: egui::Rect,
    camera: &GlobeCamera,
    deform: &GlobeDeform,
    rings: &[Vec<[f64; 2]>],
    style: CountryStyle,
);

#[derive(Debug, Clone, PartialEq)]
pub struct MarkerLabel {
    pub title: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkerVisual {
    pub pos: egui::Pos2,
    pub playing: bool,
}

pub fn sphere_point(lat_deg: f32, lon_deg: f32) -> glam::Vec3;

pub fn paint_markers(
    painter: &egui::Painter,
    bounds: egui::Rect,
    markers: &[MarkerVisual],
    hovered: Option<usize>,
    hovered_label: Option<&MarkerLabel>,
    color: egui::Color32,
    time: f64,
);

pub struct GlobeInput {
    pub drag_delta: egui::Vec2,
    pub scroll_delta: f32,
    pub primary_clicked: Option<egui::Pos2>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BandTargets {
    pub bass: f32,
    pub mid: f32,
    pub high: f32,
}

pub struct SpectrumTarget {
    pub centroid_hz: f32,
    pub spread_hz: f32,
    pub flux: f32,
    pub flatness: f32,
    pub rms_db: f32,
    pub bands: Option<BandTargets>,
}

pub struct SpectrumOverlay { /* сглаженные значения */ }

impl SpectrumOverlay {
    pub fn update(&mut self, target: Option<SpectrumTarget>, dt: f32);
    pub fn is_visible(&self) -> bool;
    pub fn pulse(&self) -> f32;
    pub fn paint(&self, painter: &egui::Painter, rect: egui::Rect, time: f64);
}
```

`CameraUniform` (`view_proj`, `light_direction`, `camera_position`, `motion`) и
`GlobeCallback` — внутренние типы модуля. Полный контракт —
`docs/interfaces/04-render.md`.

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
  по умолчанию 3.2 (ТЗ §5.1 п. 6 требовал крупную Землю; дистанция увеличена,
  чтобы за планетой помещалось орбитальное аудиокольцо — ADR-0029). Планета
  занимает около 79 % высоты окна, поэтому в стартовой ориентации видна большая
  часть обращённой к камере полусферы.
- `pitch` ограничен ±85°; `yaw` нормализуется в `[0, 2π)`.
- Проекция: FOV 45°, near 0.1, far 50, DirectX-стиль NDC (wgpu): `glam::camera::rh`.
- `screen_to_sphere_deformed`: NDC → unproject (z=0 и z=1) → луч из `eye`;
  `eye`, near и far переводятся в пространство единичной сферы обратным
  масштабом `GlobeDeform::to_unit`, затем ищется ближайшее пересечение.
  Без деформации поведение прежнее.
- `project_on_sphere`: точка поверхности → view-projection → экранные координаты;
  `None` для задней полусферы (`point · eye <= |point|²`), нечисловых значений
  и выхода за viewport. Принимает и единичную, и деформированную точку.
- `projected_radius(viewport)`: `focal = 0.5·height / tan(45°/2)`,
  `radius = focal / sqrt(distance² − 1)` — экранный радиус силуэта; центр совпадает
  с центром viewport'а. Радиус передаётся в `OrbitRing::paint` (ADR-0029).
- `advance(dt, elapsed, input, motion, allow_flips)` ограничивает физический `dt`
  0,001–0,05 с и фактический `elapsed` 0–60 с; `yaw` нормализуется в `[0, 2π)`,
  `pitch` — ±85°. `motion` задаёт музыкальную качку и `pulse`, `allow_flips` —
  разрешение Core с учётом режима окна и Reduce motion.
- `fly_to(lat, lon, duration)` — плавный доворот к точке: `yaw` идёт по кратчайшей
  дуге, `pitch` ограничен ±85°, прогресс сглаживается smoothstep, минимальная
  длительность — 50 мс. Перетаскивание отменяет анимацию; `is_animating` возвращает
  `true` и во время доворота, и во время инерции, и при авто-вращении, и при
  перевороте.
- Авто-вращение: после 4 с без перетаскивания, прокрутки и клика камера за 2 с
  разгоняется до 0,006 рад/с и уменьшает `yaw` — континенты уходят на восток.
  Любое взаимодействие и `fly_to` сбрасывают отсчёт; наведение не считается.
  `idle_countdown` даёт остаток паузы для ленивой перерисовки,
  `is_idle_spinning` — факт вращения.
- Музыкальная качка: `sway = motion.yaw_sway·(1 − priority)` входит в `eye()`.
  `priority` сглаживается к 1 при drag/zoom/клике/`fly_to` с τ 80 мс (150–250 мс
  до гашения) и к 0 после отпускания. `camera_priority()` отдаётся presentation-слою,
  который ослабляет squash/ripple до 35 %.
- Перевороты: camera roll вокруг оси взгляда (`projection · rotation_z(roll) · view`),
  триггер — фронт `pulse ≥ 0,6`. Старт только при `allow_flips`, простое ≥2 с,
  отсутствии взаимодействия/`fly_to`, `priority ≤ 0,5` и pitch не у края (запас 5°).
  Длительность 0,9 с, кулдаун 12 с, направление чередуется. Взаимодействие или
  `allow_flips = false` отменяют анимацию; остаточный roll затухает с τ 80 мс.
  `is_flipping()` и `roll()` доступны рендеру и репит-правилам.
- Начальная ориентация: yaw=0, pitch=0 — перед камерой Гринвич/Африка, север сверху.

## 5. Визуал (MVP)

- UV-сфера: 64 сегмента × 32 стека, треугольники с CCW-обходом, backface culling,
  без depth buffer (сфера выпуклая).
- Текстура Земли: `assets/textures/earth.jpg` (2048×1024, равнопромежуточная
  проекция, public domain), формат `Rgba8UnormSrgb`, загружается через `image` и
  встроена в бинарник `include_bytes!`.
- Сэмплер: `Repeat` по U, `ClampToEdge` по V, линейная фильтрация, без mipmap.
- Освещение (ТЗ §5.1 п. 1–2): фиксированное направление `(0.35, 0.28, 0.90)`,
  half-lambert `d = dot(n, l)·0.5 + 0.5`, `key = d^1.6`, итог
  `albedo · (0.55 + 0.65·key)` с тёплым key light `mix(белый → (1.0, 0.92, 0.82))`.
  Ночная сторона остаётся читаемой (ambient 0.55 вместо прежних 0.25).
  После света — подъём сатурации ×1.18 вокруг luminance и экспозиция ×1.06.
- Шейдер: `src/render/globe.wgsl`.
- Fresnel-атмосфера: обод по краю сферы,
  `pow(1 - dot(normal, view), 3.2) · (0.12 + motion2.x)`, цвет
  `(0.35, 0.60, 1.0)`. `motion2.x` — flash от onset (≤0,15). Сверху лежит
  узкая кромка `pow(1 - facing, 9) · 0.2` — обод читается и там, где текстура
  уходит за горизонт сферы. Позиция камеры приходит в uniform (`camera_position`).
  Фоновый аудио-пульс удалён 2026-10-24; новое движение — по ADR-0028.
- Вершинный шейдер применяет деформацию из uniform `motion`: анизотропный
  масштаб `vec3(scale_xz, scale_y, scale_xz)`, трёхлепестковую рябь по долготе
  с затуханием к полюсам и нормаль `normalize(normal / scale)`. Входная вершина
  берётся как точка единичной сферы, поэтому меш и egui-оверлей используют
  одну деформацию.
- Подписи стран — вне текущего объёма; контуры и маркеры рисует egui-оверлей.

## 6. Контуры стран

- `render/countries.rs` принимает кольца `[lon, lat]` и `CountryStyle`, не зная о
  `Country` и `GeoService`; Application Core выбирает страну и стиль.
- Кольца сгущаются дугами не более 2° и проецируются через `project_on_sphere`.
- Контур разбивается на видимые дуги: пересечение с горизонтом вычисляется
  линейной интерполяцией по `point · eye = 1`; невидимые участки отбрасываются.
- Заливка строится только для кольца, целиком видимого на передней полусфере,
  и триангулируется методом уха; кольца площадью меньше 1 px² не заливаются.
- Заливка выбранной страны — heatmap: альфа растёт логарифмически от количества
  загруженных станций страны (`heat_intensity = log10(n)/2`, ограничена
  `[0.15, 1]`), от `COUNTRY_HEAT_MIN_ALPHA = 26` до `COUNTRY_HEAT_MAX_ALPHA = 96`;
  контур — 2 px. Количество считается по маркерам (`GlobeMarker.count`,
  регистронезависимое сравнение ISO-кода), поэтому grouping учитывается.
  Hover — контур 1.4 px с `COUNTRY_HOVER_ALPHA = 120` и заливка
  `COUNTRY_HOVER_FILL_ALPHA = 30`, пропорциональная heat, только при `count > 0`.
  Цвета берутся из акцента темы. Если страна уже выбрана, hover для неё не
  дублируется.
- Отрисовка идёт через `egui::Painter` после `paint_globe` и до маркеров; GPU-
  ресурсы не создаются.

## 7. Маркеры станций

- `render/markers.rs` не создаёт GPU-ресурсов и не зависит от доменных типов.
  Application Core валидирует координаты, группирует станции и передаёт
  `MarkerVisual` с экранной позицией и признаком эфира.
- `sphere_point(lat, lon)` переводит градусы в точку единичной сферы по общему
  соглашению (`x = cos φ · sin λ`, `y = sin φ`, `z = cos φ · cos λ`).
- `paint_markers` рисует поверх wgpu-callback глобуса через `egui::Painter`:
  свечение, точку радиусом `MARKER_RADIUS = 3 px`, белую обводку 1 px, кольцо
  наведения (отступ 3,5 px, белое) и подпись с фоном, прижатую к границам области.
- Все маркеры одного цвета (`palette.marker`) и размера: качество и рейтинг
  станции больше не кодируются цветом или радиусом (обновление ADR-0012,
  Этап 9). Наведение осветляет цвет на 28 %.
- Играющая станция получает пульсирующее кольцо того же цвета: радиус растёт
  на 3,5 px, прозрачность падает до нуля, период 1,4 с; фаза берётся из
  `InputState::time`. Бейджа количества и подписи на самом маркере нет.
- Подпись — `MarkerLabel { title, details }`: имя белым (для группы
  `· +N`), вторая строка `#C7D2E4` — место, битрейт, кодек и голоса через `·`.
  Пустые части опускаются; фон — тёмный скруглённый прямоугольник с рамкой
  акцента. Текст собирает Application Core, рендер только раскладывает его.
- Отсечение задней полусферы и viewport выполняет `project_on_sphere`.
- Поведение взаимодействия (радиус 10 px, приоритет над страной) определяет
  Application Core; см. ADR-0012.

## 8. Спектральное состояние и визуализатор

- `SpectrumTarget` — срез метрик (`centroid_hz`, `spread_hz`, `flux`, `flatness`,
  `rms_db`) и опциональных полос `bands` без зависимости от аудио-модуля.
  `BandTargets::from_db` нормирует дБFS в `0.0..=1.0` (пол −90 дБ, диапазон 80 дБ).
  Полосы читает `MotionMixer` (ADR-0028); кольцо Listening по-прежнему работает
  от метрик оверлея.
- `SpectrumOverlay` сглаживает значения экспоненциально; первый кадр принимается
  как есть, `None` запускает затухание. Состояние живёт в `OrbitApp`.
- `render/visualizer.rs` (`VisualizerModel`) — модель cava-столбцов (ADR-0022):
  атака 28 мс, линейная гравитация 1,8 ед/с, удержанные пики с падением
  0,42 ед/с, процедурная фактура на детерминированных фазах. Модель не рисует.
- Столбцы рисует `ui::widgets::viz_bars` по данным модели: поле 156×46 px,
  24 столбца по умолчанию, зазор 2 px, скругление 1 px, градиент по абсолютной
  высоте, шапки пиков 2 px и опорная линия. Включён по умолчанию; чип
  «Visualizer» только выключает его до конца сессии.
- Отрисовки в модуле нет: старый FFT-strip удалён (ADR-0019), оверлей не создаёт
  GPU-ресурсов и не участвует в wgpu-пайплайне глобуса.
- `pulse()` даёт сигнал эфира для кольца Listening: `0.75·level + 0.25·flux`,
  ограничен `[0, 1]`; при невидимом оверлее — 0. На глобус не передаётся:
  Fresnel-кромку усиливает только `flash` от onset.
- Метрики агрегированные, поэтому и огибающая визуализатора — художественная
  оценка, а не покадровый спектр по бинам (ADR-0010, ADR-0019).

## 9. Аудио-реактивное движение

- `render/motion.rs` — чистый домен без egui/wgpu: `GlobeDeform`, `GlobeMotion`
  и `MotionMixer`. Микшер живёт в presentation-слое (`OrbitApp`) на время
  показа окна и не зависит от рендерера глобуса.
- `MotionMixer::update(target, reactivity, style, dt)` обновляет пружину
  вертикального сжатия: цель `1 − amount·(params.squash·bass_env +
  0,6·params.beat·pulse·gate) + idle + drift`, жёсткость 120,
  затухание 14, полунеявный интегратор шагами не более 1/240 с; `dt`
  ограничен 0,1 с. `scale_xz = 1/sqrt(scale_y)` сохраняет объём: планета
  сжимается по вертикали и расширяется по горизонтали.
- Огибающие полос асимметричны: атака 30 мс, спад 220 мс. Без `bands`
  используют `rms_db` (−60..0 дБ) и `flux`. `None` плавно гасит аудио-вклад
  (τ = 100 мс): через 300 мс остаётся ≈5 %, через 500 мс — менее 1 % рабочей
  амплитуды.
- Idle-дыхание 0,12 Гц с амплитудой 0,35 % (Low/Normal) или 0,5 % (High)
  продолжает движение без эфира. `Off` мгновенно возвращает `IDENTITY` и
  `GlobeMotion::disabled(style)`.
- Лимиты `MotionReactivity`: Low — 1,0 % сжатия и 0,15 % ряби; Normal —
  2,0 % и 0,25 %; High — 3,5 % и 0,40 %. Эффективную реактивность выбирает
  Application Core (Overlay ≤ Low, Listening/Reduce motion — Off).
- `GlobeMotion::uniform_values` отдаёт `[scale_y−1, scale_xz−1, ripple,
  ripple_phase]` в `CameraUniform.motion`; `uniform2_values` — `[flash,
  style_id, beat_phase, time]` в `CameraUniform.motion2`. Uniform занимает
  128 байт; `style_id` — числовой id из `MotionStyle`, рендерер его не
  интерпретирует.
- Деформация применяется ко всем слоям одинаково: шейдер — к мешу, `paint_country`
  и маркеры — через `GlobeDeform::to_world`, hit-test — лучом, переведённым
  `to_unit` в пространство единичной сферы. Рябь в hit-test не входит: она
  художественная и не меняет силуэт заметно.
- Onset-детектор работает фиксированным шагом 50 мс и хранит 2,1 с истории.
  Базовая линия — медиана и σ последних ≤20 сэмплов без последних 200 мс; порог
  Bass — `median + max(0,04; 0,35σ)`, flux — `median + max(0,02; 0,45σ)`.
  Рефрактерность 150 мс; уверенность растёт на `0,30·strength` и затухает с
  τ 1,5 с, импульс — с τ 0,18 с. Интервалы ограничены 60–180 BPM, период —
  медиана последних четырёх, фаза сбрасывается на onset. Бит проходит через
  гейт уверенности 0,20/0,50, поэтому тишина и речь не дают «марша».
- Шесть стилей задают `StyleParams`: Soft Pulse (squash 1,0, рябь 0,45),
  Groove Sway (0,65; sway 0,015 рад; вклад бита 0,15), Bass Drop (1,25; flash
  0,10; бит 0,35), Ambient Drift (0,45; дрейф 1,0; sway 0,006), Sparkle Twitch
  (0,50; рябь 1,0; скорость 1,8) и Beat March (0,80; бит 0,80; sway 0,008).
  `yaw_sway` считается как `params.sway·(sin(2π·beat_phase)·gate +
  params.drift·sin(drift_phase))`, `flash` — `params.flash·pulse·gate·release`
  (≤0,15), `pulse` — затухающий импульс onset'а (τ 0,18 с). Камера применяет
  sway с поправкой на приоритет и владеет roll-анимацией переворота;
  presentation-слой кладёт фактический roll в снимок `GlobeMotion.roll`.
  `flash` уже усиливает Fresnel-кромку в шейдере.
- Auto выбирает стиль по EMA признаков (τ 2,5 с): уровень, flux, доли
  Bass/Mid/High, логарифмический centroid 200–4000 Гц и flatness. Стиль
  удерживается не менее 3 с, параметры переходят кроссфейдом 1 с по smoothstep.
  Без полос кандидаты ограничены Soft Pulse, Groove Sway и Ambient Drift;
  ручной стиль, требующий полос, деградирует до Soft Pulse, а пропажа полос
  сразу уводит текущий band-стиль в Soft Pulse.

## 10. Тесты

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
- все маркеры имеют одинаковый радиус, играющая станция получает кольцо-пульс,
  а наведение — светлое кольцо и подпись `MarkerLabel`;
- `fly_to` центрирует заданную точку и идёт кратчайшим путём через антимеридиан;
- контур страны триангулируется для выпуклого и вогнутого колец, заливается только
  при полной видимости, обрезается по горизонту и замыкается в видимой части;
- невалидные координаты кольца пропускаются;
- спектральный оверлей принимает первый кадр без сглаживания, затухает без данных,
  ограничивает частоты и игнорирует неположительный `dt`;
- `pulse()` равен нулю без данных и следует за уровнем и flux при наличии сигнала;
- uniform камеры несёт позицию `eye` и нормализованное направление света,
  матрица `view_proj` конечна;
- heatmap стран: интенсивность растёт с количеством станций и насыщается,
  группированные маркеры суммируются, сравнение ISO-кода регистронезависимо;
- `MotionMixer` в `Off` возвращает `IDENTITY`, лимиты реактивности соблюдаются,
  сжатие сохраняет объём (`scale_xz > 1`), возврат после Stop укладывается в
  300/500 мс, idle-дыхание работает без данных, `dt` ограничен;
- без полос движение использует `rms_db` и `flux`;
- onset-детектор на регулярной последовательности находит период ≈0,5 с,
  регулярность >0,85 и уверенность >0,5; рефрактерность отсекает повторные
  удары, Stop сбрасывает фазу и уверенность;
- гейт уверенности управляет фазой бита и flash, импульс бита двигает squash и
  затухает; Bass Drop вспыхивает и укладывается в лимиты, Groove Sway качает
  yaw, Sparkle Twitch держит sway нулевым;
- Auto выбирает Ambient Drift на спокойном сигнале, Sparkle Twitch на высоком
  centroid, Bass Drop на басовом и Beat March на регулярном; без полос не
  выбирает стили, требующие Bass/High;
- Auto меняет стиль после удержания 3 с и переходит кроссфейдом; ручной стиль
  без полос деградирует до Soft Pulse;
- `uniform2_values` содержит flash, `style_id`, `beat_phase` и время; uniform
  камеры остаётся 128-байтовым;
- `GlobeDeform::to_world`/`to_unit` взаимно обратны, `uniform_values` содержит
  `scale_y−1` и `scale_xz−1`;
- деформированный луч попадает в ту же точку поверхности, что и проекция
  маркера; `GlobeCallback` передаёт `motion` и `motion2` без нарушения
  POD-раскладки;
- sway виден на свободной камере, гаснет при drag и плавно возвращается;
  `with_camera_priority(1.0)` оставляет 35 % squash и ряби;
- перевороты не стартуют без `allow_flips`, без простоя ≥2 с, у края pitch и
  на свежем инпуте; длительность и кулдаун соблюдаются;
- drag отменяет переворот, остаточный roll затухает до нуля;
- roll поворачивает проекцию и hit-test согласованно: луч по повёрнутой точке
  возвращает ту же точку сферы.

## 11. Связанные ADR

- ADR-0002: Выбор GUI-стека
- ADR-0008: Геопространственный поиск
- ADR-0010: Экспериментальный FFT-визуализатор
- ADR-0012: Маркеры станций на глобусе
- ADR-0015: UX-редизайн «Orbital Desk» (контуры стран, `fly_to`)
- ADR-0028: Аудио-реактивная деформация глобуса
- ADR-0029: Орбитальное аудиокольцо
