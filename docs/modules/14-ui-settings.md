# Модуль: Модальное окно настроек

**Путь в коде:** `src/ui/settings_panel.rs`, примитивы строк — `src/ui/controls.rs`  
**Ответственность:** Содержимое модального окна настроек: шапка, категории, карточки и подвал; scrim и каркас стекла — в `app.rs`.  
**Зависимости:** egui, `ui::animation`, `ui::controls`, `ui::palette`, `ui::widgets`, `ui::icons`, `i18n`, `settings`, `session::WindowMode`.

---

## 1. Назначение

Единое место изменения всех пользовательских настроек (ADR-0031, ред.
2026-10-28). Модальное окно пришло на смену popup из шестерёнки HUD и drawer'у:
слева — категории, справа — карточки строк с пояснениями, подписи берутся из
каталога уже разрешённого языка, поэтому смена языка видна в том же кадре.
Модуль только рисует содержимое и возвращает `Vec<UiAction>`; сохранение и
прочие побочные эффекты выполняют `OrbitApp` и `Session`. Каркас стекла, scrim,
тень и приход/уход считает `app.rs` (`draw_settings`) по прогрессам
`animation::FrameMotion`.

## 2. Состояние и геометрия

| Владелец | Поле | Смысл |
|----------|------|-------|
| `Session` | `settings_open: bool` | окно настроек открыто |
| `Session` | `pending_volume: Option<f64>` | громкость незавершённого перетаскивания |
| `OrbitApp` | `settings_rect: Option<Rect>` | прямоугольник с последнего открытого кадра |
| `OrbitApp` | `settings_category: SettingsCategory` | открытая категория, состояние представления |
| `OrbitApp` | `transitions: Transitions` | прогрессы `settings` и `scrim` кадра |
| `OrbitApp` | `Toast` | сообщения об ошибках сохранения |
| `DeskLayout` | `settings: Option<Rect>` | зона модалки в текущем кадре |

`DeskLayout::compute` считает окно в `settings_rect`: размер
`SETTINGS_MODAL = 760×544`, клампится до `viewport − 2·MARGIN`, центр — центр
вьюпорта. Окно входит в `blocks_point`, поэтому глобус не получает клики и
drag, начатые над ним. В `Listening` зоны нет: `Session::open_settings` в этом
режиме сначала переводит окно в `Overlay`. В `Panel` сцена (шапка, список,
подвал) остаётся под scrim'ом — настройки больше не занимают стеклянную
поверхность целиком.

Внутри окна:

| Константа | Значение | Смысл |
|-----------|----------|-------|
| `HEADER_HEIGHT` | 58 | шапка со знаком, заголовком, `Esc` и крестиком |
| `NAV_WIDTH` / `NAV_RAIL` | 188 / 64 | колонка подписей или рейка иконок |
| `NAV_BREAKPOINT` | 640 | ниже этой ширины окна категории сжимаются до иконок |
| `BODY_MARGIN` | 16 | поля тела и подвала |
| подвал | 32 | закреплённая строка `saved_hint` |

Содержимое входит одним `stagger` по прогрессу окна: шапка, навигация,
заголовок категории, затем две карточки (`BLOCK_HEADER`, `BLOCK_NAV`,
`BLOCK_TITLE`, `BLOCK_CARD`, `BLOCK_CARD + 1`). Гаснут только краски: у зон,
нарисованных палитрой, приглушается сам `Palette`
(`widgets::faded_palette` — `dim` передаётся в `controls` и `widgets`), у
нативных контролов egui — `Visuals` (`dimmed` + `fade_visuals`). Второй
механизм поверх первого не накладывается: `multiply_opacity` и вложенный `Ui`
ради альфы в контролах запрещены (ADR-0032 §7). Раскладка, идентификаторы
виджетов и hit-зоны остаются неподвижными.

### 2.1. Примитивы строк (`src/ui/controls.rs`)

- `dropdown(ui, palette, id_salt, width, options, selected, label_of)` —
  перечисление на `egui::ComboBox::from_id_salt`: чип с фоном и обводкой
  палитры, собственный шеврон (`Shape::line`, accent на hover и на открытом
  списке), `truncate` и подпись текущего значения из каталога. Возвращает
  `Some(index)` только при реальном выборе, поэтому строка не генерирует
  `UiAction` на каждом кадре. `id_salt` передаётся из вызова — авто-id egui
  зависят от позиции в кадре, а смена языка меняет подписи.
- `switch_row(ui, palette, label, hint, on, enabled)` — современный тумблер:
  подпись и подсказка слева, трек 40×22 справа, кликабельна вся строка.
  Ручка 16 px едет по треку через `ctx.animate_bool_with_time`; анимируется
  только отрисовка, `Sense` и размеры не меняются (ADR-0032 §7). Роль —
  `CheckBox`, `enabled = false` снимает клики и гасит цвета.
- `accent_swatch(ui, palette, color, selected, hint)` — круглый свотч 26 px;
  выбранный обведён кольцом и контрастной галочкой, роль `RadioButton`,
  подсказка — имя акцента.
- `row(ui, palette, label, hint, slot, control)` — строка настройки: галлея
  подписи с переносом по ширине `ui.available_width() − slot.0 − gap`, контрол в
  фиксированном слоте справа (`UiBuilder::max_rect` +
  `Layout::centered_and_justified`), приглушённая подсказка под подписью. Слот
  задаётся вызывающим: `DROPDOWN_SLOT` 220×30, `SWATCH_SLOT` 184×26. Высота
  строки — `max(ROW_HEIGHT, высота галлеи)`, так что перенос подписи не сдвигает
  контрол по горизонтали. Если ширина меньше `slot.0 + 132 px + gap`, строка
  складывается в вертикаль: подпись и подсказка на всю ширину, контрол под ними
  на фактическую ширину строки (`Layout::left_to_right(Align::Center)`).
  `dropdown` получает `ui.available_width()` слота, поэтому в Panel 400 px
  выпадающий список занимает всю карточку и не наезжает на текст.

Контролы рисуются уже приглушённой палитрой и не делают вложенный `Ui` ради
эффектов; переиспользуемые чипы (`ComboBox`) получают `chip_visuals` — фон,
обводку и accent из нашей палитры.

## 3. Категории

Категории задаёт `SettingsCategory { Appearance, Language, Audio, Motion, Window,
About }`; порядок навигации — `ALL`, иконки — `Moon`, `Globe`, `Volume`,
`Sparkles`, `AppWindow`, `Help`. Выбранная категория залита accent-подложкой,
роль — `Tab`; в рейке подпись заменяется подсказкой при наведении. У каждой
страницы есть заголовок 18 px и короткий подзаголовок 11.5 px из
`SettingsText`: подзаголовок не повторяет подсказку отдельной строки.

- **Appearance** — тема (`dropdown` по `ThemeMode::ALL`), акцент (шесть круглых
  свотчей в `SWATCH_SLOT`), `switch_row` визуализатора
  (`UiAction::ToggleSpectrum`), число полос (`dropdown` из `VIZ_BAR_CHOICES`,
  недоступен при выключенном визуализаторе), `switch_row` «Разрешить
  HTTP-обложки» (`UiAction::SetArtworkAllowHttp`, подсказка о подмене по пути,
  ADR-0025 ред. 3).
- **Language** — `dropdown` по `Language::ALL`: `System` и 10 языков с нативными
  именами, пояснение `language_hint`.
- **Audio** — громкость на всю ширину карточки (`widgets::volume_slider_width`)
  с процентом справа и пояснениями `audio_hint` и `volume_hint`.
- **Motion & accessibility** — реактивность (`dropdown` по
  `MotionReactivity::ALL`) с подсказкой `motion_spin_hint`, характер движения
  (`dropdown` по `MotionStyle::ALL`), reduce motion и flips — `switch_row`;
  под тумблерами остаётся текстовая подсказка, а на flips при недоступности
  показывается `flips_unavailable`.
- **Window** — `dropdown` по `WindowMode::ALL` с `window_hint`, `switch_row`
  карточки управления.
- **About** — версия, лицензия, app-id, пояснение про фоновую работу, затем
  единственная кнопка окна — «Выход» (`widgets::quiet_button` в цвете ошибки,
  `UiAction::Quit`).

Подвал окна (`HEADER_MARGIN` слева и справа, высота 32) отделён линией и
показывает `saved_hint` по центру: «Настройки сохраняются автоматически».
Скроллится только контент категории (`ScrollArea::vertical`), шапка, навигация
и подвал неподвижны.

Подписи категорий и подсказки — из `SettingsText` каталога, значения enum'ов — из
`i18n::labels` (`theme_label`, `accent_label`, `motion_label`, `style_label`,
`language_label`); собственных `label()` у `settings`-enum'ов нет. Режимы окна
подписывает локальный `mode_label`: `WindowMode` — тип session, а i18n от него не
зависит.

Неактивные контролы остаются видимыми и объясняют причину: flips выключаются при
`motion == Off` или `reduce_motion`, и вместо общей подсказки показывается
`flips_unavailable`. Выбор значения не закрывает окно.

## 4. Открытие и закрытие

| Источник | Поведение |
|----------|-----------|
| Шестерёнка HUD (Focus) | toggle окна настроек; видима и в узком окне |
| Шестерёнка верхней капсулы (Overlay) | toggle окна настроек |
| Шестерёнка шапки панели (Panel) | toggle окна настроек; сцена остаётся под scrim'ом (ADR-0033) |
| Ctrl+, (`modifiers.command` + `Comma`) | toggle в Focus и Overlay |
| Ctrl+, в Listening | `open_settings` переводит окно в Overlay и открывает настройки |
| Трей «Settings…» | `open_settings`: показывает окно, при Listening — Overlay |
| Scrim, крестик шапки, Escape | `CloseSettings` |
| Hide, WM-close, park | закрытие вьюпорта закрывает настройки |
| Смена WindowMode | настройки переживают пересоздание viewport |
| Выбор Listening | `SetWindowMode(Listening)` закрывает настройки |

`run_session` после возврата `run_window` вызывает `end_window()`, затем
проверяет `take_reopen()` и только после него `close_settings()`. Поэтому
пересоздание viewport из-за смены режима сохраняет окно открытым, а любое другое
закрытие — нет. В `OrbitApp::logic` Escape снимает самый верхний модальный слой:
сначала настройки, затем help.

## 5. Громкость

`widgets::volume_slider_width` возвращает
`SliderResponse { changed, committed }`, где `committed` — отпускание drag либо
изменение без продолжающегося drag (клик по треку, клавиша, щелчок колеса). Зоны —
окно настроек, transport, верхняя капсула Overlay и строка режимов шапки выпадающей
панели — переводят это в два действия:

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
не доехал `ScrollArea` окна настроек. Повтор тех же входных данных в многопроходной
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
`CloseSettings`. Окно — область того же порядка, аллоцирована позже и забирает
ввод у скрима; внутри окна `ui.interact` на теле не даёт клику провалиться к
скриму. Тосты аллоцируются ещё позже и остаются выше. Пока окно открыто,
`globe_view` считает кадр модальным (`layout.settings.is_some()`): вращение, зум
и клик по сфере не обрабатываются, а вне модалки работает фильтр по
`press_origin` — жест, начатый под панелью, глобус не двигает.

На уходе `DeskLayout` уже не отдаёт зону, поэтому каркас и содержимое
дорисовываются по сохранённому `settings_rect`: скрим получает `Sense::hover`,
окно — `interactable(false)`. Гаснущие слои рисуются, но ввод не принимают.
Скрытия окна по Escape нет.

## 7. Тесты

- `layout`: `open_settings_modal_is_centered_and_blocks_the_globe`,
  `closed_settings_modal_does_not_block_anything`,
  `settings_modal_clamps_to_a_narrow_viewport`,
  `panel_keeps_its_scene_under_the_settings_modal`,
  `listening_has_no_settings_zone`;
- `controls`: `switch_state_does_not_move_the_row_or_the_track` (тумблер не меняет
  аллокацию и hit-зону), `disabled_switch_keeps_its_geometry`,
  `knob_travels_between_the_track_edges`,
  `long_label_wraps_instead_of_pushing_the_control` (ширина слота фиксирована,
  правый край совпадает у короткой и длинной подписи),
  `narrow_row_stacks_the_control_below_the_label` (на 232 px контрол уходит
  под подпись и подсказку и занимает всю строку);
- `settings_panel`: reduce motion даёт конечную альфу без перехода, блоки идут
  по порядку на входе и в обратном порядке на выходе, альфа не выходит из
  `0..=1` включая NaN; `narrow_modal_falls_back_to_the_icon_rail`;
  `every_category_has_a_label_and_a_hint`;
- `widgets`: `wheel_over_slider_steps_volume_and_commits`,
  `wheel_is_deduplicated_within_one_input_time`,
  `wheel_outside_slider_leaves_volume_and_scroll_alone`,
  `wheel_clamps_at_volume_edges_without_commit` — headless-прогоны `ctx.run_ui`
  с `Event::PointerMoved` + `Event::MouseWheel { unit: Line }`; отдельный кадр
  наведения нужен потому, что `hovered()` читает состояние прошлого прохода;
- `session::window`: `mode_switch_of_an_open_window_reopens_it` — reopen
  отличается от закрытия, только поэтому настройки при смене режима сохраняются;
- `i18n::labels`: у каждого значения `ThemeMode`, `AccentColor`,
  `MotionReactivity`, `MotionStyle` есть непустая подпись; каталоги всех языков
  заполняют подзаголовки страниц;
- `settings`: `roundtrip_persists_language_volume_and_visualizer`,
  `invalid_volume_is_sanitized_on_load_and_write`,
  `unknown_language_code_falls_back_without_touching_other_fields`.
