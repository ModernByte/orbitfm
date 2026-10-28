# Модуль: Панель настроек (drawer)

**Путь в коде:** `src/ui/settings_panel.rs`, примитивы строк — `src/ui/controls.rs`  
**Ответственность:** Содержимое правой панели настроек; scrim и модалка — в `app.rs`.  
**Зависимости:** egui, `ui::animation`, `ui::controls`, `ui::palette`, `ui::widgets`, `ui::icons`, `i18n`, `settings`, `session::WindowMode`.

---

## 1. Назначение

Единое место изменения всех пользовательских настроек (ADR-0031). Drawer
пришёл на смену popup из шестерёнки HUD: один скролл, секции, пояснения,
полноширинные контролы и подписи из каталога уже разрешённого языка (смена
языка видна в том же кадре). Модуль только рисует содержимое и возвращает
`Vec<UiAction>`; сохранение и прочие побочные эффекты выполняют `OrbitApp` и
`Session`. Каркас стекла, scrim и приход/уход считает `app.rs` (`draw_settings`)
по прогрессам `animation::FrameMotion`.

## 2. Состояние

| Владелец | Поле | Смысл |
|----------|------|-------|
| `Session` | `settings_open: bool` | drawer открыт |
| `Session` | `pending_volume: Option<f64>` | громкость незавершённого перетаскивания |
| `OrbitApp` | `drawer_rect: Option<Rect>` | прямоугольник с последнего открытого кадра |
| `OrbitApp` | `transitions: Transitions` | прогрессы `drawer` и `scrim` кадра |
| `OrbitApp` | `Toast` | сообщения об ошибках сохранения |
| `DeskLayout` | `settings: Option<Rect>` | зона панели в текущем кадре |

Зона считается в `DeskLayout::compute` (`settings_rect`): ширина
`min(SETTINGS_WIDTH = 380, viewport.width() − 2·MARGIN)`, правая колонна на всю
высоту между полями. Она входит в `blocks_point`, поэтому глобус не получает
клики и drag, начатые над ней. В `Listening` зоны нет: `Session::open_settings`
в этом режиме сначала переводит окно в `Overlay`.

Содержимое входит одним `stagger` по прогрессу drawer'а: заголовок и шесть
блоков секций (`BLOCK_HEADER`…`BLOCK_ABOUT`). Гаснут только краски: у блоков,
нарисованных палитрой, приглушается сам `Palette` (`widgets::faded_palette` —
`dim` передаётся в `controls` и `widgets`), у нативных контролов egui —
`Visuals` (`dimmed` + `fade_visuals`). Второй механизм поверх первого не
накладывается: `multiply_opacity` и вложенный `Ui` ради альфы в контролах
запрещены (ADR-0032 §7). Раскладка, идентификаторы виджетов и hit-зоны
остаются неподвижными.

### 2.1. Примитивы строк (`src/ui/controls.rs`)

- `dropdown(ui, palette, id_salt, width, options, selected, label_of)` —
  перечисление на `egui::ComboBox::from_id_salt`: чип с фоном и обводкой
  палитры, собственный шеврон (`Shape::line`, accent на hover и на открытом
  списке), `truncate` и подпись текущего значения из каталога. Возвращает
  `Some(index)` только при реальном выборе, поэтому строка не генерирует
  `UiAction` на каждом кадре. `id_salt` передаётся из вызова — авто-id egui
  зависят от позиции в кадре, а смена языка меняет подписи.
- `checkbox(ui, palette, checked, enabled, label)` — квадрат 18×18 с
  скруглением 5, галочка двумя штрихами, контрастным цветом на accent-фоне;
  hit-зона включает подпись, `Sense::hover` у выключенного, `WidgetInfo::selected`
  с ролью `CheckBox` и focus-ring.
- `row(ui, palette, label, hint, slot, control)` — строка настройки: галлея
  подписи с переносом по ширине `ui.available_width() − slot.0 − gap`, контрол в
  фиксированном слоте справа (`UiBuilder::max_rect` +
  `Layout::centered_and_justified`), приглушённая подсказка под подписью. Слот
  задаётся вызывающим: `DROPDOWN_SLOT` 168×26, `VOLUME_SLOT` 150×18,
  `SWATCH_SLOT` 220×22. Высота строки — `max(ROW_HEIGHT, высота галлеи)`, так что
  перенос подписи не сдвигает контрол по горизонтали.

Контролы рисуются уже приглушённой палитрой и не делают вложенный `Ui` ради
эффектов; переиспользуемые чипы (`ComboBox`) получают `chip_visuals` — фон,
обводку и accent из нашей палитры.

## 3. Секции

- **Appearance** — тема (`dropdown` по `ThemeMode::ALL`), акцент (шесть
  плашек-свотчей 28×22 в `SWATCH_SLOT` с подсказкой), `checkbox` визуализатора
  (`UiAction::ToggleSpectrum`), число столбцов (`dropdown` из `VIZ_BAR_CHOICES`,
  недоступен при выключенном визуализаторе).
- **Language** — `dropdown` по `Language::ALL`: `System` и 10 языков с нативными
  именами, пояснение `language_hint`.
- **Audio** — громкость (`widgets::volume_slider_width` в `VOLUME_SLOT` 150×18) и
  пояснение `audio_hint`; у самого слайдера подсказка `volume_hint` — про колесо.
- **Motion & accessibility** — реактивность (`dropdown` по `MotionReactivity::ALL`)
  с подсказкой `motion_spin_hint`, характер движения (`dropdown` по
  `MotionStyle::ALL`), reduce motion и flips — `checkbox`; под чекбоксами
  остаётся текстовая подсказка, а наведение на flips показывает `flips_unavailable`.
- **Window** — `dropdown` по `WindowMode::ALL` с `window_hint`, `checkbox`
  карточки управления, `Hide`, `Quit`.
- **About** — версия, лицензия, app-id, пояснение про фоновую работу и трей,
  подсказка про сохранение.

Подписи секций и подсказки — из `SettingsText` каталога, значения enum'ов — из
`i18n::labels` (`theme_label`, `accent_label`, `motion_label`, `style_label`,
`language_label`); собственных `label()` у `settings`-enum'ов нет. Режимы окна
подписывает локальный `mode_label`: `WindowMode` — тип session, а i18n от него не
зависит.

Неактивные контролы остаются видимыми и объясняют причину: flips выключаются при
`motion == Off` или `reduce_motion`, и вместо общей подсказки показывается
`flips_unavailable`. Выбор значения не закрывает drawer.

## 4. Открытие и закрытие

| Источник | Поведение |
|----------|-----------|
| Шестерёнка HUD (Focus) | toggle drawer; видима и в узком окне |
| Шестерёнка верхней капсулы (Overlay) | toggle drawer |
| Шестерёнка шапки панели (Panel) | toggle drawer; в `Panel` открытый drawer занимает стеклянную поверхность целиком, поэтому шапка и список уходят (ADR-0033) |
| Ctrl+, (`modifiers.command` + `Comma`) | toggle drawer в Focus и Overlay |
| Ctrl+, в Listening | `open_settings` переводит окно в Overlay и открывает drawer |
| Трей «Settings…» | `open_settings`: показывает окно, при Listening — Overlay |
| Scrim, крестик заголовка, Escape | `CloseSettings` |
| Hide, WM-close, park | закрытие вьюпорта закрывает drawer |
| Смена WindowMode | drawer переживает пересоздание viewport |
| Выбор Listening | `SetWindowMode(Listening)` закрывает drawer |

`run_session` после возврата `run_window` вызывает `end_window()`, затем
проверяет `take_reopen()` и только после него `close_settings()`. Поэтому
пересоздание viewport из-за смены режима сохраняет drawer, а любое другое
закрытие — нет. В `OrbitApp::logic` Escape снимает самый верхний модальный слой:
сначала drawer, затем help.

## 5. Громкость

`widgets::volume_slider_width` возвращает
`SliderResponse { changed, committed }`, где `committed` — отпускание drag либо
изменение без продолжающегося drag (клик по треку, клавиша, щелчок колеса). Зоны —
drawer, transport, верхняя капсула Overlay и строка режимов шапки выпадающей панели —
переводят это в два действия:

- `changed && !committed` → `UiAction::PreviewVolume` → `Session::preview_volume`:
  только `audio.set_volume` и `pending_volume`, файл не трогается;
- `committed` → `UiAction::CommitVolume` → `Session::commit_volume`: звук плюс
  запись в `SettingsStore`, pending сбрасывается.

Клавиатурный шаг тоже коммитит: стрелки ±5 %, Home/End — край. Колесо мыши —
третий дискретный ввод того же `widgets::volume_slider_width`, поэтому оно
достается всем четырём зонам без дублирования кода: шаг ±5 % на щелчок (`VOLUME_KEY_STEP`),
коммит сразу, `pending_volume` не открывается. Три следствия реализованы в виджете:
обработка только под указателем (`response.hovered()`) — иначе слайдер воровал бы
скролл у списка станций и зум глобуса; шаг считается по сырым `input.events` с
пересчётом `MouseWheelUnit::{Point, Line, Page}` в пиксели, потому что egui
растягивает один щелчок на несколько кадров (`smooth_scroll_delta` = 40 px плюс
угасающий хвост), и каждый хвост дал бы лишнюю запись в `settings.json`;
сглаженная дельта обнуляется, пока указатель над слайдером, чтобы остаток щелчка
не доехал `ScrollArea` drawer'а. Повтор тех же входных данных в многопроходной
отрисовке отсекается по `input.time` (`insert_temp` на `response.id`), иначе один
щелчок применился бы дважды. Подсказка при наведении — `settings.volume_hint`.

Дискретные команды (IPC `Volume`, трей VolumeUp/Down, MPRIS) идут через `Session::set_volume`
и пишут файл сразу. Прерванный drag дописывает в файл `end_window()` через
`flush_pending_volume()` — этот путь покрывает `Hide`, WM-close и `Quit`, так как
все они заканчиваются закрытием вьюпорта. Значение клампится в `0.0..=1.0`,
non-finite заменяется на `DEFAULT_VOLUME`.

## 6. Ввод

Scrim — `egui::Area` на весь viewport в `Order::Foreground`, затемнение
`SCRIM_ALPHA = 90` (чёрное в тёмной теме, белое в светлой); клик по нему даёт
`CloseSettings`. Drawer — область того же порядка, аллоцирована позже и забирает
ввод у скрима; тосты аллоцируются ещё позже и остаются выше. Пока drawer открыт,
`globe_view` считает кадр модальным (`layout.settings.is_some()`): вращение, зум
и клик по сфере не обрабатываются, а вне модалки работает фильтр по
`press_origin` — жест, начатый под панелью, глобус не двигает.

На уходе `DeskLayout` уже не отдаёт зону, поэтому каркас и содержимое дорисовываются
по сохранённому `drawer_rect`: скрим получает `Sense::hover`, drawer —
`interactable(false)`. Гаснущие слои рисуются, но ввод не принимают. Скрытия окна
по Escape нет.

## 7. Тесты

- `layout`: `open_settings_drawer_blocks_its_column`,
  `closed_settings_drawer_does_not_block_anything`,
  `drawer_clamps_width_on_a_narrow_viewport`, `listening_has_no_drawer_zone`;
- `controls`: `checked_state_does_not_move_the_checkbox` (галочка не меняет ни
  аллокацию, ни hit-зону), `long_label_wraps_instead_of_pushing_the_control`
  (ширина слота фиксирована, правый край совпадает у короткой и длинной подписи);
- `settings_panel`: reduce motion даёт конечную альфу без перехода, блоки идут
  по порядку на входе и в обратном порядке на выходе, альфа не выходит из
  `0..=1` включая NaN;
- `widgets`: `wheel_over_slider_steps_volume_and_commits`,
  `wheel_is_deduplicated_within_one_input_time`,
  `wheel_outside_slider_leaves_volume_and_scroll_alone`,
  `wheel_clamps_at_volume_edges_without_commit` — headless-прогоны `ctx.run_ui`
  с `Event::PointerMoved` + `Event::MouseWheel { unit: Line }`; отдельный кадр
  наведения нужен потому, что `hovered()` читает состояние прошлого прохода;
- `session::window`: `mode_switch_of_an_open_window_reopens_it` — reopen
  отличается от закрытия, только поэтому drawer при смене режима сохраняется;
- `i18n::labels`: у каждого значения `ThemeMode`, `AccentColor`,
  `MotionReactivity`, `MotionStyle` есть непустая подпись;
- `settings`: `roundtrip_persists_language_volume_and_visualizer`,
  `invalid_volume_is_sanitized_on_load_and_write`,
  `unknown_language_code_falls_back_without_touching_other_fields`.
