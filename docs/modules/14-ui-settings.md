# Модуль: Панель настроек (drawer)

**Путь в коде:** `src/ui/settings_panel.rs`  
**Ответственность:** Содержимое правой панели настроек; scrim и модалка — в `app.rs`.  
**Зависимости:** egui, `ui::animation`, `ui::palette`, `ui::widgets`, `ui::icons`, `i18n`, `settings`, `session::WindowMode`.

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
`min(SETTINGS_WIDTH = 340, viewport.width() − 2·MARGIN)`, правая колонна на всю
высоту между полями. Она входит в `blocks_point`, поэтому глобус не получает
клики и drag, начатые над ней. В `Listening` зоны нет: `Session::open_settings`
в этом режиме сначала переводит окно в `Overlay`.

Содержимое входит одним `stagger` по прогрессу drawer'а: заголовок и шесть
блоков секций (`BLOCK_HEADER`…`BLOCK_ABOUT`). Гаснут только краски — `dimmed` с
`fade_visuals` правят `Visuals`, `widgets::faded_group` приглушает контролы, чей
цвет приходит из палитры. Раскладка, идентификаторы виджетов и hit-зоны
остаются неподвижными (ADR-0032 §7).

## 3. Секции

- **Appearance** — тема (`ThemeMode::ALL`), акцент (шесть плашек-свотчей с
  подсказкой), чекбокс визуализатора (`UiAction::ToggleSpectrum`), число столбцов
  из `VIZ_BAR_CHOICES`.
- **Language** — `Language::ALL`: `System` и 10 языков с нативными именами,
  пояснение.
- **Audio** — громкость (`widgets::volume_slider_width`, 230 px) и пояснение.
- **Motion & accessibility** — реактивность (`MotionReactivity::ALL`), характер
  движения (`MotionStyle::ALL`), reduce motion, flips; под чипами — подсказка
  `motion_spin_hint` о том, что без реактивности глобус вращается сам.
- **Window** — `WindowMode::ALL`, пояснение, чекбокс карточки управления,
  `Hide`, `Quit`.
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

`widgets::volume_slider` и `widgets::volume_slider_width` возвращают
`SliderResponse { changed, committed }`, где `committed` — отпускание drag либо
изменение без продолжающегося drag (клик по треку, клавиша). Зоны — drawer,
transport и верхняя капсула Overlay — переводят это в два действия:

- `changed && !committed` → `UiAction::PreviewVolume` → `Session::preview_volume`:
  только `audio.set_volume` и `pending_volume`, файл не трогается;
- `committed` → `UiAction::CommitVolume` → `Session::commit_volume`: звук плюс
  запись в `SettingsStore`, pending сбрасывается.

Клавиатурный шаг тоже коммитит: стрелки ±5 %, Home/End — край. Дискретные
команды (IPC `Volume`, трей VolumeUp/Down, MPRIS) идут через `Session::set_volume`
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
- `settings_panel`: reduce motion даёт конечную альфу без перехода, блоки идут
  по порядку на входе и в обратном порядке на выходе, альфа не выходит из
  `0..=1` включая NaN;
- `session::window`: `mode_switch_of_an_open_window_reopens_it` — reopen
  отличается от закрытия, только поэтому drawer при смене режима сохраняется;
- `i18n::labels`: у каждого значения `ThemeMode`, `AccentColor`,
  `MotionReactivity`, `MotionStyle` есть непустая подпись;
- `settings`: `roundtrip_persists_language_volume_and_visualizer`,
  `invalid_volume_is_sanitized_on_load_and_write`,
  `unknown_language_code_falls_back_without_touching_other_fields`.
