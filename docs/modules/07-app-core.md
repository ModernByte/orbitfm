# Модуль: Presentation (Orbital Desk)

**Путь в коде:** `src/app.rs`, `src/ui/*`, цикл запуска окна — `src/main.rs` (`run_session`,
`run_window`)
**Ответственность:** отрисовка кадра, ввод, перевод `UiAction` в вызовы `Session`;
ресурсы одного прогона UI-цикла.

---

## 1. Назначение

Presentation-слой: всё, что знает про egui и wgpu. Не содержит бизнес-логики доменов
и долгоживущего состояния — только кадры, стеклянные зоны и превращение ввода в команды
ядра.

Модель окна определена ADR-0018: на Wayland честное «close = hide» возможно лишь реальным
закрытием root-вьюпорта с повторным запуском `eframe::run_native` из главного потока.
Поэтому `OrbitApp<'a>` заимствует `&'a mut Session` ровно на время одного прогона и
создаётся заново при каждом показе, а подсистемы, доменные данные и состояние окна
принадлежат Application Core (`src/session/`, контракт — `docs/modules/11-session.md`
и `docs/interfaces/10-session.md`).

## 2. Основные обязанности

- Поднять подсистемы в корректном порядке и передать их в `Session` как `AppServices`
  (`launch` в `src/main.rs`); при выходе остановить IPC-сервер.
- Вести цикл показа/скрытия окна (`run_session`): `restore_size` → `begin_window` →
  `run_window` → `end_window` → `is_quitting` → `park`.
- На каждый прогон собрать per-run ресурсы в `OrbitApp::new`: тему и акцент
  (`palette::configure_style`), цветной emoji-шрифт, `GlobeRenderer` в `callback_resources`,
  `FaviconCache`, `reqwest::Client`, `VisualizerModel`, `Toast`, и установить `WakeHandle`
  (`wake.install` замыкания, зовущего `Context::request_repaint`).
- Реализовать `eframe::App` (`logic` + `ui` + `clear_color` в eframe 0.37): фоновое
  обслуживание через `Session::tick`, сборка кадра, применение `UiAction`.
- Транслировать `UiIntent` ядра в `ViewportCommand` (`Focus`/`Close`) и сообщать ядру
  размер окна из `inner_rect`.
- Показывать `Notice` ядра тостом; ошибки доменных операций не хранятся в UI.
- Маршрутизацией `IpcCommand` и `MediaCommand` занимается `Session` — здесь только
  очереди, которые ядро само читает (из `tick()` или из `park()`).

## 3. Состояние прогона (per-run)

Текущее состояние (Этап 8, фазы 1–6 + ADR-0018):

```rust
pub(crate) struct OrbitApp<'a> {
    session: &'a mut Session,
    globe_ready: bool,
    spectrum_overlay: SpectrumOverlay,
    /// Аудио-деформация планеты на время показа окна (ADR-0028).
    motion: MotionMixer,
    /// Орбитальное аудиокольцо Focus (ADR-0029).
    orbit_ring: OrbitRing,
    /// Callout выбранной/наведённой страны с fade-переходами.
    callout: CalloutState,
    visualizer: VisualizerModel,
    favicons: FaviconCache,
    http: reqwest::Client,
    toast: Toast,
}
```

- `session` — единственный источник доменных данных и подсистем; заимствование живёт
  ровно один прогон (`Box::new(|cc| Ok(Box::new(OrbitApp::new(cc, session))))` в
  `run_window`).
- `globe_ready` — результат создания `GlobeRenderer` из
  `CreationContext::wgpu_render_state`; без него глобус заменяется заглушкой, а ввод
  игнорируется. Признак per-run, потому что wgpu-ресурсы умирают вместе с прогоном.
- `spectrum_overlay` — сглаженные level/flux эфира для кольца Listening (живёт кадрами).
- `motion` — `MotionMixer` (ADR-0028): spring-squash, idle-дыхание и огибающие
  полос. Обновляется один раз за кадр в `draw_layers`; результат
  `GlobeMotion` получают шейдер, страны, маркеры, hit-test и callout. Как и
  остальные per-run ресурсы, не переживает скрытие окна.
- `orbit_ring` — модель и painter кругового аудиокольца за планетой (ADR-0029):
  presence, сглаженные метрики, 120 столбцов, комета и искры; живёт кадрами рядом
  с `SpectrumOverlay` и не переживает скрытие окна.
- `callout` — карточка страны (текущая и уходящая) с альфой, чтобы fade-in/out переживал
  кадры (ТЗ §1.2).
- `visualizer` — уровни, пики и фазы столбцов визуализатора (ADR-0022); строится по
  `Settings::viz_bars` и пересоздаётся действием `UiAction::SetVizBars`.
- `favicons` — `FaviconCache`: машина состояний Queued/Loading/Ready/Failed и текстуры,
  привязанные к `egui::Context` этого прогона (ADR-0016). После закрытия окна пуст —
  при следующем показе иконки грузятся заново. Декодируются только PNG/JPEG/WebP
  до 512 px по стороне и 512 КиБ; SVG/GIF/ICO отклоняются (ADR-0025).
- `http` — `reqwest::Client` с `USER_AGENT`, таймаутом 6 с и лимитом 3 редиректов;
  для favicon проверяются `http(s)`-схема, MIME-allowlist и потоковый лимит тела
  512 КиБ (ADR-0025).
- `toast` — одно текущее сообщение (3 секунды).

Всё остальное живёт в `Session`: подсистемы (`audio`, `geo`, `radio`, `media`, `library`,
`settings`, `runtime`), `globe_camera`, `visualizer_enabled`, `spectrum_enabled`,
`current_url`/`current_station`,
`search: SearchState` (строки запросов, `context`, `results`, `generation`, `pending`,
`empty`, `error`, каналы поиска и рулетки), `markers`, `library_tab`, `country_filter`,
`panel_open`, `show_help`, `window: WindowState` (видимость, размер, `intent`, `quitting`),
`notices`, `wake`, `commands`, `audio_events`. `visualizer_enabled` — пользовательский
переключатель визуализатора и кольца, инициализируется значением
`audio.spectrum_enabled()` (по умолчанию `true`); `spectrum_enabled` — фактическое
состояние аудио-анализа, которое включается и тогда, когда визуализатор выключен,
но движение глобуса требует полос. `Session::tick()` каждый кадр зеркалит
`spectrum_enabled` из движка, а ошибки включения анализа доходят до UI через
результат метода и показываются тостом.

Читаемое состояние на кадр снимает `Frame` — зоны берут данные из него, а не из ядра:

```rust
struct Frame {
    palette: Palette,
    settings: Settings,
    playback: PlaybackState,
    volume: f64,
    markers_total: usize,
    current_favorite: bool,
    can_resume: bool,
    spectrum_target: Option<SpectrumTarget>,
    /// Эффективная реактивность движения с поправкой на режим окна.
    motion_reactivity: MotionReactivity,
    motion_style: MotionStyle,
    /// Клик по сфере выбирает страну только в режиме страны (ТЗ §5.2).
    country_mode: bool,
    layout: DeskLayout,
}
```

`Frame::capture(&ctx, session, &mut visualizer, viewport)` вызывается до отрисовки зон:
`Palette::for_ui(ctx.theme(), accent)`, `session.settings.settings()`,
`session.audio.state()`, `session.audio.volume()`, `session.audio.spectrum()` →
`SpectrumTarget` (шесть агрегатов и `bands` через `BandTargets::from_db`),
`session.motion_reactivity()` и `settings.motion_style` для движения, сумма
`marker.count` в `markers_total`, `library.is_favorite` для
текущей станции, `current_url.is_some()` в `can_resume`,
`search.context.mode == SearchMode::Country` в `country_mode` и
`DeskLayout::compute(viewport, panel_open, show_help, session.window_mode())` в
`layout`. Там же считается шаг
анимации визуализатора (`advance` по `stable_dt` и `time`). `GlobeMarker` (в
`src/session/markers.rs`) хранит представителя группы, координаты, количество станций,
список UUID участников, максимальный по группе сигнал популярности (`votes`) и
максимальный битрейт — оба уходят только в подпись при наведении. Маркер считается играющим, если текущий `stationuuid`
входит в `members`: в одном городском узле станций больше одной, и пульс должен ловиться
на любой из них.

`SearchMode` определён в `src/session/search.rs` (режим выбирает ядро), `src/ui/mod.rs`
делает только ре-экспорт; `LibraryTab` определён в `src/session/mod.rs` и импортируется
в `src/ui/station_panel.rs`.

### 3.1. UI-слой (Фазы 1–6)

UI вынесен из `app.rs` в `src/ui/`:

```
src/ui/
├── mod.rs            # UiAction, playback_label, single_line, truncate, country_label;
│                     # SearchMode — только ре-экспорт из crate::session::search
├── palette.rs        # Palette + токены ТЗ §3.1 (space_top/space_bottom, glass_top/glass_bottom,
│                     # rim_light), configure_style, theme_preference, theme_label / accent_label,
│                     # accent_color, mix_colors, heat_color — рампа §3.2
├── layout.rs         # DeskLayout: геометрия плавающих зон, LayoutDensity из WindowMode,
│                     # blocks_point для глобуса
├── capsule.rs        # капсулы Overlay (top, мини-глобус, избранное) и Listening
├── widgets.rs        # кастомные контролы: pill, chip, search_field, tab_bar, live_badge,
│                     # eq_bars_in, viz_bars, volume_slider (клавиатурный шаг), circle_button,
│                     # avatar, logo_mark, flag_emoji, glass_panel (стеклянный градиент),
│                     # focus_ring/circle_focus_ring, faded
├── images.rs         # FaviconCache: машина состояний Queued/Loading/Ready/Failed (ADR-0016)
├── hud.rs            # верхний HUD: логотип, Country/Genre, поиск станции/страны, now-playing
│                     # с favicon, live-бейдж, тема-луна, шестерёнка, кнопки списка/?/скрытия
├── mode_switch.rs    # pill-переключатель «Country | Genre» и popup выбора жанра
├── station_filter.rs # локальные фильтры: текст, жанр, сортировка, плотность compact_rows
├── station_panel.rs  # правая плавающая панель: вкладки (LibraryTab из crate::session),
│                     # фильтры, список станций
├── station_row.rs    # единая строка списка: favicon-аватар, имя, город, теги, битрейт,
│                     # звезда; фокус, ↑/↓ по видимому списку, Space/Enter
├── transport.rs      # нижний transport: now-playing, prev/next, play/pause, stop, heart,
│                     # Radio Roulette, codec/kbps, громкость, столбцы визуализатора и чип
├── overlays.rs       # help-карточка управления и легенда «Station density»
├── globe_view.rs     # ввод камеры, проекция и вид маркеров, heatmap стран, country callout,
│                     # звёздное небо, статичный Fresnel-обод
├── toast.rs          # одно сообщение на 3 секунды
└── icons.rs          # векторные иконки: star, heart, dice, gear, moon, speaker, mouse, skip
```

Правила:

- UI-модули получают только нужные срезы состояния, каждая зона возвращает
  `Vec<UiAction>` (`.inner` своего `ui.scope_builder`);
- текстовые поля поиска станции/страны (`session.search.query`,
  `session.search.country_query`, `session.search.genre_query`), активная вкладка
  (`session.library_tab`) и фильтры страны (`session.country_filter`) редактируются
  напрямую через `&mut`;
- UI не вызывает `AudioEngine`, `RadioBrowserClient`, `StationLibrary` и `SettingsStore`
  (поиск страны идёт через `GeoService::search_countries`, выбор страны под курсором —
  через `GeoService::country_at_sphere_point`: это чистые offline-запросы; загрузка
  фавиконок — состояние в `FaviconCache`, сетевые запросы спавнит `OrbitApp::poll_favicons`);
- надписи интерфейса — на английском (решение фазы 6);
- `src/app.rs` — единственная точка побочных эффектов: `apply_actions`/`apply_action`
  выполняют `Play`, `ToggleFavorite`, `SelectCountry`, `SwitchMode`, `SelectGenre`,
  `RunSearch`, `Roulette`, `ClearHistory`, `SetTheme`, `SetAccent`, `ToggleSpectrum`,
  `SetVizBars`, `SetMotion`, `SetMotionStyle`, `SetAllowFlips`, `SetReduceMotion`, `SetVolume`, `TogglePlayback`, `Stop`,
  `PlayRelative`, `Hide`, `Quit`,
  `TogglePanel`, `ToggleHelp`, `SetWindowMode`, превращая каждое в вызов метода
  `Session`, и после каждого действия просят repaint.

`UiAction` — внутренний тип, не публичный контракт. `render` по-прежнему не импортирует
доменные типы: `globe_view` передаёт в `paint_globe` камеру и готовые маркеры
(`MarkerVisual`), а `GlobeMarker` приходят из `crate::session::markers`.

### 3.2. Раскладка «Orbital Desk»

Глобус занимает всё окно (`CentralPanel` без отступов, full-bleed со звёздным
небом), поверх него в том же кадре рисуются плавающие зоны — `ui.scope_builder` с
`max_rect` из `DeskLayout::compute(viewport, panel_open, help_open, mode)`; порядок
отрисовки равен
порядку слоёв. Геометрия (`src/ui/layout.rs`): отступ 14 px, зазор 12 px,
HUD — 58 px сверху, панель — 312 px справа, transport — 84 px снизу (контролы + громкость +
40 px визуализатора, ТЗ §3.3), help-карточка 216×108 слева сверху (только когда открыта),
легенда 190×58 слева снизу. `panel` и `help` — `Option<egui::Rect>`: закрытая зона не
рисуется и не перекрывает глобус. `DeskLayout::blocks_point` отсекает клики и колесо
глобуса внутри плавающих зон.

`WindowMode` (ADR-0021) задаёт `LayoutDensity`: `Full` — раскладка Focus выше,
`Compact` — HUD-строка, мини-глобус и капсула избранного (до 8 строк `station_row`),
`Minimal` — одна строка now-playing (`src/ui/capsule.rs`). В капсулах звёздное небо и
country-режим выключены, `pulse` считается в `draw_layers` один раз за кадр и
передаётся только Listening; глобус от эфира не пульсирует. Focus-геометрия не
меняется — это закреплено тестами `DeskLayout`.

| Зона | Содержимое |
|------|------------|
| HUD | логотип, `Country/Genre`, поиск (у страны — popup подсказок), now-playing с favicon и LIVE, луна, шестерёнка, кнопка списка, `?`, `×` |
| Панель | `Results / Favorites / History` с иконками, локальные фильтры (поле, чипы, `Sort by`, `Compact`), строки станций |
| Transport | favicon 44 px, имя/город/`Now Playing: теги`, `⏮ play ⏭`, stop, heart, `Radio Roulette`, `CODEC kbps`, мини-спектр, чип `Visualizer` |
| Help-карточка | мыши-пиктограммы: Right-drag — orbit, Left click — country/station, Scroll — zoom |
| Легенда | `Station density` градиент Low→High |

Иконки рисуются через `Painter` в `ui/icons.rs` и кастомных виджетах `ui/widgets.rs`
(треугольники, дуги из `Shape::line`, mesh) и не зависят от набора глифов шрифта.
Фон стеклянных зон — `widgets::glass_panel` (восемь полос градиента, верхний блик,
рамка) — рисуется до содержимого, потому что `egui::Frame` умеет только сплошную заливку.

`Toast` показывает одно сообщение 3 секунды внизу по центру над transport.
Help-карточка видима по требованию: она открывается сама только при первом запуске
(`Settings::help_seen == false`), далее — кнопкой `?` в HUD, закрывается `Esc`,
повторной `?` и гаснет после первого выбора страны или запуска станции
(`Session::mark_help_seen`).

## 4. Порядок инициализации

```
IpcServer::try_bind → AudioEngine::new → GeoService::from_embedded
→ MediaControls::new(wake) → RadioBrowserClient::new → StationLibrary::load_default
→ SettingsStore::load_default → фоновый refresh_servers → задача audio events
→ wake → IpcServer::run → AppServices → run_session → Session::new
→ (restore_size → begin_window → run_window → OrbitApp::new → … → end_window → park)
```

`try_bind` выполняется до создания аудио-ядра, чтобы второй экземпляр завершился, не
занимая аудиоустройство. Ошибки `AudioEngine::new` и загрузки гео-датасета фатальны;
ошибки MPRIS, библиотеки, настроек и прогрева каталога — только предупреждения. При
ошибке загрузки библиотеки создаётся `StationLibrary::ephemeral()`: изменения работают
до завершения процесса, повреждённый файл не перезаписывается. Аналогично ошибка
загрузки настроек даёт `SettingsStore::ephemeral()` с темой System и акцентом Blue.

`OrbitApp::new` вызывается на каждый прогон окна: применяет настройки оформления к
`egui::Context` (`palette::configure_style`), ставит системный цветной emoji-шрифт
(ADR-0017), создаёт `GlobeRenderer` из `cc.wgpu_render_state` и вставляет его в
`render_state.renderer.write().callback_resources`, строит `VisualizerModel` по
`Settings::viz_bars` и устанавливает `WakeHandle`:
`session.wake.install(move || ctx.request_repaint())`. При отсутствии wgpu-состояния
приложение работает без глобуса (`globe_ready = false`). Снятие слота (`wake.clear()`)
делает `Session::end_window`, а не UI-код.

## 5. Главный цикл

При открытом окне `logic()` (перед отрисовкой):

1. `Session::tick()` — внутри него: разбор накопившихся `AudioEvent`, очереди
   `IpcRequest` с ответами через `IpcRequest::try_respond`, `MediaCommand` от MPRIS,
   `poll_search`, `poll_roulette`, `sync_media` и синхронизация `spectrum_enabled`
   с движком.
2. `drain_notices` — `Session::take_notices()` → `Toast::info`/`Toast::error`.
3. `poll_favicons` — `FaviconCache::begin_frame`/`poll`, `take_fetches` → `reqwest`-задачи
   в `session.runtime` с проверками схемы, MIME и размера (ADR-0025), декод в
   `catch_unwind`, repaint через 120 мс, пока кэш занят.
4. Размер окна: `inner_rect` из ввода → `Session::set_window_size([w, h])`.
5. `Session::take_intent()` → `ViewportCommand::Focus` (`UiIntent::Show`, окно уже
   открыто — остаётся вернуть фокус) или `ViewportCommand::Close` (`UiIntent::Close`).
6. `Escape`, когда карточка открыта, → `session.show_help = false` + `mark_help_seen`.
7. Repaint-политика: `SPECTRUM_REPAINT` (≈33 мс) при включённом визуализаторе и
   `Playing`/`Buffering`. `draw_layers` добавляет кадры движения: 33 мс при
   `motion.intensity > 0.01`, иначе `MOTION_IDLE_REPAINT` (≈66 мс) — только если
   эффективная реактивность включена; при `Off`/Reduce motion лишних кадров нет.
   Импульсы камеры и маркеров запрашивает сам `globe_view`.

`ui()` вызывает `draw()`: `Frame::capture` → `egui::CentralPanel` с прозрачной заливкой →
`draw_layers` (глобус, HUD, панель, transport, help, легенда, toast) → `apply_actions`.
`clear_color()` возвращает `[0.0, 0.0, 0.0, 0.0]` — окно прозрачное, стекло зон рисует
само.

При закрытом окне UI-цикла нет вовсе: ни `logic()`, ни `ui()` не вызываются, кадры не
аллоцируются. События в этом состоянии читает `Session::park` — `tokio::select!` по
приёмникам IPC, MPRIS и аудио-событий; команды исполняются без UI, а `UiIntent::Show`
заканчивает парковку и `run_session` перезапускает прогон.

## 6. Глобус, маркеры и выбор страны

Зону контента делит `center`-область: `globe_view::show` получает прямоугольник
глобуса, камеру, маркеры, состояние спектра и `&mut CalloutState`, а возвращает
`GlobeResponse { actions, clicked_point }`.

1. ПКМ + drag → `GlobeInput::drag_delta`; колесо при наведении → `scroll_delta`;
   ЛКМ → `primary_clicked`. Ввод под плавающими зонами отсекается
   `DeskLayout::blocks_point`.
2. `GlobeCamera::advance(stable_dt, elapsed, &input, &motion, allow_flips)`
   обновляет ориентацию, инерцию, зум, музыкальный sway и перевороты.
   `allow_flips` вычисляется здесь: настройка `allow_flips` ∧ `Full` ∧
   эффективная реактивность включена; `OrbitRing::paint` рисует кольцо до
   `paint_globe`, поэтому сфера перекрывает его внутреннюю часть (ADR-0029).
   `GlobeState.motion` пришёл из `MotionMixer`; `globe_view` один раз за кадр
   получает `GlobeMotion` с ослабленной при взаимодействии геометрией и
   передаёт `deform` шейдеру, странам, маркерам, callout и hit-test.
3. Hover страны (только в режиме «Страна»):
   `screen_to_sphere_deformed(hover_pos, viewport, &deform)` →
   `country_at_sphere_point` (при ПКМ-drag hover отключается). `globe_view` рисует
   контур наведённой страны; выбранная страна получает акцентный контур 2 px и
   заливку-heatmap с альфой 26..96 от `log10` количества загруженных станций
   (`GlobeMarker.count` по ISO-коду). Курсор `PointingHand` — над страной или
   маркером, `Grabbing` — при вращении.
4. При клике сначала проверяется ближайший маркер в радиусе 10 px: он приоритетнее
   выбора страны и возвращает `UiAction::Play`.
5. Если маркера нет и режим — «Страна», `screen_to_sphere_deformed` даёт точку
   единичной сферы; `OrbitApp::draw_layers` передаёт её в
   `GeoService::country_at_sphere_point`
   (`session.geo`) и при попадании создаёт `UiAction::SelectCountry`. Тот же путь
   используется для выбора из HUD. В режиме «Жанр» проверка страны отключена
   (`country_enabled = false`), поэтому клик по суше не меняет контекст.
6. `apply_action` передаёт решение ядру: `Session::focus_country` (Core) —
   `country_shape(...).focus_point()` запускает `GlobeCamera::fly_to` на 650 мс (центр
   крупнейшего кольца, корректно на антимеридиане); затем `Session::start_country_search`
   активирует страну в `search.context`, сбрасывает локальные фильтры и запрашивает
   `countrycode=<ISO alpha-2>`, `limit = 100`, `hide_broken = true`.
7. Маркеры проецируются через `GlobeDeform::to_world` и `project_on_sphere`;
   наведение показывает двухстрочную
   подпись по ТЗ §2.4 — имя (для группы `· +N`) и `место · kbps · кодек` — и
   курсор-указатель, станция в эфире получает `colors.active`, кольцо и пульс.
   `paint_globe` получает `GlobeMotion`, затем
   `paint_country_highlights` и `paint_markers` рисуют egui-оверлей с той же
   деформацией; repaint
   запрашивается, пока камера `is_animating` или на сфере виден играющий маркер.
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
`globe_ready = false` вместо глобуса рисуется заглушка `3D globe unavailable`, ввод
игнорируется.

## 7. Аудиовизуализатор и кольцо Listening

- В правой части transport — cava-столбцы `viz_bars` (огибающая из шести агрегатов,
  не бинный FFT — ADR-0010, ADR-0022) и чип `Visualizer`. Спектр-фильтр включён
  при старте, поэтому столбцы оживают сразу с эфиром.
- Чип вызывает `UiAction::ToggleSpectrum` → `Session::set_visualizer_enabled`, ядро делает
  `runtime.block_on(AudioEngine::set_spectrum_enabled)` только если меняется
  потребность в анализе; ошибка показывается тостом и не
  прерывает воспроизведение. Состояние чипа не сохраняется в настройках.
- `Session::tick()` каждый кадр синхронизирует `spectrum_enabled` с движком, а
  `Frame::capture` преобразует `AudioEngine::spectrum()` в `SpectrumTarget` (включая
  `bands: Option<BandTargets>`): он питает `SpectrumOverlay` (кольцо
  Listening), `VisualizerModel::advance` и `MotionMixer` (полосы и fallback
  RMS/flux — ADR-0028). Если визуализатор выключен, но движение включено,
  `analysis_needed` удерживает аудио-анализ; при `Off`/Reduce motion он
  отключается.
  Модель (`visualizer`) живёт в `OrbitApp` как ресурс прогона, отрисовку делает
  `ui::widgets::viz_bars`: поле 156×46 px, зазор 2 px, скругление 1 px, градиент по
  абсолютной высоте, шапки пиков 2 px.
- `VisualizerModel` (ADR-0022): атака 28 мс, линейная гравитация спада 1,8 ед/с,
  пики падают на 0,42 ед/с; мелкая фактура столбцов зависит от `flux` и
  детерминированной фазы. `advance` считается при
  `Playing`/`Buffering`/`Reconnecting` и включённом фильтре; иначе цели заменяются
  idle-«дыханием». Число столбцов берётся из `Settings::viz_bars` (16/24/32, по
  умолчанию 24) и меняется действием `UiAction::SetVizBars` — `Session::set_viz_bars`
  сохраняет настройку, а UI пересоздаёт модель.
- `SpectrumOverlay::pulse()` (`0.75·level + 0.25·flux`, 0 при выключенном
  фильтре) передаётся только в кольцо Listening. Fresnel-обод глобуса
  статичен; геометрию планеты двигает отдельный `MotionMixer` (ADR-0028):
  squash, рябь и idle-дыхание уходят в `GlobeMotion`, а не в `pulse()`.
- `MotionMixer::update` вызывается в `draw_layers` один раз за кадр с
  `stable_dt`; цель — `SpectrumTarget` только в
  `Playing`/`Buffering`/`Reconnecting`, иначе `None` (плавный возврат в idle).
  Эффективная реактивность приходит из `Session::motion_reactivity()`
  (Reduce motion → `Off`, Overlay ≤ `Low`, Listening — `Off`).
- `OrbitRing` (ADR-0029) обновляется в `draw_layers` из `SpectrumTarget`, пока
  открыт Focus, включён визуализатор и эфир жив
  (`Playing`/`Buffering`/`Reconnecting`); иначе цели нет и кольцо плавно гаснет.
  Модуль не знает про аудио: корона, аврора, комета и искры — функции шести
  агрегатов, `dt` и времени. В Overlay/Listening кольцо не рисуется.
- Пока визуализатор активен, `logic()` просит кадр каждые ~33 мс. `draw_layers`
  добавляет кадры движения: ~33 мс при `motion.intensity > 0.01`, ~66 мс для
  idle-дыхания, кадры на время переворота (`globe_camera.is_flipping()`) и ни
  одного лишнего кадра при `Off`/Reduce motion. В `globe_view`
  repaint запрашивается на ~33 мс при играющем маркере, анимирующемся кольце или
  авто-вращении; при ручном вращении и довороте — без задержки; до начала
  авто-вращения окно будится к концу 4-секундной паузы.
- Камера получает `stable_dt` для физики и `unstable_dt` для отсчёта простоя:
  после сна окна `stable_dt` остаётся малым. Через 4 с без взаимодействия глобус
  начинает медленно вращаться (0,006 рад/с) — подробности в ADR-0023.
- IPC-команды для визуализатора нет: управление доступно только из GUI. Полосы
  спектра по бинам по-прежнему не поддерживаются (ADR-0010, ADR-0022).
- При закрытом окне визуализатор не обновляется: он живёт кадрами прогона, а при
  показе строится заново из `Settings::viz_bars`.

## 8. Поиск и запуск станций

- «Найти»/Enter в свободном контексте (`UiAction::RunSearch` → `Session::start_search`)
  запускают `RadioBrowserClient::search` с `name`, `limit = 100`, `hide_broken = true`;
  выбранная страна/жанр при этом сбрасываются. Если контекст активен, поле поиска
  применяется локально к уже загруженному списку (`country_filter.text`), без сетевого
  запроса. Запрос выполняется в tokio-задаче (`Session::begin_search`), результат приходит
  через `mpsc` и будит GUI через `WakeHandle`.
- Параллельно с основным запросом (`tokio::join!`) выполняется гео-запрос с теми же
  фильтрами, `has_geo_info = true`; лимиты — 100 для названия/страны и 200 для жанра как для
  списка, так и для маркеров. Ошибка гео-запроса логируется и не влияет на список;
  результат используется только для маркеров.
- Маркеры строятся в ядре (`session::markers::build_markers`) из гео-станций: координаты
  должны быть конечными, лежать в `[-90, 90]`/`[-180, 180]` и не быть `(0, 0)`; станции
  с координатами, округлёнными до 2 знаков, объединяются, дубликаты по UUID отбрасываются.
- Маркеры рисуются одинаковыми (радиус 3 px, единый `palette.marker`); качество и рейтинг
  не кодируются цветом или размером, а показываются в подписи при наведении:
  место · kbps · CODEC · голоса. Играющая станция получает кольцо-пульс; наведение —
  светлое кольцо и подпись (`MarkerLabel`, ADR-0012).
- Поиск по стране использует тот же канал и отличается только `SearchParams`
  (`country_code`, без `name`, `limit = 100`); гео-запрос для маркеров тоже
  выполняется с `limit = 100`. Заголовок панели `Country: …` остаётся до следующего
  поиска или смены режима.
- Поиск по жанру: `tag = <тег>`, `limit = 200`, гео-лимит 200. Чипы пресетов и
  поле своего тега находятся в popup `ui/mode_switch.rs`; выбор запускает
  `Session::start_genre_search`, который очищает `genre_query` и сбрасывает локальные
  фильтры. Заголовок панели — `Genre: …`; станции без координат остаются в списке.
- `SearchMode` переключается только кнопками HUD (`UiAction::SwitchMode` →
  `Session::switch_mode`); смена режима очищает контекст, результаты, маркеры, фильтры и
  вкладку `Results`.
- Каждый запуск поиска увеличивает `search.generation`; `SearchResponse` несёт
  номер поколения, `Session::poll_search` отбрасывает ответы прошлого режима/жанра и не
  возвращает устаревшие маркеры.
- HUD «Страна» вызывает `GeoService::search_countries` (до 6 подсказок) при каждом
  изменении запроса; Enter выбирает первую, клик — выбранную. Оба пути создают
  `UiAction::SelectCountry`, `Session::start_country_search` очищает `country_query` и
  сбрасывает `country_filter`.
- Панель страны применяет `station_filter::filtered_results` к уже загруженным
  результатам: текст ищется в `name/tags/country/state`, жанровые чипы проверяют
  теги без учёта регистра, сортировка — по `votes`, `bitrate` или имени. Это
  локальные операции без сети; карточки показывают state/country, kbps, codec,
  теги и голоса. При пустом результате фильтров — «Quiet in this sector» /
  «No station matches the filters.» и кнопка `Reset filters`, сохраняющая выбранную
  сортировку.
- Пустой успешный ответ выставляет `search.empty`; UI показывает `No stations found`.
- Новый поиск очищает маркеры; ошибка основного запроса также очищает их.
- Выбор результата или IPC `play <uuid>` вызывает `Session::start_station`:
  проверяется непустой `url_resolved`, выполняется `audio.play`, сохраняются
  `current_url`/`current_station`, асинхронно отправляется click (`Session::spawn_click`).
- `play <uuid>` сначала ищет станцию в последних результатах поиска
  (`Session::resolve_station`), затем вызывает `get_station` с таймаутом 8 с
  (`STATION_LOOKUP_TIMEOUT`); таймаут маппится в `RadioError::Network`.

### 8.1. Roulette

- Кнопка `Radio Roulette` в transport вызывает `Session::start_roulette`; пока
  `search.roulette_pending`, повторные нажатия игнорируются, кнопка заблокирована.
- `RadioBrowserClient::random_station` выполняется в tokio-задаче (до трёх попыток,
  станция обязана иметь поток и координаты); результат приходит через `mpsc` и будит GUI.
- `Session::poll_roulette` запускает станцию, при успехе перелетает камерой за 900 мс
  (`ROULETTE_FLY_MS`) к её координатам и отдаёт `Notice::Info` — UI показывает тост
  `Roulette: имя — страна`. Ошибка каталога или запуска показывается тостом и не меняет
  текущий эфир.
- Запуск идёт через `start_station`: станция попадает в историю, отправляется click,
  обновляет MPRIS; перетаскивание отменяет перелёт, как и у выбора страны.
- IPC-команды Roulette нет: действие доступно только из GUI (ADR-0015).

### 8.2. Prev/Next по видимому списку

- Очередь навигации не хранится постоянно: `Session::visible_queue()` каждый раз снимает
  снапшот текущего видимого списка — вкладка `Results` (с учётом локальных
  фильтров и сортировки в контекстном режиме), иначе `Favorites` или `History`.
- `Session::play_relative(step)` находит позицию текущей станции в снапшоте и делает шаг
  со `rem_euclid` (циклом); если текущей станции в списке нет — `+1` даёт первую,
  `-1` последнюю; пустой список — no-op.
- Источники действия: кнопки `⏮`/`⏭` в transport (`UiAction::PlayRelative`) и
  MPRIS `Next`/`Previous`; обе маршрутизации ведут в один метод ядра.

## 9. Избранное и история

- `StationLibrary` загружается в `main` до запуска GUI и принадлежит `Session`; ошибка
  загрузки даёт эфемерную библиотеку без файла (ADR-0013).
- Успешный `Session::start_station` (строка результатов, избранного или истории, маркер,
  Roulette, IPC `play`) вызывает `record_played`: станция перемещается в начало истории,
  история ограничена 20 записями. Возобновление из MPRIS (`Play`/`PlayPause`)
  историю не меняет.
- Переключение звезды (`UiAction::ToggleFavorite` → `Session::toggle_favorite`) запрашивает
  repaint; звезда есть у текущей станции (heart в transport) и у каждой строки списка
  (звезда в панели). Ошибка сохранения доходит как `Notice::Error` и показывается тостом.
- Под фильтрами показываются вкладки `Results`/`Favorites (N)`/`History (N)`, если
  есть результаты или сохранённые станции. Список прокручивается на всю доступную
  высоту; клик по строке запускает станцию, клик по звезде — меняет избранное.
- Новый поиск сбрасывает вкладку на `Results`; на вкладке `History` есть кнопка
  `Clear history` (`UiAction::ClearHistory` → `Session::clear_history`) с подтверждением
  тостом.
- Ошибки библиотеки логируются и показываются тостом; они не прерывают
  воспроизведение. Пути и формат файла описаны в `docs/modules/09-library.md`.

## 10. Оформление

- `SettingsStore` загружается в `main` до запуска GUI, хранится в `Session`; ошибка чтения
  даёт эфемерное хранилище с настройками по умолчанию, повреждённый файл не
  перезаписывается (ADR-0014).
- При старте прогона и после каждого изменения `palette::configure_style` применяет к
  `egui::Context`: `ThemePreference` (System/Dark/Light), прозрачные заливки панелей
  и окна (нужно для overlay), цвет выделения `selection.bg_fill` из акцента,
  контрастную обводку выделения и цвет гиперссылок. Значение сохраняется ядром
  (`Session::set_theme`, `Session::set_accent`), перекраску делает UI.
- Блок `Settings` в HUD (попап шестерёнки): пункты темы `System`/`Dark`/`Light`, шесть
  пресетов акцента (`Orbit`, `Emerald`, `Amber`, `Violet`, `Rose`, `Ocean`), выбор числа
  столбцов `Visualizer bars`, секция `Motion` (`Off`/`Low`/`Normal`/`High` и чекбокс
  `Reduce motion`), `Hide window` и `Quit`. Изменение применяется сразу и
  сохраняется через `SettingsStore`; ошибка записи показывается тостом и не прерывает
  воспроизведение. `Reduce motion` не перезаписывает `motion`: эффективная
  реактивность просто становится `Off`.
- `Palette` и `configure_style` живут в `ui/palette.rs`. `Palette::for_ui(ctx.theme(), accent)`
  вызывается в `Frame::capture` и задаёт цвета панелей, обводки, приглушённого и
  ошибочного текста, подложек и глобуса. В тёмной теме акцент совпадает с цветом маркеров;
  в светлой — затемняется для контраста с фоном, а маркеры остаются яркими.
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

`sync_media()` — метод `Session` (`src/session/playback.rs`). Он вызывается в конце
`tick()` (то есть каждый кадр при открытом окне), после `start_station`, в
`end_window()` и после каждой исполненной команды в `park()`, чтобы витрина не отставала
без UI. `MediaControls::set_playback` дедуплицирует неизменившееся состояние. Соответствие:

- `Stopped`/`Error` → трек отсутствует, `Stopped`;
- `Paused` → трек есть, `Paused`;
- `Playing`/`Buffering`/`Reconnecting` → трек есть, `Playing`.

Метаданные берутся из `current_station`: `name` → title, `favicon` → artUrl,
`url_resolved` → `xesam:url`. Artist — «штат, страна» (или одно из полей), при
отсутствии локации — теги станции; пустые значения отбрасываются. Без станции
используется URL потока, при пустом имени трека — `WINDOW_TITLE`.

## 12. Жизненный цикл окна

Механизм определён ADR-0018 (редакция 2): скрытие — это реальное закрытие
root-вьюпорта, а не невидимое окно. `ViewportCommand::CancelClose`, `ViewportCommand::Visible`
и сжатие поверхности до 1×1 не используются: на Wayland `set_visible` в winit реализован
пустой функцией, а минимальный размер окна клампится константой `2×1`; редакция 1 ADR
отклонена live-проверкой на niri (окно оставалось в раскладке и перехватывало ввод).

- `×` в HUD (tooltip «Hide window (audio keeps playing)») и `Hide window` в попапе
  шестерёнки дают `UiAction::Hide` → `Session::hide()` → `UiIntent::Close`; `logic()`
  шлёт `ViewportCommand::Close`, `run_native` возвращается (`run_and_return: true`),
  `Session::end_window` помечает окно закрытым, снимает `WakeHandle` и обновляет MPRIS.
  Запрос композитора закрыть окно идёт тем же путём — никакого перехвата.
- В закрытом состоянии нет ни поверхности, ни кадров: `run_session` вызывает
  `Session::park()`, который читает IPC, MPRIS и аудио-события. `show`/`toggle` в этом
  состоянии дают `UiIntent::Show`, `park` выходит с `ParkOutcome::Show`, и цикл
  повторяется: `restore_size()` → `begin_window(size)` → `run_window` (второй и каждый
  следующий `eframe::run_native` из того же главного потока).
- Размеры применяются до создания окна: `NativeOptions.viewport` собирается из
  `Session::restore_size()` (`with_inner_size` + `with_min_inner_size`,
  `WINDOW_SIZE` 920×660 по умолчанию, `WINDOW_MIN_SIZE` 720×480), а фактический размер
  текущего окна возвращается в ядро из `inner_rect` в `logic()`. Спросить размер уже
  созданного окна на Wayland нельзя — композитор вправе отбросить просьбу.
- Позиция окна не сохраняется: размещение — прерогатива композитора (правило
  `open-floating` из `wm-configs/orbitfm-niri.kdl`). `ViewportCommand::Focus` при
  показе открытого окна на Wayland документированно не имеет эффекта, поэтому фокус
  тоже на усмотрение композитора.
- `quit` = выход: `Session::quit()` ставит флаг `quitting` **до** отправки
  `UiIntent::Close`; `run_session` проверяет `is_quitting()` сразу по возврату из
  `run_native` и завершает процесс (drop `Session` → `AudioEngine::drop` останавливает
  mpv, затем `shutdown_tx` + drain IPC-сервера с таймаутом 2 с, сокет удаляется).
- `toggle` опирается на признак `Session` (`WindowState::visible`), а не на состояние WM:
  окно, свёрнутое композитором, считается открытым, и `toggle` его закрывает.
- `?` открывает/закрывает карточку управления (`UiAction::ToggleHelp`), это не окно.

## 13. Текущие ограничения

- Heatmap считает только загруженные станции выбранной/наведённой страны; глобального
  aggregate по всем странам нет (отдельное решение, ADR-0015)
- Roulette доступна только из GUI и всегда запускает воспроизведение; «только
  показать станцию без эфира» не поддерживается
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
- Показ окна стоит пересоздания UI-прогона (wgpu-адаптер, `GlobeRenderer`, шрифты,
  `egui::Context`): ≈1.02 с на niri 26.04 от приёма IPC-`show` до отрисованного деска,
  из них ≈73 мс — сам IPC-запрос (ADR-0018)
- `FaviconCache` после повторного прогона пуст: иконки загружаются заново, текстуры
  были привязаны к умершему `egui::Context`
- Позиция окна и фокус при показе — на усмотрение композитора; пользователь может
  увидеть окно без ввода-фокуса (ограничение Wayland, ADR-0018 п. 9)
- `orbitfm status` в припаркованном состоянии всегда отвечает `window hidden`, а
  `window visible` — пока прогон окна жив: свёрнутое композитором окно при этом считается
  открытым (ADR-0018 п. 9)

## 14. Связанные документы

- ADR-0002, ADR-0005, ADR-0006, ADR-0010, ADR-0012, ADR-0013, ADR-0014, ADR-0015,
  ADR-0016, ADR-0017, ADR-0018, ADR-0019, ADR-0028, ADR-0029
- `docs/modules/11-session.md`, `docs/interfaces/10-session.md` — контракт Application Core
- `docs/interfaces/07-app.md` — контракт presentation-слоя
- `docs/architecture/00-overview.md`
- `docs/ui/design-vision.md`, `docs/ui/implementation-plan.md`
- `docs/modules/09-library.md`
- `docs/modules/10-settings.md`
- `wm-configs/orbitfm-niri.kdl`, `wm-configs/orbitfm-hyprland.conf` — правила окон
