# Модуль: Анимации интерфейса

**Путь в коде:** `src/ui/animation.rs`  
**Ответственность:** Длительности, кривые, `Presence`, stagger для переходов UI.  
**Зависимости:** egui, `settings::MotionReactivity`/`reduce_motion`.

---

## 1. Назначение

Единый ритм появления, исчезновения и hover-переходов (ADR-0032). Модуль
не знает про session, аудио и render; аудио-движение глобуса и кольца остаётся
в ADR-0028/ADR-0029.

## 2. Константы

| Переход | Длительность |
|---------|--------------|
| Drawer | 200 мс |
| Scrim | 160 мс |
| Вход сцены | 220 мс |
| Панель | 220 мс |
| Help | 170 мс |
| Legend | 150 мс |
| Callout | 180 мс |
| Вкладка | 120 мс |
| Toast in / out | 180 / 200 мс |
| Stagger строк | 18 мс |

Появление — ease-out cubic, исчезновение — ease-in cubic. Overshoot только у
капсульных кнопок.

## 3. API

```rust
pub(crate) enum Curve { EaseOut, EaseIn, Overshoot }
pub(crate) fn eased(t: f32, curve: Curve) -> f32;
/// Задержка строки в общем прогрессе: 0 для первой, +18 мс на каждую следующую.
pub(crate) fn stagger(index: usize, progress: f32) -> f32;

/// Стабильный переход по Id: ctx.animate_bool_with_time + reduce motion.
pub(crate) struct Presence { ... }
impl Presence {
    pub(crate) fn new(id: egui::Id) -> Self;
    pub(crate) fn value(&mut self, ctx: &egui::Context, open: bool, duration: f32, reduce: bool) -> f32;
}
```

`eased` клампит вход и возвращает точные 0/1 на концах. `stagger` не превышает
1 и не создаёт задержку за пределами перехода.

## 4. Reduce motion

`settings.reduce_motion` — мастер-выключатель: `Presence` сразу возвращает
конечное состояние, длительности равны нулю, пульс Listening статичен,
`Style::animation_time` выставлен в 0 для всех стилей egui. Аудио-сцена
продолжает получать данные, но геометрия не двигается.

## 5. Кадры

Кадры запрашиваются только на время перехода. Постоянные циклы остаются
прежними: Live/Listening пульс и аудио-движение 15–30 fps. UI-анимации не
поднимают idle-нагрузку и не перерисовывают скрытое окно.

## 6. Сценарии

Focus: вход HUD → panel/help → transport один раз за прогон окна; panel fade +
8 px slide; help/legend/callout fade + 6 px rise; вкладки fade/slide 120 мс;
строки hover fade 80 мс и press scale 0.96.

Overlay: вход капсул 180 мс, stagger избранного, плавная смена цвета play/stop.

Listening: вход строки; пульс обода 20 fps, при reduce motion — статичное кольцо.

Drawer: scrim 160 мс, панель slide+fade 200 мс, закрытие в обратном порядке.

Toast: стек до трёх, вход fade+slide 180 мс, выход 200 мс, дубликат обновляет
запись, якорь зависит от плотности окна.

Смена WindowMode: только вход нового режима, без кроссфейда (viewport
пересоздаётся, ADR-0021).

## 7. Тесты

- `eased` на концах и монотонность;
- `stagger` для первой/последней строки;
- reduce motion → мгновенное конечное состояние;
- отсутствие запроса кадров после завершения перехода.
