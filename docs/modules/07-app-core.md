# Модуль: Application Core

**Путь в коде:** `src/app.rs`, `src/main.rs`, `src/ui/*`
**Ответственность:** Связывание всех подсистем, состояние приложения, главный цикл egui, маршрутизация команд.

---

## 1. Назначение

Центральный оркестратор.  
Не содержит бизнес-логики доменов, только координацию.

## 2. Основные обязанности

- Инициализация всех модулей в правильном порядке
- Хранение shared state (видимость окна, текущий URL, флаг завершения)
- Маршрутизация `IpcCommand` и `MediaCommand` в соответствующие модули
- Реализация `eframe::App` (`logic` + `ui` в eframe 0.37)
- Управление видимостью окна (show/hide вместо destroy)

## 3. Состояние приложения

Текущее состояние (Этап 8, фазы 1–5):

```rust
pub struct AppState {
    pub window_visible: bool,
}

pub struct AppServices {
    pub audio: AudioEngine,
    pub geo: GeoService,
    pub radio: RadioBrowserClient,
    pub media: Option<MediaControls>,
    pub library: StationLibrary,
    pub settings: SettingsStore,
    pub runtime: tokio::runtime::Handle,
}

pub struct OrbitApp {
    state: AppState,
    commands: mpsc::Receiver<IpcRequest>,
    wake: WakeHandle,
    audio: AudioEngine,
    audio_events: broadcast::Receiver<AudioEvent>,
    geo: GeoService,
    radio: RadioBrowserClient,
    media: Option<MediaControls>,
    library: StationLibrary,
    settings: SettingsStore,
    globe_camera: GlobeCamera,
    globe_ready: bool,
    spectrum_overlay: SpectrumOverlay,
    callout: CalloutState,
    spectrum_enabled: bool,
    visualizer: VisualizerModel,
    current_url: Option<String>,
    current_station: Option<Station>,
    search_query: String,
    country_query: String,
    genre_query: String,
    search_context: SearchContext,
    search_results: Vec<Station>,
    markers: Vec<GlobeMarker>,
    library_tab: LibraryTab,
    country_filter: CountryFilterState,
    search_generation: u64,
    search_pending: bool,
    search_empty: bool,
    search_rx: Option<mpsc::UnboundedReceiver<SearchResponse>>,
    search_error: Option<String>,
    roulette_pending: bool,
    roulette_rx: Option<mpsc::UnboundedReceiver<Result<Station, RadioError>>>,
    panel_open: bool,
    favicons: FaviconCache,
    http: reqwest::Client,
    toast: Toast,
    runtime: tokio::runtime::Handle,
    quitting: bool,
}
```

`globe_ready` — результат создания `GlobeRenderer` из `CreationContext::wgpu_render_state`.
`spectrum_enabled` синхронизируется с аудио-ядром в `logic()`, `spectrum_overlay` хранит
сглаженное состояние обода глобуса, `visualizer` — уровни и пики столбцов
визуализатора (ADR-0019); ошибки включения фильтра показываются тостом.
`callout` хранит карточку страны (текущую и уходящую) с альфой, чтобы fade-in/out
переживал кадры (ТЗ §1.2).
`search_context` хранит режим (`SearchMode`) и выбранную страну или жанр;
смена режима очищает контекст. `country_query` — текст поиска страны в HUD,
`genre_query` — свой тег в popup жанра (оба очищаются после выбора);
`country_filter` — локальные фильтры и сортировка панели (`text`, `genre`,
`sort`, `compact_rows`), которые не делают сетевых запросов и сбрасываются при
новом выборе страны/жанра (плотность `compact_rows` переживает сброс фильтров). `search_generation` — счётчик поколений: каждый запуск поиска
увеличивает его, а `SearchResponse` несёт номер, по которому `poll_search`
отбрасывает устаревшие ответы. `search_empty` отмечает успешный ответ без станций.
`roulette_pending` блокирует повторный запуск Roulette, `roulette_rx` принимает
результат фонового `random_station`.
`SearchOutcome` — внутренний тип с результатами основного и гео-запроса;
`GlobeMarker` (в `ui/globe_view.rs`) хранит представителя группы, координаты,
количество станций, список UUID участников, максимальный по группе сигнал
популярности (`votes`) для размера и максимальный битрейт для цвета. Маркер
считается играющим, если текущий `stationuuid` входит в `members`: в одном
городском узле станций больше одной, и пульс должен ловиться на любой из них.

`current_url` заполняется из `AudioEvent::Started` и из запуска станции; `Stopped`/`Error`
его больше не очищают — последний URL нужен MPRIS для повторного запуска.
`current_station` хранит метаданные активной станции и источник для `status` и MPRIS.
`media` — `Option`, потому что MPRIS может быть недоступен (нет D-Bus); приложение в этом
случае продолжает работать.

`library` хранит избранное и историю (см. §9), `library_tab` — активную вкладку списка
(`Results`/`Favorites`/`History`, по умолчанию `Results`). Ошибки чтения/записи
библиотеки и настроек показываются тостами и не остаются в состоянии приложения.

`settings` хранит тему и акцент (см. §10). `panel_open` управляет видимостью панели
станций, `toast` — одно текущее сообщение.

### 3.1. UI-слой (Фазы 1–6)

UI вынесен из `app.rs` в `src/ui/`:

```
src/ui/
├── mod.rs            # SearchMode, UiAction, playback_label, single_line, truncate
├── palette.rs        # Palette + токены ТЗ §3.1 (space_top/space_bottom, glass_top/glass_bottom,
│                     # rim_light), configure_style, theme_* / accent_*, heat_color — рампа §3.2
├── layout.rs         # DeskLayout: геометрия плавающих зон и blocks_point для глобуса
├── widgets.rs        # кастомные контролы: pill, chip, search_field, tab_bar, live_badge,
│                     # eq_bars_in, viz_bars, volume_slider, circle_button, avatar, logo_mark,
│                     # flag_emoji, glass_panel (стеклянный градиент), faded
├── images.rs         # FaviconCache: машина состояний Queued/Loading/Ready/Failed (ADR-0016)
├── hud.rs            # верхний HUD: логотип, Country/Genre, поиск станции/страны, now-playing
│                     # с favicon, live-бейдж, тема-луна, шестерёнка, кнопки списка/?/скрытия
├── mode_switch.rs    # pill-переключатель «Country | Genre» и popup выбора жанра
├── station_filter.rs # локальные фильтры: текст, жанр, сортировка, плотность compact_rows
├── station_panel.rs  # правая плавающая панель: вкладки, фильтры, список станций
├── station_row.rs    # единая строка списка: favicon-аватар, имя, город, теги, битрейт, звезда
├── transport.rs      # нижний transport: now-playing, prev/next, play/pause, stop, heart,
│                     # Radio Roulette, codec/kbps, громкость, столбцы визуализатора и чип
├── overlays.rs       # help-карточка управления и легенда «Station density»
├── globe_view.rs     # ввод камеры, маркеры и вес, heatmap стран, country callout,
│                     # звёздное небо, pulse обода
├── toast.rs          # одно сообщение на 3 секунды
└── icons.rs          # векторные иконки: star, heart, dice, gear, moon, speaker, mouse, skip
```

Правила:

- UI-модули получают только нужные срезы состояния, возвращают `Vec<UiAction>`;
- текстовые поля поиска станции/страны, активная вкладка и фильтры страны
  редактируются напрямую через `&mut`;
- UI не вызывает `AudioEngine`, `RadioBrowserClient`, `StationLibrary` и `SettingsStore`
  (поиск страны идёт через `GeoService::search_countries` — это чистый offline-запрос;
  загрузка фавиконок — состояние в `FaviconCache`, сетевые запросы спавнит `OrbitApp`);
- надписи интерфейса — на английском (решение фазы 6);
- `app.rs` — единственная точка побочных эффектов: `apply_actions` выполняет
  `Play`, `ToggleFavorite`, `SelectCountry`, `SwitchMode`, `SelectGenre`,
  `RunSearch`, `Roulette`, `ClearHistory`, `SetTheme`, `SetAccent`,
  `ToggleSpectrum`, `SetVolume`, `TogglePlayback`, `Stop`, `PlayRelative`,
  `Hide`, `Quit`, `TogglePanel`.

`UiAction` — внутренний тип, не публичный контракт. `render` по-прежнему не импортирует
доменные типы: `globe_view` передаёт в `paint_globe` камеру и готовые маркеры.

### 3.2. Раскладка «Orbital Desk»

Глобус занимает всё окно (`CentralPanel` без отступов, full-bleed со звёздным
небом), поверх него в том же кадре рисуются плавающие зоны — `ui.scope_builder` с
`max_rect` из `DeskLayout::compute(viewport, panel_open)`; порядок отрисовки равен
порядку слоёв. Геометрия (`src/ui/layout.rs`): отступ 14 px, зазор 12 px,
HUD — 58 px сверху, панель — 312 px справа, transport — 68 px снизу, help-карточка
216×108 слева сверху, легенда 190×58 слева снизу. `DeskLayout::blocks_point`
отсекает клики и колесо глобуса внутри плавающих зон.

| Зона | Содержимое |
|------|------------|
| HUD | логотип, `Country/Genre`, поиск (у страны — popup подсказок), now-playing с favicon, LIVE, громкость, луна, шестерёнка, кнопка списка |
| Панель | `Results / Favorites / History` с иконками, локальные фильтры (поле, чипы, `Sort by`, `Compact`), строки станций |
| Transport | favicon 44 px, имя/город/`Now Playing: теги`, `⏮ play ⏭`, stop, heart, `Radio Roulette`, `CODEC kbps`, мини-спектр, чип `FFT` |
| Help-карточка | мыши-пиктограммы: Right-drag — orbit, Left click — country/station, Scroll — zoom |
| Легенда | `Station density` градиент Low→High |

Иконки рисуются через `Painter` в `ui/icons.rs` и кастомных виджетах `ui/widgets.rs`
(треугольники, дуги из `Shape::line`, mesh) и не зависят от набора глифов шрифта egui.

`Toast` показывает одно сообщение 3 секунды внизу по центру над transport.
Help-карточка видима постоянно (авто-скрываемая подсказка первого запуска удалена
в фазе 6).

## 4. Порядок инициализации

```
IpcServer::try_bind → AudioEngine::new → GeoService::from_embedded
→ MediaControls::new(wake) → RadioBrowserClient::new → StationLibrary::load_default
→ SettingsStore::load_default → фоновый refresh_servers → задача audio events
→ wake → IpcServer::run → eframe → OrbitApp::new
```

`try_bind` выполняется до создания аудио-ядра, чтобы второй экземпляр завершился, не
занимая аудиоустройство. Ошибки `AudioEngine::new` и загрузки гео-датасета фатальны;
ошибки MPRIS, библиотеки, настроек и прогрева каталога — только предупреждения. При
ошибке загрузки библиотеки создаётся `StationLibrary::ephemeral()`: изменения работают
до завершения процесса, повреждённый файл не перезаписывается. Аналогично ошибка
загрузки настроек даёт `SettingsStore::ephemeral()` с темой System и акцентом Blue.

`OrbitApp::new` применяет настройки оформления к `egui::Context`, создаёт
`GlobeRenderer` из `cc.wgpu_render_state` и вставляет его в
`render_state.renderer.write().callback_resources`. При отсутствии wgpu-состояния
приложение работает без глобуса (`globe_ready = false`).

## 5. Главный цикл (логика)

Каждый `logic()`:

1. Разбирает накопившиеся `AudioEvent`.
2. Обрабатывает очередь `IpcRequest` и отвечает через `IpcRequest::try_respond`.
3. Разбирает `MediaCommand` от MPRIS.
4. Забирает результат фонового поиска (`poll_search`).
5. Забирает результат Roulette (`poll_roulette`).
6. Синхронизирует состояние с MPRIS (`sync_media`).
7. Перехватывает закрытие окна (`close_requested`) и превращает его в Hide, если это не
   `quit`.

`ui()` рисует раскладку «Orbital Desk» только при видимом окне. В eframe 0.37 скрытое окно не
вызывает `ui()`, но `logic()` продолжает выполняться при запрошенном repaint. Поэтому
IPC- и MPRIS-команды обрабатываются и в скрытом состоянии; пробуждение выполняют
`WakeFn` из IPC-сервера, MPRIS-колбэка, задача аудио-событий и завершённый поиск.

`draw()` собирает `Vec<UiAction>` от `hud`, `globe_view`, `station_panel` и `transport`,
затем применяет их в `apply_actions`; побочные эффекты выполняются только после
отрисовки кадра.

## 6. Глобус, маркеры и выбор страны

Зону контента делит `center`-область: `globe_view::show` получает прямоугольник
глобуса, камеру, маркеры, состояние спектра и `&mut CalloutState`, а возвращает
`GlobeResponse { actions, clicked_point, interacted }`.

1. ПКМ + drag → `GlobeInput::drag_delta`; колесо при наведении → `scroll_delta`;
   ЛКМ → `primary_clicked`.
2. `GlobeCamera::update(stable_dt, &input)` обновляет ориентацию, инерцию и зум.
3. Hover страны (только в режиме «Страна»): `screen_to_sphere(hover_pos)` →
   `country_at_sphere_point` (при ПКМ-drag hover отключается). `globe_view` рисует
   контур наведённой страны; выбранная страна получает акцентный контур 2 px и
   заливку-heatmap с альфой 26..96 от `log10` количества загруженных станций
   (`GlobeMarker.count` по ISO-коду). Курсор `PointingHand` — над страной или
   маркером, `Grabbing` — при вращении.4. При клике сначала проверяется ближайший маркер в радиусе 10 px: он приоритетнее
   выбора страны и возвращает `UiAction::Play`.
5. Если маркера нет и режим — «Страна», `screen_to_sphere` даёт точку сферы;
   `app.rs` передаёт её в `GeoService::country_at_sphere_point` и при попадании
   создаёт `UiAction::SelectCountry`. Тот же путь используется для выбора из HUD.
   В режиме «Жанр» проверка страны отключена (`country_enabled = false`), поэтому
   клик по суше не меняет контекст.
6. `apply_actions` вызывает `focus_country`: `country_shape(...).focus_point()`
   запускает `GlobeCamera::fly_to` на 650 мс (центр крупнейшего кольца, корректно
   на антимеридиане). Затем `start_country_search` активирует страну в
   `search_context`, сбрасывает локальные фильтры и запрашивает
   `countrycode=<ISO alpha-2>`, `limit = 100`, `hide_broken = true`.
7. Маркеры проецируются через `project_on_sphere`; наведение показывает двухстрочную
   подпись по ТЗ §2.4 — имя (для группы `· +N`) и `место · kbps · кодек` — и
   курсор-указатель, станция в эфире получает `colors.active`, кольцо и пульс.
   `paint_globe` добавляет paint callback с `spectrum_overlay.pulse()`, затем
   `paint_country_highlights` и `paint_markers` рисуют egui-оверлей; repaint
   запрашивается, пока камера `is_animating`, пульсирует обод или на сфере виден
   играющий маркер.
8. Country callout (ТЗ §1): активная карточка — выбранная страна (`alpha → 1.0`),
   иначе наведённая (`alpha → 0.85`), в режиме «Жанр» — `None`. `update_callout`
   ведёт две карточки в `CalloutState` (уходящая догорает до `CALLOUT_EPSILON`),
   экспоненциально с постоянной `CALLOUT_FADE_SECONDS = 0.15` с
   `stable_dt`; при недосчитанной альфе запрашивается repaint через 16 мс.
   `paint_country_callout` берёт якорь `country_shape(iso).focus_point()`,
   проецирует его `project_on_sphere` (страна на задней полусфере карточку не
   даёт), рисует leader line 1 px с `accent` при α 0.75, пин `live` с кольцом и
   стеклянную карточку: флаг `flag_emoji(iso)`, название 13.5 px, подзаголовок
   «{N} stations» (N — `GlobeMarker.count` по ISO, честный ноль), скругление
   11 px, slide `(1 − alpha) · 6 px` от якоря. Карточка ставится над якорем и
   переворачивается под него, если сверху не хватает места, и клампится в зону
   между `layout.hud`, `layout.transport` и панелью станций.

Клик по океану или территории без валидного ISO-кода ничего не меняет. При
`globe_ready = false` вместо глобуса рисуется заглушка, ввод игнорируется.

## 7. Аудиовизуализатор

- В правой части transport — столбцы `viz_bars` (оценка огибающей спектра, не
  бинный FFT — ADR-0019) и чип `Visualizer`; рядом чипа метрик кадра больше нет.
- Чип вызывает `AudioEngine::set_spectrum_enabled` через `runtime.block_on`;
  ошибка показывается тостом и не прерывает воспроизведение.
- Каждый кадр `logic()` синхронизирует `spectrum_enabled` с движком, а
  `AudioEngine::spectrum()` преобразует в `SpectrumTarget`: он питает и
  `SpectrumOverlay::update` (обод глобуса), и `VisualizerModel::advance`.
  Модель живёт в `AppState`, отрисовку делает `ui::widgets::viz_bars` с
  градиентом зелёный → жёлтый → красный (ТЗ §3.3).
- `advance` считается при `Playing`/`Buffering`/`Reconnecting` и включённом
  фильтре; иначе цели заменяются idle-«дыханием». Число столбцов берётся из
  `Settings::viz_bars` (16/24/32, по умолчанию 24) и меняется действием
  `UiAction::SetVizBars` — тогда модель пересоздаётся.
- `SpectrumOverlay::pulse()` (`0.75·level + 0.25·flux`, 0 при выключенном
  фильтре) передаётся в `paint_globe` до отрисовки: атмосфера пульсирует при
  `Playing` и затухает при `Stop` или выключенном визуализаторе.
- Пока визуализатор или обод живы, `logic()` запрашивает repaint с интервалом
  ~33 мс.
- IPC-команды для визуализатора нет: управление доступно только из GUI. Полосы
  спектра по бинам по-прежнему не поддерживаются (ADR-0010, ADR-0019).

## 8. Поиск и запуск станций

- «Найти»/Enter в свободном контексте запускают `RadioBrowserClient::search`
  с `name`, `limit = 100`, `hide_broken = true`; выбранная страна/жанр при этом
  сбрасываются. Если контекст активен, поле поиска применяется локально к уже
  загруженному списку (`country_filter.text`), без сетевого запроса. Запрос
  выполняется в tokio-задаче, результат приходит через `mpsc` и будит GUI.
- Параллельно с основным запросом выполняется гео-запрос с теми же фильтрами,
  `has_geo_info = true`; лимиты — 100 для названия/страны и 200 для жанра как для
  списка, так и для маркеров. Ошибка гео-запроса логируется и не влияет на список;
  результат используется только для маркеров.
- Маркеры строятся из гео-станций: координаты должны быть конечными, лежать в
  `[-90, 90]`/`[-180, 180]` и не быть `(0, 0)`; станции с координатами, округлёнными
  до 2 знаков, объединяются, дубликаты по UUID отбрасываются.
- Вес маркера считается из голосов (sqrt-сжатие к 5000) и битрейта (линейно к 320)
  со смесью 70/30, ограничивается `[0, 1]`; у группы берётся максимум. Радиус —
  `3.5 + 3.5·weight`, активная станция получает +2 px, свечение растёт с весом.
- Поиск по стране использует тот же канал и отличается только `SearchParams`
  (`country_code`, без `name`, `limit = 100`); гео-запрос для маркеров тоже
  выполняется с `limit = 100`. Заголовок `Страна: …` остаётся до следующего
  поиска или смены режима.
- Поиск по жанру: `tag = <тег>`, `limit = 200`, гео-лимит 200. Чипы пресетов и
  поле своего тега находятся в popup `ui/mode_switch.rs`; выбор запускает
  `start_genre_search`, который очищает `genre_query` и сбрасывает локальные
  фильтры. Заголовок панели — `Genre: …`; станции без координат остаются в списке.
- `SearchMode` переключается только кнопками HUD; смена режима очищает контекст,
  результаты, маркеры, фильтры и вкладку `Results`.
- Каждый запуск поиска увеличивает `search_generation`; `SearchResponse` несёт
  номер поколения, `poll_search` отбрасывает ответы прошлого режима/жанра и не
  возвращает устаревшие маркеры.
- HUD «Страна» вызывает `GeoService::search_countries` (до 6 подсказок) при каждом
  изменении запроса; Enter выбирает первую, клик — выбранную. Оба пути создают
  `UiAction::SelectCountry`, очищают `country_query` и после выбора сбрасывают
  `country_filter`.
- Панель страны применяет `station_filter::filtered_results` к уже загруженным
  результатам: текст ищется в `name/tags/country/state`, жанровые чипы проверяют
  теги без учёта регистра, сортировка — по `votes`, `bitrate` или имени. Это
  локальные операции без сети; карточки показывают state/country, kbps, codec,
  теги и голоса. При пустом результате фильтров — «В этом секторе пока тихо» и
  кнопка «Сбросить фильтры», сохраняющая выбранную сортировку.
- Пустой успешный ответ выставляет `search_empty`; UI показывает «Станции не найдены».
- Новый поиск очищает маркеры; ошибка основного запроса также очищает их.
- Выбор результата или IPC `play <uuid>` вызывает `start_station`:
  проверяется непустой `url_resolved`, выполняется `audio.play`, сохраняются
  `current_url`/`current_station`, асинхронно отправляется click.
- `play <uuid>` сначала ищет станцию в последних результатах поиска, затем вызывает
  `get_station` с таймаутом 8 с; таймаут маппится в `RadioError::Network`.

### 8.1. Roulette

- Кнопка `Radio Roulette` в transport вызывает `start_roulette`; пока `roulette_pending`,
  повторные нажатия игнорируются, кнопка заблокирована.
- `RadioBrowserClient::random_station` выполняется в tokio-задаче (до трёх попыток,
  станция обязана иметь поток и координаты); результат приходит через `mpsc` и будит GUI.
- `poll_roulette` запускает станцию, при успехе перелетает камерой за 900 мс
  (`ROULETTE_FLY_MS`) к её координатам и показывает тост `Roulette: имя — страна`.
  Ошибка каталога или запуска показывается тостом и не меняет текущий эфир.
- Запуск идёт через `start_station`: станция попадает в историю, отправляется click,
  обновляет MPRIS; перетаскивание отменяет перелёт, как и у выбора страны.
- IPC-команды Roulette нет: действие доступно только из GUI (ADR-0015).

### 8.2. Prev/Next по видимому списку

- Очередь навигации не хранится постоянно: `visible_queue()` каждый раз снимает
  снапшот текущего видимого списка — вкладка `Results` (с учётом локальных
  фильтров и сортировки в контекстном режиме), иначе `Favorites` или `History`.
- `play_relative(step)` находит позицию текущей станции в снапшоте и делает шаг
  со `rem_euclid` (циклом); если текущей станции в списке нет — `+1` даёт первую,
  `-1` последнюю; пустой список — no-op.
- Источники действия: кнопки `⏮`/`⏭` в transport (`UiAction::PlayRelative`) и
  MPRIS `Next`/`Previous`; обе маршрутизации ведут в один метод.

## 9. Избранное и история

- `StationLibrary` загружается в `main` до запуска GUI; ошибка загрузки даёт
  эфемерную библиотеку без файла (ADR-0013).
- Успешный `start_station` (строка результатов, избранного или истории, маркер,
  Roulette, IPC `play`) вызывает `record_played`: станция перемещается в начало истории,
  история ограничена 20 записями. Возобновление из MPRIS (`Play`/`PlayPause`)
  историю не меняет.
- Переключение звезды вызывает `toggle_favorite` и запрашивает repaint; звезда есть
  у текущей станции (heart в transport) и у каждой строки списка (звезда в панели). Ошибка сохранения
  показывается тостом.
- Под фильтрами показываются вкладки `Results`/`Favorites (N)`/`History (N)`, если
  есть результаты или сохранённые станции. Список прокручивается на всю доступную
  высоту; клик по строке запускает станцию, клик по звезде — меняет избранное.
- Новый поиск сбрасывает вкладку на `Results`; на вкладке `History` есть кнопка
  `Clear history` (`clear_history`) с подтверждением тостом.
- Ошибки библиотеки логируются и показываются тостом; они не прерывают
  воспроизведение. Пути и формат файла описаны в `docs/modules/09-library.md`.

## 10. Оформление

- `SettingsStore` загружается в `main` до запуска GUI; ошибка чтения даёт эфемерное
  хранилище с настройками по умолчанию, повреждённый файл не перезаписывается
  (ADR-0014).
- При старте и после каждого изменения `configure_style` применяет к
  `egui::Context`: `ThemePreference` (System/Dark/Light), прозрачные заливки панелей
  и окна (нужно для overlay), цвет выделения `selection.bg_fill` из акцента,
  контрастную обводку выделения и цвет гиперссылок.
- Блок «Настройки» в HUD: пункты темы «Системная / Тёмная / Светлая», шесть пресетов
  акцента (Орбита, Изумруд, Янтарь, Фиалка, Роза, Океан), «Скрыть окно» и «Выход».
  Изменение применяется сразу и сохраняется через `SettingsStore`; ошибка записи
  показывается тостом и не прерывает воспроизведение.
- `Palette` и `configure_style` живут в `ui/palette.rs`. `Palette::for_ui(theme, accent)`
  задаёт цвета панелей, обводки, приглушённого и ошибочного текста, подложек и
  глобуса. В тёмной теме акцент совпадает с цветом маркеров; в светлой — затемняется
  для контраста с фоном, а маркеры остаются яркими.
- Токены фона и стекла (ТЗ §3.1): `space_top`/`space_bottom` — вертикальный градиент
  космоса, `glass_top`/`glass_bottom` и `rim_light` — стеклянные панели,
  `heat_color(t)` — общая рампа плотности §3.2 для заливки стран и легенды.
  `Frame` egui умеет только сплошную заливку, поэтому `palette.glass()`/`panel()`
  возвращают прозрачные рамки, а фон рисует `widgets::glass_panel` — восемь полос
  градиента, верхний блик и мягкая рамка — до содержимого зоны.
- Обычные маркеры получают `palette.marker`, активный маркер сохраняет тёплый
  янтарный цвет.
- IPC-команд для оформления нет: настройка доступна только из GUI.

## 11. Синхронизация с MPRIS

`sync_media()` вызывается каждый кадр, но `MediaControls::set_playback` дедуплицирует
неизменившееся состояние. Соответствие:

- `Stopped`/`Error` → трек отсутствует, `Stopped`;
- `Paused` → трек есть, `Paused`;
- `Playing`/`Buffering`/`Reconnecting` → трек есть, `Playing`.

Метаданные берутся из `current_station`: `name` → title, `favicon` → artUrl,
`url_resolved` → `xesam:url`. Artist — «штат, страна» (или одно из полей), при
отсутствии локации — теги станции; пустые значения отбрасываются. Без станции
используется URL потока.

## 12. Жизненный цикл окна

- Закрытие крестиком или по `Super+Q` в WM → `ViewportCommand::CancelClose` + Hide
  (`window_visible = false`). На Wayland `Visible` — no-op, поэтому скрытие заодно сжимает
  поверхность до 1×1: `MinInnerSize([1,1])` → `InnerSize([1,1])` → `Visible(false)`
  (ADR-0018). Окно перестаёт занимать место в раскладке композитора и перехватывать ввод,
  процесс и воспроизведение остаются живыми.
- Показ → `Visible(true)` → `MinInnerSize([720,480])` → `InnerSize(сохранённый размер)` →
  `Minimized(false)` → `Focus`. Размер до скрытия хранится в `AppState::restore_size`;
  если он неизвестен, берётся `crate::WINDOW_SIZE` (920×660).
- `toggle` / `show` / `hide` из IPC и клик по виджету Noctalia идут через ту же
  `set_window_visible`, поэтому виджет работает как toggle.
- Кнопка `×` в HUD — это Hide, а не выход (tooltip «Hide window (audio keeps playing)」);
  `?` открывает/закрывает карточку управления. Выход остаётся отдельным пунктом
  (`Quit` в попапе шестерёнки и команда IPC).
- Команда `quit` → `quitting = true` + `ViewportCommand::Close`; после выхода из GUI main
  останавливает IPC-сервер, аудио-поток и удаляет сокет

## 13. Текущие ограничения

- Heatmap считает только загруженные станции выбранной/наведённой страны; глобального
  aggregate по всем странам нет (отдельное решение, ADR-0015)
- Roulette доступна только из GUI и всегда запускает воспроизведение; «только
  показать станцию без эфира» не поддерживается
- Вес маркера учитывает только голоса и битрейт; популярность жанра и свежесть
  станции не участвуют
- Hover страны вычисляется raycast-ом каждый кадр без кэша последней точки
- `Next`/`Previous` из MPRIS и кнопки `⏮`/`⏭` в transport идут по текущему видимому
  списку (`play_relative(±1)` с циклом): очередь — снимок панели, а не постоянный
  плейлист (ADR-0013 переопределён фазой 6 для этого поведения)
- На глобусе нет подписей стран, атмосфера ограничена Fresnel-ободом; маркеры
  группируются только по совпадающим координатам, кластеризации близких точек нет
- Клик мимо окна (overlay) ещё не реализован
- Кэш станций каталога только in-memory; на диске хранятся лишь избранное и история
- Метаданные ICY (название трека внутри потока) не читаются
- FFT-визуализатор управляется только из GUI, не имеет IPC-команды и показывает
  агрегированные метрики вместо полос спектра
- Акцент выбирается только из шести пресетов, произвольный цвет не поддерживается
  (ADR-0014); тема System следует системной теме, которую сообщает eframe
- Светлая палитра производная от `egui::Theme::Light`; отдельной ручной настройки
  контрастности нет
- Шрифт egui не содержит CJK: имена станций на китайском и японском отображаются
  квадратами (замена шрифта вне Фазы 1)

## 14. Связанные документы

- ADR-0002, ADR-0005, ADR-0006, ADR-0010, ADR-0012, ADR-0013, ADR-0014, ADR-0015,
  ADR-0016, ADR-0017
- `docs/architecture/00-overview.md`
- `docs/ui/design-vision.md`, `docs/ui/implementation-plan.md`
- `docs/modules/09-library.md`
- `docs/modules/10-settings.md`
