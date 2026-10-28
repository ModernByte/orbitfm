# Интерфейс: Системный трей (SNI)

**Реализация:** `src/tray/` на `ksni` 0.3.7 (StatusNotifierItem + `com.canonical.dbusmenu`)  
**Решение:** ADR-0020 «Иконка в системном трее (StatusNotifierItem на ksni)»  
**Платформа:** только Linux и только при включённой фиче `tray` (`default = ["tray"]`);
на остальных конфигурациях `spawn` возвращает `None`, и приложение работает ровно
так же, только без иконки.

Трей не знает ни про egui, ни про станции: наружу у него ровно два
доменно-независимых типа. `TraySnapshot` — что показать, `TrayCommand` — что
сделать. Команды впадают в тот же dispatch Application Core, что IPC и MPRIS,
поэтому трей — третий источник команд, а не новый протокол (ADR-0005 не меняется).
Подписи меню и заглушка тултипа берутся из `i18n::catalog` языка снимка (ADR-0030).

```rust
/// Шаг громкости: и колесо по иконке, и пункты меню двигают громкость на него.
pub(crate) const VOLUME_STEP: f64 = 0.1;

/// Запрос пользователя из трея.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrayCommand {
    Toggle,
    Show,
    PlayPause,
    Stop,
    Previous,
    Next,
    Roulette,
    VolumeUp,
    VolumeDown,
    SetTheme(ThemeMode),
    Settings,
    Quit,
}

/// Что трей показывает прямо сейчас.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TraySnapshot {
    pub(crate) window_visible: bool,
    pub(crate) playing: bool,
    pub(crate) station: Option<String>,
    pub(crate) theme: ThemeMode,
    pub(crate) language: Language,
}

/// Живой трей: куда публиковать снимки и откуда читать команды.
/// `None` в `Session::tray` означает «трея нет».
pub(crate) struct TrayHandle {
    /// Очередь команд: `try_recv` в `tick()` при открытом окне и
    /// `recv` в `select!` при закрытом (`src/session/park.rs`).
    pub(crate) commands: tokio::sync::mpsc::UnboundedReceiver<TrayCommand>,
    /* приватный отправитель снимков */
}

impl TrayHandle {
    /// Публикует снимок. `false` означает, что трей умер: больше к нему не идём.
    pub(crate) fn publish(&self, snapshot: &TraySnapshot) -> bool;
}

/// Регистрирует трей и запускает его поток-владелец.
pub(crate) fn spawn(
    app_id: &'static str,
    title: &'static str,
    wake: WakeFn,
) -> Option<TrayHandle>;
```

## 1. Контракт типа

| Элемент | Сигнатура | Поведение |
|---------|-----------|-----------|
| `VOLUME_STEP` | `pub(crate) const f64 = 0.1` | Единый шаг для `VolumeUp`/`VolumeDown` (колесо и меню): 10 шагов от `0.0` до `1.0` |
| `TrayCommand` | `enum`, 12 вариантов, `Copy + Eq` | Ровно один вариант на каждое действие пользователя; `Hide` нет — окно прячут тем же `Toggle`, что Noctalia-виджет и IPC; `Settings` открывает окно настроек (ADR-0031) |
| `TraySnapshot` | `struct`, 5 полей, `Default + Eq` | Снимок витрины; `Default` — окно скрыто, эфира нет, тема и язык дефолтные. `Eq` нужен `sync_tray`: публикуется только изменение, и смена языка — тоже изменение |
| `TrayHandle` | `struct` | Владелец обеих очередей живого трея; `None` в `Session::tray` означает «трея нет» |
| `TrayHandle::publish` | `fn publish(&self, snapshot: &TraySnapshot) -> bool` | Non-blocking отправка снимка в поток-владелец; `false` — канал закрыт, трей умер |
| `TrayHandle::commands` | `UnboundedReceiver<TrayCommand>` | Единственный путь команд наружу из dbus-потока |
| `spawn` | `fn spawn(app_id: &'static str, title: &'static str, wake: WakeFn) -> Option<TrayHandle>` | `Some` — SNI-item зарегистрирован и поток-владелец жив; `None` — хоста нет, регистрация не удалась или сборка без фичи. Ошибки только логируются (`warn!`) |

## 2. `TrayCommand` → действие Core

Исполнение — `Session::execute_tray_command` (`src/session/tray.rs`), тот же
код при открытом и при закрытом окне. Пункты в таблице названы полями каталога
`text.tray.*` (английский источник); рисуется подпись на языке снимка (ADR-0030).

| Команда | Источник в SNI | Вызов Core |
|---------|----------------|------------|
| `Toggle` | левый клик (`Activate`) и пункт `show_window`/`hide_window` | `window.toggle()` → `UiIntent::Show`/`Close` |
| `Show` | средний клик (`SecondaryActivate`) | `window.show()` |
| `PlayPause` | `CheckmarkItem` `play_pause` | `toggle_playback()` |
| `Stop` | `StandardItem` `stop` | `stop_playback()` |
| `Previous` / `Next` | `StandardItem` `previous` / `next` | `play_relative(∓1)` по видимой очереди |
| `Roulette` | `StandardItem` `radio_roulette` | `start_roulette()`; ответ придёт в `tick()` или в `Parked::Roulette` |
| `VolumeUp` / `VolumeDown` | колесо (`Scroll`) и пункты `volume_up` / `volume_down` | `nudge_volume(±VOLUME_STEP)` с `clamp` к `0.0..=1.0` |
| `SetTheme(mode)` | `RadioGroup` с подписями `i18n::theme_label` | `set_theme(mode)` в `SettingsStore` |
| `Settings` | `StandardItem` `settings` | `open_settings()`: в `Listening` сначала переводит окно в `Overlay`, затем открывает окно настроек и показывает окно, если оно было скрыто |
| `Quit` | `StandardItem` `quit` | `window.quit()` |

Ответов у трей-команд нет: SNI-меню не ждёт строки, поэтому неудача уходит в
`warn!` (и в `Notice::Error` только там, где UI всё равно откроется, — тема).
`Stop` и `Quit` намеренно разные пункты: `Stop` глушит поток, `Quit` завершает
процесс.

## 3. `TraySnapshot` → витрина SNI

| Поле снимка | Что показывает хост |
|-------------|---------------------|
| `window_visible` | подпись пункта `show_window` / `hide_window` |
| `playing` | `Status::Active`, если `playing` или `window_visible`, иначе `Status::Passive`; чекмарк `play_pause`; выбор между шаблонами `now_playing` и `paused` в первой строке меню |
| `station` | `ToolTip.description`; подстановка `{station}` в первую (неактивную) строку меню через `i18n::fill`; `None` → `nothing_playing` |
| `theme` | `selected` в `RadioGroup` System / Dark / Light, подписи — `i18n::theme_label` |
| `language` | язык всех подписей меню и заглушки тултипа: `i18n::catalog(language)`; смена языка меняет снимок, поэтому меню перестраивается без перезапуска (ADR-0030) |

`playing` — то же определение, что у MPRIS и `status`: `Playing`, `Buffering`
или `Reconnecting` (`src/session/tray.rs`). Язык в снимке всегда разрешённый —
`Session` резолвит `System` при старте и при смене настройки. Название станции и
`ToolTip.title` не переводятся (ADR-0030 п. 4). Громкости в снимке нет: у SNI нет
витрины для неё, а `Volume Up/Down` — действие, а не состояние.

## 4. Инварианты потоков и блокировок

- В процессе может быть только один трей: блокирующий ksni-API входит в один
  общий process-global current-thread runtime (ADR-0020 п. 4).
- Поток-владелец — обычный `std::thread` с именем `orbitfm-tray`. `spawn`,
  `Handle::update` и `shutdown().wait()` с воркера `rt-multi-thread` паникуют
  «Cannot start a runtime from within a runtime».
- Колбэки ksni (`activate`, `secondary_activate`, `scroll`, `activate` пунктов
  меню) только кладут команду в `unbounded_channel` и вызывают `wake`. Ничего
  блокирующего в них нет, иначе зависнет dbus-поток.
- `Handle::update` (пересборка свойств и меню с dbus round-trip) вызывается
  только на потоке-владельце. Из UI-потока его звать нельзя — поэтому `Session`
  пишет снимки в канал, а поток сворачивает очередь до последнего (`try_recv`
  после `recv`) и обновляет item один раз.
- `icon_pixmap()` хост вызывает на каждое событие трея, поэтому пиксели
  строятся однократно в `static ICONS: LazyLock<Vec<Icon>>` (ADR-0020 п. 6).

## 5. Деградация

| Ситуация | Поведение |
|----------|-----------|
| SNI-хоста нет (`Error::Watcher(_)`, `Error::WontShow`, catch-all по `#[non_exhaustive]`) | `spawn` → `None`, `warn!`, приложение работает как раньше |
| Хост ушёл (`watcher_offline`) | возврат `true`: соединение держим и ждём `watcher_online` — `false` выключило бы трей до конца жизни процесса |
| Регистрация прошла, но соединение мёртвое | `Handle::is_closed()` после `spawn` → `warn!` и `None` (ADR-0020 п. 8) |
| Поток-владелец умер во время работы | `publish` → `false`, `Session` обнуляет `tray` и `tray_snapshot`; `tray_recv` в `select!` гасит ветку, чтобы не было busy-цикла |
| Процесс выходит | `Session` дропается, канал снимков закрывается, поток делает `handle.shutdown().wait()` |
| Сборка без фичи `tray`, macOS, Windows | `spawn` → `None`; `TrayCommand` варианты формально не конструируются, поэтому помечены `cfg_attr(..., allow(dead_code))` |

## 6. Что не является контрактом

- `OrbitTray` (`src/tray/sni.rs`) — реализация `ksni::Tray`, живёт на потоке-владельце;
- `icon::ICONS`, `icon::Mark`, `icon::pixel`, `sni::serve`, `sni::start`,
  `sni::standard`, `sni::now_playing_label`, `sni::nothing_playing`,
  `sni::find_hicolor` и `THEME_PATH` — приватные детали отрисовки, сборки меню
  и регистрации;
- мнемоники и порядок пунктов меню — детали ADR-0020 п. 5, а не API; хост вправе
  показать их как хочет;
- `icon_theme_path`: если в системе есть `.../icons/hicolor`, хост может взять
  векторную иконку из упаковки вместо нашего pixmap'а.

## 7. Тесты, фиксирующие контракт

`src/tray/mod.rs` — `snapshot_defaults_to_hidden_and_idle`,
`snapshots_compare_by_content`; `src/tray/sni.rs` —
`status_is_active_only_when_something_happens`, `tooltip_shows_the_station_or_silence`,
`menu_offers_window_playback_volume_theme_settings_and_quit`,
`menu_header_shows_the_current_stream_without_being_a_command`,
`every_label_comes_from_the_catalog_for_the_snapshot_language`,
`theme_radio_marks_the_current_mode`, `play_pause_check_follows_the_stream`,
`clicks_publish_a_command_and_wake_the_core`,
`icon_sizes_match_the_specification_and_byte_order`, `theme_path_is_empty_or_a_hicolor_root`;
`src/tray/icon.rs` — `icons_are_built_for_every_host_size`, `argb_puts_alpha_first`,
`planet_core_is_opaque_and_corners_are_empty`, `orbit_reaches_beyond_the_planet`,
`live_dot_is_part_of_the_mark`.

Живая проверка в Noctalia (регистрация, свойства, клики, колесо, меню, команды
при закрытом окне) — в ADR-0020, раздел «Проверка реализации».

## 8. Связанные документы

- ADR-0020 — выбор протокола, зависимости, меню, иконка, деградация
- ADR-0030 — язык снимка и каталог `text.tray.*` (заменяет English-only конвенцию ADR-0020 п. 5)
- ADR-0031 — окно настроек, которое открывает команда `Settings`
- ADR-0018 — почему трей нужен: закрытие окна = скрытие, процесс живёт
- ADR-0005 — IPC (трей не меняет протокол), ADR-0006 — MPRIS, ADR-0007 — Noctalia
- `docs/modules/12-tray.md` — модульная спецификация `src/tray/`
- `docs/interfaces/10-session.md` § 7 — куда `TrayCommand` впадает в парковке
- `docs/interfaces/09-settings.md` — `ThemeMode` как часть снимка
- `docs/interfaces/12-i18n.md` — контракт каталога, `Language` и `theme_label`
