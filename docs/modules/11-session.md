# Модуль: Application Core (`src/session/`)

**Путь в коде:** `src/session/*`  
**Ответственность:** Всё, что переживает закрытие окна: подсистемы, доменное
состояние, маршрутизация IPC-, MPRIS- и трей-команд, воспроизведение, поиск,
маркеры, простой без окна.  
**Зависимости:** `audio`, `geo`, `services`, `media`, `tray`, `library`, `settings`,
`render` (только `GlobeCamera`), `ui` (чистые помощники `single_line` и
`station_filter`), `tokio`, `tracing`  
**Решение:** ADR-0018 «Скрытие окна = закрытие вьюпорта с перезапуском UI-цикла»,
ADR-0020 «Иконка в системном трее (StatusNotifierItem на ksni)»,
ADR-0021 «Режимы окна (`WindowMode`)»

---

## 1. Назначение и границы

На Wayland скрыть окно можно только закрыв root-вьюпорт, поэтому
`eframe::run_native` перезапускается из главного потока при каждом показе, а
`OrbitApp` создаётся заново. Чтобы состояние не умирало вместе с окном, оно
вынесено в `Session` — пакет `src/session/`, которым владеет главный цикл
`run_session` (`src/main.rs`).

| Живёт в `src/session/` (переживает закрытие окна) | Живёт в presentation (один прогон UI) |
|---|---|
| `AudioEngine`, `GeoService`, `RadioBrowserClient`, `MediaControls`, `TrayHandle`, `StationLibrary`, `SettingsStore`, `tokio::runtime::Handle`, `WakeHandle` | `GlobeRenderer` в `callback_resources`, `egui`-стиль (`palette::configure_style`), цветной emoji-шрифт |
| Текущая станция и URL, контекст/результаты/маркеры поиска и поколение, `library_tab`, `country_filter`, `globe_camera`, `panel_open`, `show_help`, `spectrum_enabled` | `VisualizerModel`, `SpectrumOverlay`, `CalloutState`, `FaviconCache` с текстурами, `reqwest::Client` для иконок, `Toast` |
| Геометрия окна (`WindowState`: `visible`, `size`, `intent`, `quitting`) и очередь `Notice` | `globe_ready`, чтение `egui::Context`, раскладка `DeskLayout` и рисование зон |
| Исполнение `IpcCommand`/`MediaCommand`/`TrayCommand`, `park()` | `UiAction` → вызовы методов `Session`; `UiIntent` → `ViewportCommand` |

Правило слоя (AGENTS § 4.2): presentation зависит от Core, а не наоборот. Методы
Core не принимают `&egui::Context` — детали в `docs/interfaces/10-session.md` § 2.

## 2. Состояние `Session`

Поля (`src/session/mod.rs`, `struct Session`) сгруппированы по назначению.

### 2.1. Подсистемы и каналы

| Поле | Тип | Пояснение |
|------|-----|-----------|
| `audio` | `AudioEngine` | Владелец libmpv-потока; при уничтожении `Session` останавливает аудио-поток (`impl Drop for AudioEngine`) |
| `audio_events` | `broadcast::Receiver<AudioEvent>` | Подписка `audio.subscribe()` в `Session::new`; вторая подписка в `main` только будит GUI |
| `commands` | `mpsc::Receiver<IpcRequest>` | Очередь IPC из `IpcServer::commands()` |
| `geo` | `GeoService` | Границы стран, `country_shape`, `country_at_sphere_point` |
| `radio` | `RadioBrowserClient` | Клонируется в фоновые задачи поиска, рулетки и click |
| `media` | `Option<MediaControls>` | `None`, когда MPRIS недоступен (нет D-Bus); тогда очередь команд нет |
| `tray` | `Option<TrayHandle>` | `None`, когда SNI-хоста нет или сборка без фичи `tray`; сам `Option` меняется и в рантайме: мёртвый трей `sync_tray` выключает ветку |
| `library` | `StationLibrary` | Избранное и история (до 20 записей, `HISTORY_LIMIT`) |
| `settings` | `SettingsStore` | Тема, акцент, `viz_bars`, `help_seen` |
| `runtime` | `tokio::runtime::Handle` | `block_on` для коротких аудио-операций и `spawn` для сетевых задач |
| `wake` | `WakeHandle` | Разделяемый слот пробуждения: `Arc<Mutex<Option<Notifier>>>` |

`wake` — именно слот, а не замыкание: его делят IPC-сервер, playwire-колбэк и
фоновые задачи. Presentation ставит `Notifier` (`ctx.request_repaint()`) в
`OrbitApp::new` и снимает в `end_window`, поэтому в простое `waker()` без слота
молчит, а события читаются очередями напрямую (`wake_handle_is_quiet_without_a_window`).

### 2.2. Доменные данные

| Поле | Тип | Пояснение |
|------|-----|-----------|
| `globe_camera` | `GlobeCamera` | Позиция камеры должна переживать закрытие окна (ADR-0018 п. 2); вращение и зум пишет presentation, Core вызывает `fly_to` |
| `visualizer_enabled` | `bool` | Пользовательский переключатель визуализатора и кольца; инициализируется `audio.spectrum_enabled()` (по умолчанию `true`) |
| `spectrum_enabled` | `bool` | Фактическое состояние аудио-анализа; включается по `analysis_needed` (визуализатор или движение глобуса), каждый `tick()` перечитывается из движка, при ошибке включения откатывается |
| `current_url` | `Option<String>` | Заполняется из `start_station` и из `AudioEvent::Started`; не очищается при `Stopped`/`Error` — нужен для повторного запуска из MPRIS |
| `current_station` | `Option<Station>` | Метаданные эфира: `status`, MPRIS, подсветка строки списка |
| `search` | `SearchState` | Всё о поиске: строки запросов, контекст, результаты, поколение, незавершённые задания |
| `markers` | `Vec<GlobeMarker>` | Маркеры глобуса, перестраиваются при каждом ответе поиска |
| `library_tab` | `LibraryTab` | Активная вкладка панели = источник очереди `prev`/`next` |
| `country_filter` | `CountryFilterState` | Локальные фильтры панели (`text`, `genre`, `sort`, `compact_rows`) |
| `panel_open` | `bool` | Панель станций; `true` по умолчанию |
| `show_help` | `bool` | `!settings.settings().help_seen` при создании ядра |

Эффективную реактивность движения отдаёт `motion_reactivity()` (ADR-0028):
`reduce_motion` → `Off`; Focus — значение настройки, Overlay — не выше `Low`,
Listening — `Off`. `motion_reactivity()`, `analysis_needed()` и `sync_analysis()`
не хранят отдельного состояния: они вычисляются из настроек и режима окна.

### 2.3. Окно и очередь сообщений

| Поле | Тип | Пояснение |
|------|-----|-----------|
| `window` | `WindowState` | `visible`, `size: Option<[f32; 2]>`, `mode: WindowMode`, `reopen`, `intent: Option<UiIntent>`, `quitting` (`src/session/window.rs`) |
| `notices` | `VecDeque<Notice>` | Очередь сообщений Core → пользователю; `take_notices()` забирает всю очередь через `std::mem::take` |
| `tray_snapshot` | `Option<TraySnapshot>` | Последний опубликованный треем снимок: `sync_tray` шлёт в SNI только изменение |

`size: Option<...>` — `None` только до первого `begin_window` (то есть на самом
первом показе процесса): тогда `restore_size()` отдаёт `WINDOW_SIZE` (920×660).
Пользовательский размер относится только к `Focus`: `remember_size` игнорирует
капсульные размеры, иначе `Overlay` заразил бы `Focus` шириной 420 px
(`user_size_is_remembered_only_while_focused`). `mode` — режим окна (ADR-0021):
он есть и у скрытого окна, и `restore_size()`/`min_size()` берут геометрию из него.
`reopen` — единичный признак «пересоздать вьюпорт с геометрией нового режима»;
его забирает `run_session` (`take_reopen`) и снимает `begin_window`.
`quitting` — единственный признак, отличающий «скрыли» от «вышли»: окно
закрывается и в том, и в другом случае, поэтому `run_session` обязан проверить
флаг ровно один раз по возврату из `run_native`. `notices` — очередь, а не одно
сообщение: сообщения, накопленные при закрытом окне, доживают до первого кадра
после показа (в отличие от `Toast`, который держит одно сообщение).

## 3. Состав пакета

| Файл | Ответственность | Типы и функции наружу (`pub(crate)`) |
|------|-----------------|--------------------------------------|
| `mod.rs` | Ядро: владение подсистемами, пробуждение, `status`, ре-экспорт контрактов пакета | `Session`, `AppServices`, `Notice`, `ParkOutcome`, `WakeHandle`, `LibraryTab`; наружу из `window.rs` — `UiIntent`, `WindowMode`, `WindowState`, `WINDOW_TITLE` |
| `window.rs` | Состояние окна: режим (`WindowMode`), геометрия и заголовки по режиму, намерения UI, признак перезапуска (ADR-0018, ADR-0021) | `WindowMode` (+ `ALL`/`label`/`default_size`/`min_size`/`title`/`as_str`), `UiIntent`, `WindowState`, `WINDOW_SIZE`, `WINDOW_MIN_SIZE`, `WINDOW_TITLE`, `OVERLAY_SIZE`, `OVERLAY_MIN_SIZE`, `OVERLAY_TITLE`, `LISTENING_SIZE`, `LISTENING_MIN_SIZE`, `LISTENING_TITLE` |
| `commands.rs` | Разбор `IpcCommand` (общий для окна и парковки), `impl From<WindowModeArg> for WindowMode` и настроек оформления и движения | `Session::{execute, process_commands, set_theme, set_accent, set_viz_bars, set_visualizer_enabled, set_motion, set_motion_style, set_allow_flips, set_reduce_motion, mark_help_seen}` |
| `playback.rs` | Аудио-события, запуск/остановка, избранное, история, очередь `prev`/`next`, витрина MPRIS | `Session::{process_audio_events, apply_audio_event, resolve_station, start_station, spawn_click, toggle_favorite, clear_history, focus_country, set_volume, set_audio_paused, resume_playback, toggle_playback, stop_playback, process_media_commands, execute_media_command, visible_queue, play_relative, sync_media}` |
| `search.rs` | Поиск по имени/стране/жанру с поколениями ответов и рулетка | `SearchMode`, `SearchContext`, `SearchState`, `SearchResponse`, `STATION_LOOKUP_TIMEOUT`, `Session::{start_search, start_country_search, start_genre_search, switch_mode, poll_search, start_roulette, poll_roulette, apply_roulette}` |
| `markers.rs` | Агрегация станций с координатами в маркеры глобуса | `GlobeMarker`, `build_markers` (+ `#[cfg(test)] sample_station`, `geo_station`) |
| `tray.rs` | Трей: публикация снимка и исполнение `TrayCommand` тем же dispatch | `Session::{sync_tray, process_tray_commands, execute_tray_command}` (приватные `nudge_volume`, `tray_snapshot`) |
| `park.rs` | Простой без окна: `select!` по IPC, MPRIS, трею, аудио и ответу рулетки | `ParkOutcome`, `Session::park` (приватные `Parked`, `wait_on`, `media_recv`, `tray_recv`, `roulette_recv`) |

Помощники `mpris_artist`, `country_search_params`, `roulette_label`,
`station_geo`, `station_popularity`, `Parked`, `wait_on`, `media_recv`,
`tray_recv`, `roulette_recv`, `format_status` остаются приватными: ими
пользуется один файл.

## 4. Поведение

### 4.1. Аудио-события

`process_audio_events` вычерпывает `audio_events` через `try_recv`: `Empty` и
`Closed` завершают цикл, `Lagged(skipped)` логируется (`warn!`) и продолжается
дальше. `apply_audio_event` реагирует только на `AudioEvent::Started { url }` —
пишет его в `current_url`. Остальные события (`Stopped`, `Buffering`,
`Reconnecting`, `Error`, `EndOfFile`) в Core ничего не меняют: состояние
читается из `audio.state()` по требованию.

### 4.2. Разрешение и запуск станции

`resolve_station(station_uuid)`:
1. линейный поиск в `search.results` — если станция уже в выдаче, сетевой запрос
   не делается (внимание: берётся полный список выдачи, локальные фильтры
   панели в подборе не участвуют);
2. иначе `runtime.block_on` + `tokio::time::timeout(STATION_LOOKUP_TIMEOUT /* 8 с */,
   radio.get_station(uuid))`;
3. таймаут маппится в `RadioError::Network("stations catalog timeout")`.

`start_station(station)`:
1. `url_resolved.trim()` — пусто ⇒ `Err("station has no url_resolved")`;
2. `block_on(audio.play(&url))` — ошибка возвращена как `String`;
3. сохраняются `current_url` и `current_station`;
4. `library.record_played` — станция встаёт в начало истории; ошибка не мешает
   слушать: `warn!` + `Notice::Error("History not saved: …")`;
5. `spawn_click(stationuuid)` — фоновая задача `radio.send_click`, ошибка только
   логируется;
6. `sync_media()`.

### 4.3. Избранное и история

`toggle_favorite(&station)` — `library.toggle_favorite`; успех логируется
`debug!`, ошибка даёт `warn!` и `Notice::Error("Favorites not saved: …")`.
`clear_history()` — при успехе `Notice::Info("History cleared")`, при ошибке
`Notice::Error("Failed to clear history: …")`. Ошибки библиотеки никогда не
прерывают воспроизведение.

### 4.4. Фокус страны

`focus_country(&country)`: `geo.country_shape(&country.iso_a2)` →
`CountryShape::focus_point()` → `globe_camera.fly_to(lat, lon, 650 мс)`. Если
формы страны или точки нет — метод ничего не делает. Выбор страны в панели
запускается из presentation двумя вызовами подряд: `focus_country` +
`start_country_search` (`src/app.rs`, `UiAction::SelectCountry`).

### 4.5. Очередь `prev`/`next`

`visible_queue()` каждый раз снимает снапшот видимого списка — постоянного
плейлиста нет:

| `library_tab` | Очередь |
|---------------|---------|
| `Favorites` | `library.favorites().to_vec()` |
| `History` | `library.history().to_vec()` |
| `Results` и контекст активен | `filtered_results(&search.results, &country_filter)` (текст, жанр, сортировка) |
| `Results` без контекста | `search.results.clone()` |

`play_relative(step: i8)`: пустой снапшот — no-op; позиция текущей станции ищется
по `stationuuid`; шаг считается как `(index + step).rem_euclid(len)`, то есть со
циклом в обе стороны. Если текущей станции в списке нет, `+1` даёт первый
элемент, `-1` — последний. Запуск станции идёт через `start_station`, ошибка —
`Notice::Error`. Источники одинаковые: кнопки `⏮`/`⏭` (`UiAction::PlayRelative`) и
MPRIS `Next`/`Previous`.

### 4.6. Синхронизация с MPRIS

`sync_media()` вызывает `media.set_playback(&PlaybackInfo)`, если `MediaControls`
доступны; соответствие `PlaybackState`:

| Состояние | Трек | `playing` |
|-----------|------|-----------|
| `Stopped`, `Error(_)` | отсутствует (`title` — пустая строка) | `false` |
| `Paused` | есть | `false` |
| `Playing`, `Buffering`, `Reconnecting` | есть | `true` |

Правила метаданных: `title` — `single_line(station.name)`, а если он пуст и трек
есть — `WINDOW_TITLE`; `artist` — «`state, country`», при отсутствии одного из
полей — то, что осталось, при отсутствии обоих — `single_line(tags)`, дальше
`None`; `art_url` — `station.favicon` без изменений; `url` — `url_resolved`
(если непустой после `trim`), иначе `current_url`, иначе пустая строка;
`volume` — `audio.volume()`. Без `current_station` заголовок — URL потока
(пустой URL при наличии трека даёт `WINDOW_TITLE`), `artist` и `art_url` — `None`.
Ошибка `set_playback` только логируется.
`sync_media()` вызывается каждый `tick()`, при `end_window()`, после `start_station()`
и после каждого события в `park()`; `MediaControls::set_playback` дедуплицирует
неизменившиеся данные.

### 4.7. Поиск

Режим (`SearchMode`) и выбранный контекст (`SearchContext { mode, country, genre }`)
живут в `search.context`; `is_active()` — есть страна или жанр. `activate_country`
сбрасывает жанр, `activate_genre` — страну.

| Метод | Поведение |
|-------|-----------|
| `start_search` | `search.query.trim()` пуста — no-op; контекст активен → запрос кладётся в `country_filter.text` (локальный фильтр, без сети); `search.pending` → no-op; иначе контекст очищается и запускается `SearchParams { name, limit = 100, hide_broken = true }` |
| `start_country_search(country)` | активирует страну, `country_filter.reset()`, запрос `country_code = <ISO alpha-2>`, `limit = 100`, `hide_broken = true`, затем `country_query.clear()` |
| `start_genre_search(tag)` | `tag.trim().to_lowercase()` пуст — no-op; активирует жанр, `country_filter.reset()`, `tag`, `limit = 200`, `hide_broken = true`, затем `genre_query.clear()` |
| `switch_mode(mode)` | если режим не изменился — no-op; иначе `invalidate_search()`, очистка результатов, маркеров и фильтров, `library_tab = Results` |

`begin_search` увеличивает `generation` (`wrapping_add`), создаёт
`mpsc::unbounded_channel`, выставляет `pending = true`, `empty = false`,
`error = None`, очищает `results` и `markers`, ставит вкладку `Results` и
спавнит tokio-задачу: `tokio::join!(radio.search(params), radio.search(geo_params))`,
где `geo_params` — копия с `has_geo_info = true` и `limit = geo_limit`
(100 для названия и страны, 200 для жанра). Ошибка гео-запроса логируется и
заменяется пустым списком — на выдачу она не влияет. После отправки ответа
задача вызывает `waker()`.

`poll_search` (`try_recv`) различает:
- ответ чужого поколения — только `debug!` («устаревший ответ поиска отброшен»),
  состояние не меняется: быстрый `Esc`-переполск не перерисовывает панель старым
  списком и не возвращает старые маркеры;
- `Ok(outcome)` — `empty = stations.is_empty()`, `results = outcome.stations`,
  `markers = build_markers(outcome.geo_stations)`, `pending = false`, `rx = None`;
- `Err(err)` — ошибка Radio Browser: `error = Some(err)`, маркеры очищаются,
  `pending = false`, `rx = None` (результаты прошлого ответа остаются в списке);
- `Disconnected` — `error = Some("search task ended unexpectedly")`.

Лимиты зафиксированы константами `search.rs`: `SEARCH_RESULT_LIMIT = 100`,
`COUNTRY_RESULT_LIMIT = 100`, `GENRE_RESULT_LIMIT = 200`,
`ROULETTE_FLY_MS = 900`, `STATION_LOOKUP_TIMEOUT = 8 с`.

### 4.8. Рулетка

`start_roulette` блокируется флагом `search.roulette_pending` (повторные нажатия
игнорируются, кнопка в transport неактивна), создаёт канал, ставит `roulette_rx`
и спавнит `radio.random_station()` — тот делает до трёх попыток
(`RANDOM_ATTEMPTS`) и принимает станцию, только если у неё есть `url_resolved` и
корректные координаты. `poll_roulette` читает канал через `try_recv` и отдаёт
найденное в `apply_roulette(outcome: Option<Result<Station, RadioError>>)`, где
`None` — отправитель исчез:

- `Some(Ok(station))` — сбрасывает pending и канал, берёт подпись
  `roulette_label` («имя — страна», без пустых разделителей, иначе `unnamed station`),
  цель `geo_lat.zip(geo_long)`, запускает станцию; при успехе `fly_to` на
  `ROULETTE_FLY_MS = 900 мс` (если координаты есть) и `Notice::Info("Roulette: <подпись>")`;
  ошибка запуска — `Notice::Error("Roulette did not start: …")`;
- `Some(Err(err))` — `Notice::Error("Roulette: <err>")`, текущий эфир не меняется;
- `None` — `Notice::Error("Roulette: task ended unexpectedly")`.

Тот же `apply_roulette` вызывает припаркованный цикл (`Parked::Roulette`), иначе
рулетка из трея при закрытом окне осталась бы невостребованной: ответ приходит
через ~900 мс после команды, а `try_recv` больше никто не крутит.

### 4.9. Маркеры (`markers::build_markers`)

Вход — `Vec<Station>` гео-выдачи, выход — `Vec<GlobeMarker>`. Порядок:
1. дедупликация по `stationuuid` (`HashSet`), повторные UUID отбрасываются;
2. валидация координат `station_geo`: оба значения обязаны быть `Some`, конечными,
   `lat ∈ [-90, 90]`, `lon ∈ [-180, 180]`; `(0, 0)` — заглушка Radio Browser, не
   показывается;
3. ключ группы `((lat * 100).round() as i64, (lon * 100).round() as i64)` — то
   есть округление до 2 знаков (`GROUP_SCALE = 100.0`), станции одного
   «пятнистого пикселя» сливаются;
4. первая станция группы становится её представителем (`GlobeMarker.station`,
   `lat`, `lon`); для совпавшего ключа обновляются `count += 1`,
   `popularity = max(...)`, `bitrate = max(...)`, в `members` добавляется UUID.

`station_popularity` — **только** `votes as f32`: в Core это максимальный сигнал
популярности группы (ADR-0012), но на геометрию маркера он больше не влияет.
Формулы самого отображения в Core нет: `src/render/markers.rs` рисует все маркеры
одинаковыми (радиус `3 px`, единый `palette.marker`), а битрейт и голоса уходят в
подпись при наведении (`MarkerLabel`).
`build_markers` вызывается только из `poll_search` (тесты `src/ui/globe_view.rs`
используют функцию напрямую).

### 4.10. `tick()`

Порядок фиксирован (`src/session/mod.rs`):

```
process_audio_events → process_commands → process_media_commands
→ process_tray_commands → poll_search → poll_roulette
→ sync_media → sync_tray → spectrum_enabled = audio.spectrum_enabled()
```

`tick()` ничего не рисует и не трогает `egui`: ответы IPC отправляются внутри
`process_commands`, а намерения окна забирает `take_intent()` в `OrbitApp::logic`.

### 4.11. `park()`

Цикл: `runtime.block_on(wait_on(&mut commands, &mut audio_events, &mut media,
&mut tray, &mut search.roulette_rx))` → обработка одного события
(`Parked::Ipc` → `execute` + `try_respond`, `Parked::Audio` → `apply_audio_event`,
`Parked::Media` → `execute_media_command`, `Parked::Tray` → `execute_tray_command`,
`Parked::Roulette` → `apply_roulette`, `Parked::QueueClosed` → `quit()`) →
`sync_media()` + `sync_tray()` → проверка `is_quitting()` (`ParkOutcome::Quit`) и
`intent == Some(UiIntent::Show)` (`ParkOutcome::Show`).

`wait_on` собран из пяти веток `tokio::select!`; закрытый `audio_events`,
отсутствие `MediaControls`, отсутствующий или мёртвый `TrayHandle` и невызванная
рулетка не будят цикл зря (`Err(_) => continue`, а `media_recv`, `tray_recv` и
`roulette_recv` для `None` возвращают `std::future::pending()`). Исполнение команд
намеренно вынесено из async-контекста: `execute` зовёт `Handle::block_on` для
операций аудио-ядра. `Parked::QueueClosed` — это закрытие очереди IPC-сервером
(отправитель живёт в `Session`, так что у MPRIS- и трей-веток такой причины выхода
нет): держать процесс без канала нечем, поэтому он трактуется как выход.

В припаркованном состоянии без UI исполняются IPC-команды `play`, `stop`,
`volume`, `status`, `mode` (обе формы), все `MediaCommand` (`Play`, `Pause`, `PlayPause`, `Stop`,
`SetVolume`, `Next`, `Previous`) и все `TrayCommand` (`Toggle`, `Show`,
`PlayPause`, `Stop`, `Previous`, `Next`, `Roulette`, `VolumeUp`, `VolumeDown`,
`SetTheme`, `Quit`). `hide` ничего не делает (окно уже закрыто), `show` и
`toggle` ставят `UiIntent::Show` и выводят `park()`. `mode <режим>` при
скрытом окне меняет только `window.mode` и тоже выводит `park()`: новое окно
сразу собирается с геометрией режима, без перезапуска ради перезапуска. `quit`
ставит `quitting` и выводит `ParkOutcome::Quit`.

### 4.12. IPC-команды и оформление

`execute(&IpcCommand) -> String` — общий разбор для окна и парковки (полная
таблица — `docs/interfaces/10-session.md` § 4). Здесь же `impl From<WindowModeArg>
for WindowMode`: `src/ipc/` не знает доменного режима, превращение делает Core
(ADR-0021 п. 5). Настройки оформления и движения идут через
`SettingsStore` и возвращают `Result<(), String>`: `set_theme`, `set_accent`,
`set_viz_bars`, `set_motion`, `set_motion_style`, `set_allow_flips`,
`set_reduce_motion`;
`set_visualizer_enabled`
вызывает `sync_analysis()` и меняет пользовательский переключатель, а не
состояние анализа напрямую; `mark_help_seen` ошибку только
логирует. Перекрашиванием `egui` занимается presentation: после `set_theme`/
`set_accent` он сам вызывает `palette::configure_style` (ADR-0018 п. 3).

### 4.13. Трей: снимок и команды (ADR-0020)

`sync_tray()` собирает `TraySnapshot { window_visible, playing, station, theme }`
из того же состояния, что и `status`/MPRIS: `playing` — `Playing | Buffering |
Reconnecting`, `station` — `single_line(name)` при живом эфире иначе `None`,
`theme` — из `SettingsStore`. Публикация только по изменению: сравнение с
`tray_snapshot`, `publish` в канал `TrayHandle`, а `false` (поток-владелец умер)
гасит ветку — `tray` и `tray_snapshot` обнуляются, повторных dbus-round-trip не
будет. Вызовы: каждый `tick()`, каждый `end_window()` и после каждого события в
`park()`.

`process_tray_commands()` вычерпывает `tray.commands` через `try_recv` (путь
открытого окна), а `execute_tray_command(TrayCommand)` исполняет одну команду —
общий код для окна и парковки:

| Команда | Действие |
|---------|----------|
| `Toggle` / `Show` | `window.toggle()` / `window.show()` через `UiIntent` |
| `PlayPause` / `Stop` | `toggle_playback()` / `stop_playback()`, ошибка — только `warn!` |
| `Previous` / `Next` | `play_relative(∓1)` по видимой очереди |
| `Roulette` | `start_roulette()`, ответ придёт в `Parked::Roulette` или в следующем `tick()` |
| `VolumeUp` / `VolumeDown` | `nudge_volume(±VOLUME_STEP /* 0.1 */)` с `clamp` к `0.0..=1.0` до вызова аудио-ядра |
| `SetTheme(mode)` | `set_theme(mode)` в `SettingsStore`; ошибка — `Notice::Error` |
| `Quit` | `window.quit()` |

Ошибки команд трея в пользователя не кричат: трей не ждёт ответа, поэтому
неудача уходит в `warn!`, а видимый результат — обновлённый снимок.

### 4.14. Режимы окна (ADR-0021)

`WindowMode { Focus, Overlay, Listening }` живёт в `src/session/window.rs` рядом
с `WindowState`, а не в `SettingsStore`: режим нужен припаркованному циклу
(ADR-0014 ограничивает настройки оформлением). Каждый режим несёт свою геометрию
и заголовок:

| Режим | Размер | Минимум | Заголовок |
|-------|--------|---------|-----------|
| `Focus` | 920×660 | 720×480 | `OrbitFM` |
| `Overlay` | 420×620 | 360×480 | `OrbitFM · Overlay` |
| `Listening` | 420×152 | 360×120 | `OrbitFM · Listening` |

`set_window_mode(mode)` — единственная точка переключения: тот же режим →
`UiIntent::Show`; новый режим при открытом окне → `reopen = true` +
`UiIntent::Close`; при закрытом → просто `Show` (следующий прогон уже в новом
режиме). `run_session` после `end_window()` забирает `take_reopen()` и, если
признак стоит, открывает вьюпорт заново без парковки — иначе живой resize на
Wayland был бы просьбой, которую композитор вправе отбросить (ADR-0018 п. 5).
Заголовок — часть контракта: по нему WM-правила из `wm-configs/` подбирают
размер капсулы, поэтому у каждого режима он свой.

## 5. Текущие ограничения

- `FaviconCache` с текстурами живёт в `OrbitApp` (`src/app.rs`) и теряется при
  закрытии окна: после показа иконки загружаются заново (ADR-0018 п. 9).
- Сохраняется только размер окна (`WindowState::size`) и только в `Focus`,
  позиция — нет: размещение окон решает композитор (правило `open-floating` в
  `wm-configs/orbitfm-niri.kdl`). Капсульные режимы живут фиксированными
  пресетами геометрии (ADR-0021 п. 4).
- `UiIntent::Show` при открытом окне превращается в `ViewportCommand::Focus`,
  который на Wayland документированно не имеет эффекта — фокус при показе
  зависит от композитора.
- `status` в припаркованном состоянии всегда отвечает `window hidden`;
  `toggle` опирается на признак `Session`, а не на состояние композитора: окно,
  свёрнутое WM, считается открытым и `toggle` его закрывает.
- Смена темы/акцента из IPC невозможна: этих команд в протоколе нет (ADR-0005),
  `set_accent` вызывают только действия UI. Тема доступна из трея (`SetTheme`),
  но при закрытом окне перекраска `egui` произойдёт лишь на следующем показе.
- Часть состояния (`search.query`, `panel_open`, `show_help`, `library_tab`,
  `country_filter`, `globe_camera`) UI правит напрямую через `&mut`-поля;
  инкапсуляция держится на соглашении «побочные эффекты — только через методы».
- Показ окна стоит пересоздания UI-прогона (wgpu-адаптер, `GlobeRenderer`,
  шрифты, `egui::Context`) — измерено ≈1.02 с на niri 26.04 (ADR-0018).
- `Next`/`Previous` исполняются по видимой очереди, и `src/media/mod.rs` рекламит
  `can_go_next`/`can_go_previous` ровно при непустой очереди — расхождение
  способностей и поведения устранено.
- `WindowMode` реализован в Core, IPC и UI (ADR-0021): `LayoutDensity` в
  `src/ui/layout.rs` даёт Focus полную раскладку, Overlay — капсулу из HUD-строки,
  мини-глобуса и избранного, Listening — строку now-playing (`src/ui/capsule.rs`).
- Смена режима при открытом окне стоит пересоздания UI-прогона, то есть той же
  цены, что показ окна (≈1 с, ADR-0018): `orbitfm mode overlay` моргает окном.
- Режим не переживает `quit`: он состояние `Session`, а не `SettingsStore`,
  поэтому процесс всегда стартует в `Focus` (если CLI явно не попросил другой).
- `TraySnapshot` не содержит громкости: у SNI нет витрины для неё, а
  `Volume Up/Down` — действие, а не состояние. Громкость по-прежнему видна только
  в `status` и MPRIS.

## 6. Тестирование

Unit-тесты лежат в модулах: `mod.rs` — молчание `WakeHandle` без слота и все
варианты `format_status`; `window.rs` — чередование `toggle`, отсутствие
намерения при `hide` закрытого окна, обе ветки смены режима, единичность
`reopen`, геометрия и заголовок каждого режима, изоляция пользовательского
размера в `Focus`; `commands.rs` — префикс `error` у неизвестной команды,
round-trip протокола и `WindowModeArg → WindowMode`; `playback.rs` — приоритет
локации над тегами в MPRIS;
`search.rs` — сброс контекста при смене режима, отбрасывание устаревшего ответа,
подпись рулетки, параметры запроса страны; `markers.rs` — группировка по
координатам, пропуск невалидных и `(0, 0)`, дедупликация по UUID, популярность =
`votes`, максимумы по группе. `tray.rs` — это тонкий dispatcher над существующими
методами, отдельного теста он требует только через живой SNI (ниже) и тесты
модуля `src/tray/`.

Живая проверка механизма — в ADR-0018 («Проверка реализации»): `hide` убирает
окно из раскладки niri, в скрытом состоянии работают `play`, `volume`, `status`,
`show` возвращает интерфейс со старым состоянием, `quit` завершает процесс без
паник. Живая проверка трея (SNI-регистрация, меню, клики, колесо, команды при
закрытом окне) — в ADR-0020 («Проверка реализации»).

## 7. Связанные документы

- ADR-0018: Скрытие окна = закрытие вьюпорта с перезапуском UI-цикла
- ADR-0020: Иконка в системном трее (StatusNotifierItem на ksni)
- ADR-0021: Режимы окна (`WindowMode`) и IPC-глагол `mode`
- `docs/interfaces/10-session.md` — формальный контракт `Session`
- `docs/modules/12-tray.md`, `docs/interfaces/11-tray.md` — модуль трея
- `docs/modules/07-app-core.md` — presentation-слой (`src/app.rs`, `src/ui/*`)
- `docs/architecture/00-overview.md` § 4, § 4.1 — состав Application Layer
- `docs/interfaces/05-ipc.md`, `docs/interfaces/06-media.md`,
  `docs/modules/09-library.md`, `docs/modules/10-settings.md`
