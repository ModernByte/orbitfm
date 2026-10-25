# Модуль: Анимации интерфейса

**Путь в коде:** `src/ui/animation.rs`  
**Ответственность:** Длительности, кривые, `Presence`, stagger для переходов UI.  
**Зависимости:** egui; из настроек — только флаг `reduce_motion`.

---

## 1. Назначение

Единый ритм появления, исчезновения и hover-переходов (ADR-0032). Модуль не
знает про session, аудио и render: раскладка, hit-testing и состояние ему не
принадлежат. Аудио-движение глобуса и кольца остаётся в ADR-0028/ADR-0029.

## 2. Константы

| Переход | Длительность |
|---------|--------------|
| Drawer (`DRAWER`) | 200 мс |
| Scrim (`SCRIM`) | 160 мс |
| Вход сцены (`SCENE_ENTER`) | 220 мс |
| Панель (`PANEL`) | 220 мс |
| Help (`HELP`) | 170 мс |
| Legend (`LEGEND`) | 150 мс |
| Callout (`CALLOUT`) | 180 мс |
| Вкладка (`TAB`) | 120 мс |
| Toast in / out (`TOAST_IN` / `TOAST_OUT`) | 180 / 200 мс |
| Капсулы Overlay и Listening (`CAPSULE`) | 180 мс |
| Hover строки списка (`ROW_HOVER`) | 80 мс |
| Press (`PRESS`) | 120 мс |
| Шаг stagger (`STAGGER_STEP_MS`, приватная) | 18 мс |

Геометрия смещений: `SLIDE_X = 8` px, `RISE_Y = 6` px, `PRESS_SCALE = 0.96`.
Потолок overshoot-шкалы держит `widgets::POP_LIMIT = 1.15`, вершину кривой —
приватная `OVERSHOOT`.

Появление — ease-out cubic, исчезновение — ease-in cubic. `Curve::Overshoot`
(перелёт выше 1.0 в середине) разрешён только капсульным кнопкам.

## 3. API

```rust
/// Длительности и смещения — константы модуля (§2), а не настройки пользователя.
pub(crate) enum Curve { EaseOut, EaseIn, Overshoot }
pub(crate) fn eased(t: f32, curve: Curve) -> f32;
/// Задержка строки в общем прогрессе: 0 для первой, +18 мс на каждую следующую.
pub(crate) fn stagger(index: usize, progress: f32) -> f32;
/// Смещение панели по X: `SLIDE_X` пикселей в закрытом состоянии, ноль в открытом.
pub(crate) fn slide_x(progress: f32) -> f32;
/// Подъём help/legend/callout: `RISE_Y` пикселей снизу в закрытом состоянии.
pub(crate) fn rise_y(progress: f32) -> f32;
/// Прозрачность по прогрессу: невидимая зона не рисует каркас.
pub(crate) fn alpha(progress: f32) -> f32;

/// Стабильный переход по Id: ctx.animate_bool_with_time + reduce motion.
pub(crate) struct Presence { ... }
impl Presence {
    pub(crate) fn new(id: egui::Id) -> Self;
    pub(crate) fn value(&mut self, ctx: &egui::Context, open: bool, duration: f32, reduce: bool) -> f32;
}

/// Переходы одного прогона окна; живёт в `OrbitApp`, создаётся заново вместе с ним.
pub(crate) struct Transitions { ... }
impl Transitions {
    pub(crate) fn new() -> Self;
    /// Считывает прогрессы кадра одним вызовом, включая закрываемые зоны.
    pub(crate) fn frame(&mut self, ctx: &egui::Context, flags: ZoneFlags, reduce: bool) -> FrameMotion<'_>;
}

/// Видимость зон на кадр.
pub(crate) struct ZoneFlags {
    pub scene: bool, pub panel: bool, pub help: bool, pub legend: bool,
    pub drawer: bool, pub capsules: bool, pub favorites: bool, pub listening: bool,
}

/// Готовые прогрессы кадра: зоны только читают значения.
pub(crate) struct FrameMotion<'a> {
    pub reduce: bool,
    pub scene: f32, pub panel: f32, pub help: f32, pub legend: f32,
    pub drawer: f32, pub scrim: f32,
    pub capsules: f32, pub favorites: f32, pub listening: f32,
    pub keyed: &'a mut PresenceStore,
}

/// Именованные переходы (вкладки, press и цвет play/stop в капсулах):
/// до `KEYED_LIMIT = 12`, сверх потолка самые старые вытесняются FIFO —
/// вытеснение не ломает анимацию, но перезапускает вход ключа.
pub(crate) struct PresenceStore { ... }
impl PresenceStore {
    pub(crate) fn value(&mut self, ctx: &egui::Context, id: egui::Id, open: bool, duration: f32, reduce: bool) -> f32;
}
```

`Transitions` содержит по одному `Presence` на зону (`scene`, `panel`, `help`,
`legend`, `drawer`, `scrim`, `capsules`, `favorites`, `listening`) плюс
`PresenceStore` для динамических ключей. Ключей должно быть мало: длинный список
строк анимируется общим прогрессом через `stagger`, а не по записи на строку.
`scrim` опрашивается по флагу `drawer`, но со своей длительностью.

Прогресс кадра считает `OrbitApp::draw_layers` один раз и передаёт в зоны
`&mut FrameMotion` — переход по одному `Id` не может опрашиваться дважды за
кадр, иначе `Presence` рассинхронизируется. `Transitions` принадлежит окну, а
`OrbitApp` создаётся заново при каждом показе и при пересоздании viewport
(ADR-0018, ADR-0021), поэтому вход проигрывается сам: отдельного сброса в
модуле нет. Закрытые зоны тоже опрашиваются — иначе исчезновение не анимируется.

Собственный `Presence` держит только то состояние, которое живёт вне кадра
самостоятельно, — стек тостов в `Toast`; номер слоя берётся из пула и входит в
`Id`, потому что `egui::Id::unique` детерминирован и одинаковый текст дал бы
слоям один переход. Подложка строк списка станций (`station_row::row_motion`)
намеренно обходится без `PresenceStore`: она зовёт
`ctx.animate_bool_with_time` напрямую и при `Style::animation_time == 0` сразу
принимает конечное состояние — десяткам строк не хватает ключей под потолок.

`egui::AnimationManager` возвращает целевое значение на первом обращении к `Id`,
поэтому `Presence` сначала пропускает bootstrap-кадр (`seen`): первый кадр
сидирует закрытое состояние, просит `request_repaint` и отдаёт ноль, дальше
идёт `eased(progress, EaseOut)` на вход и `EaseIn` на уход.

`eased` клампит вход и возвращает точные 0/1 на концах. Неконечный прогресс
(`NaN`, `±INF`) во всех функциях ядра даёт ноль — fail-closed: в интерфейс не
может протечь NaN-геометрия или частично видимая зона. `stagger` отсчитывает
задержку от окна входа сцены (18 мс из 220 мс ≈ 8,2 % прогресса) и применяется к
прогрессу конкретной зоны: на переходах короче 220 мс шаг пропорционально меньше,
зато ряд всегда укладывается в своё окно. Задержка клампится у края окна
(`STAGGER_MAX_DELAY = 0.9`), чтобы последние строки не оставались невидимыми.

## 4. Reduce motion

`settings.reduce_motion` — мастер-выключатель:

- `Presence::value` и `PresenceStore::value` возвращают конечное состояние, не
  опрашивая `AnimationManager`; нулевая или неконечная длительность ведёт себя
  так же;
- `palette::configure_style` через `all_styles_mut` выставляет
  `Style::animation_time` в 0 для всех стилей egui, поэтому штатные контролы и
  подложка строк списка тоже мгновенно принимают конечное состояние;
- `widgets::pop_scale(·, reduce)` сразу даёт 1.0, а `capsule_button` на reduce
  вообще не опрашивает press-переход и не занимает слот `PresenceStore`;
- секции drawer'а входят без stagger-задержек;
- радиус обода Listening не дышит (`pulse` остаётся только в прозрачности);
- аудио-сцена продолжает получать данные, но геометрия не двигается.

## 5. Кадры

Кадры запрашиваются только на время перехода: их просит сам egui, пока значение
`animate_bool_with_time` между 0 и 1, плюс один `request_repaint` на bootstrap-кадре
входа и `request_repaint_after` у тостов по таймеру жизни. Постоянные циклы
остались прежними и не выросли: пульс Live-бейджа и полос эквалайзера —
`LIVE_REPAINT` 120 мс, аудио-движение глобуса — 33 мс в эфире и 66 мс в idle-дыхании
(ADR-0028), кадры эфира для пульса Listening и визуализатора — 33 мс.
UI-анимации не поднимают idle-нагрузку и не перерисовывают скрытое окно:
«вечных» переходов в модуле нет.

## 6. Сценарии

Focus: вход HUD → panel/help → transport одним `stagger` на индексах 0…3 за
прогон окна; панель — fade плюс slide 8 px; help — fade с 6 px rise и stagger по
строкам, legend — fade с 6 px rise; callout страны живёт своим fade-состоянием в
`globe_view` (180 мс, 6 px); вкладки Results/Favorites/History — fade и slide
120 мс по трём ключам `PresenceStore` (все три опрашиваются за кадр ровно один
раз, уходящая вкладка догорает до нуля, выбранная кликом начинает с нуля),
индикатор подчёркивания идёт прогрессом активной вкладки; две вкладки
одновременно не рендерятся; строки списка — hover fade 80 мс и press scale 0.96.

Overlay: каркас и содержимое капсул входят 180 мс, кнопка play/stop появляется с
overshoot (`pop_scale`), строки избранного — общим stagger, цвет диска
перетекает по `CAPSULE`, press — по `PRESS`.

Listening: вход строки 180 мс; пульс обода остаётся на кадрах эфира, при reduce
motion радиус статичен.

Drawer: scrim 160 мс, панель slide+fade 200 мс, секции входят stagger'ом и на
выходе гаснут в обратном порядке. На выходе `DeskLayout` уже не выделяет зону,
поэтому каркас дорисовывается по последнему прямоугольнику drawer'а, а ввод на
время выхода отключается. Повторное открытие не мигает.

Toast: стек до трёх записей (переполнение вытесняет самую старую), жизнь 3 с,
вход fade плюс rise 6 px за 180 мс, уход 200 мс, дубликат обновляет существующую
запись вместо нового слоя. Якорь зависит от плотности и считает его
`DeskLayout::toast_offset`: стек встаёт над занятой нижней зоной — над transport
в Focus, над избранным в Overlay, в Listening остаётся только поле окна. Сам
стек поднимается вверх на высоты уже отрисованных записей.

Смена WindowMode: только вход нового режима, без кроссфейда (viewport
пересоздаётся, ADR-0021).

## 7. Тесты

Ядро (`animation`):

- `eased` на концах и монотонность кубических кривых, overshoot не выше 1.15;
- `stagger` для первой строки, потолок задержки и `1.0` для всех при полном
  прогрессе;
- reduce motion и нулевая/неконечная длительность → мгновенное конечное состояние;
- вход `Presence` с bootstrap-кадром: первый кадр 0, дальше монотонный рост до 1;
- завершённый переход не просит кадры;
- новый `Presence` с тем же `Id` переигрывает вход (окно пересоздаётся вместе с
  `Transitions`).

Зоны: `widgets::pop_scale`/`pressed_scale` (перелёт с потолком, конечный размер
на reduce motion), `settings_panel::section_alpha` (порядок блоков и обратный
порядок на выходе), `capsule::favorite_rows_share_one_stagger` и
`play_color_carries_its_state_between_frames`, `station_row::press_scales_the_plate_about_its_center`,
`station_panel::tab_roundtrip_index_and_back`, `hud::garbage_progress_never_leaks_nan_alpha`,
`transport::content_fade_follows_the_shared_scene_stagger`, тесты `toast`
(дубликат, потолок стека, вход/выход по кадрам, освобождение слота, якоря стека).
