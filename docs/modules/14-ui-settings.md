# Модуль: Панель настроек (drawer)

**Путь в коде:** `src/ui/settings_panel.rs`  
**Ответственность:** Правая панель настроек, scrim и модальные действия.  
**Зависимости:** egui, `ui::palette`, `ui::widgets`, `i18n`, `settings`.

---

## 1. Назначение

Единое место изменения всех пользовательских настроек (ADR-0031). Drawer
заменяет popup из HUD: один скролл, секции, пояснения, полноширинные контролы.
Модуль только рисует и возвращает `UiAction`; сохранение выполняет `Session`.

## 2. Состояние

| Владелец | Поле | Смысл |
|----------|------|-------|
| `Session` | `settings_open: bool` | drawer открыт |
| `DeskLayout` | `settings: Option<Rect>` | зона панели в текущем кадре |
| `OrbitApp` | `Toast` | сообщения об ошибках сохранения |

Зона drawer'а вычисляется в `DeskLayout::compute`. Она входит в
`blocks_point`, поэтому глобус не получает клики и drag, начавшиеся над ней.
В `Listening` зоны нет: drawer принудительно закрыт.

## 3. Секции

- **Appearance** — тема, акцент, визуализатор, число столбцов.
- **Language** — `System` и 10 языков с нативными именами.
- **Audio** — громкость.
- **Motion & accessibility** — реактивность, стиль, flips, reduce motion.
- **Window** — Focus/Overlay/Listening, Hide, Quit.
- **About** — версия, app-id, лицензия, пояснение про фоновую работу.

Неактивные контролы (flips при выключенном движении) остаются видимыми и
объясняют причину. Выбор значения не закрывает drawer.

## 4. Открытие и закрытие

| Источник | Поведение |
|----------|-----------|
| Шестерёнка Focus | toggle drawer |
| Шестерёнка Overlay | toggle drawer |
| Ctrl+, | toggle drawer в Focus/Overlay |
| Ctrl+, в Listening | перевести окно в Overlay и открыть |
| Трей «Settings…» | показать окно, при Listening — Overlay, открыть drawer |
| Scrim / крестик / Escape | закрыть drawer |
| Hide, WM-close, park | `close_settings()` |
| Смена WindowMode | drawer сохраняется только при reopen |
| Выбор Listening | закрыть drawer |

`run_session` вызывает `close_settings()` после проверки `take_reopen()`, поэтому
пересоздание viewport'а из-за смены режима сохраняет drawer, а любое другое
закрытие — нет.

## 5. Громкость

`widgets::volume_slider*` возвращает `SliderResponse { changed, committed }`.
При drag применяется только `audio.set_volume`; на release, клике или
клавиатурном шаге — `Session::commit_volume` с записью файла. Pending-значение
сбрасывается в файл при `hide`, `end_window` и `quit`.

## 6. Ввод

Scrim рисуется на весь viewport под drawer'ом (`Order::Foreground`), клик по
нему закрывает панель, колесо и клики не доходят до глобуса. Тосты остаются
выше. `Escape` сначала закрывает drawer, затем help.

## 7. Тесты

- `blocks_point` для открытого/закрытого drawer'а и узкого viewport;
- ширина clamp и отсутствие зоны в Listening;
- `SliderResponse`: drag → только changed, release → committed;
- закрытие drawer'а на hide/park и сохранение при reopen.
