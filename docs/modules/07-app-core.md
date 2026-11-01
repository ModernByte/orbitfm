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
  `settle_analysis` → `run_window` → `end_window` → `is_quitting` → `take_reopen` →
  `close_settings` → `settle_analysis` → `park`. Анализ эфира живёт только при открытом
  окне: `settle_analysis` включает его на показ и снимает перед парковкой (ADR-0027).
  Окно настроек закрывается при любом hide/WM-close/парковке и переживает только
  пересоздание вьюпорта из-за смены режима (ADR-0031).
- На каждый прогон собрать per-run ресурсы в `OrbitApp::new`: тему и акцент
  (`palette::configure_style`), цветной emoji-шрифт, `GlobeRenderer` в `callback_resources`,
  `FaviconCache`, `ArtworkFetcher` (`services::artwork`), `VisualizerModel`, `Toast`, `Transitions`
  (переходы зон, ADR-0032) и установить `WakeHandle`
  (`wake.install` замыкания, зовущего `Context::request_repaint`).
- Реализовать `eframe::App` (`logic` + `ui` + `clear_color` в eframe 0.37): фоновое
  обслуживание через `Session::tick`, сборка кадра, применение `UiAction`.
- Транслировать `UiIntent` ядра в `ViewportCommand` (`Focus`/`Close`) и сообщать ядру
  размер окна из `inner_rect`.
- Показывать `Notice` ядра тостом; ошибки доменных операций не хранятся в UI.
- Маршрутизацией `IpcCommand` и `MediaCommand` занимается `Session` — здесь только
  очереди, которые ядро само читает (из `tick()` или из `park()`).

## 3. Состояние прогона (per-run)

Текущее состояние (Этап 8, фазы 1–6 + ADR-0018, ADR-0031, ADR-0032):

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
    artwork: ArtworkFetcher,
    toast: Toast,
    /// Переходы UI этого прогона окна (ADR-0032).
    transitions: Transitions,
    /// Прямоугольник окна настроек с последнего открытого кадра: нужен, чтобы
    /// окно гасло по месту, а не исчезало скачком (ADR-0032).
    settings_rect: Option<egui::Rect>,
    /// Открытая категория окна настроек: состояние представления.
    settings_category: SettingsCategory,
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
  привязанные к `egui::Context` этого прогона (ADR-0016, ред. 2). После закрытия окна пуст —
  при следующем показе иконки грузятся заново. Декодируются только PNG/JPEG/WebP
  до 512 px по стороне и 512 КиБ; SVG/GIF/ICO отклоняются (ADR-0025). Границы
  `entries ≤ 256` и `in_flight ≤ 8` — инварианты: вытеснение ступенчатое, результат
  сверяется с поколением записи, слот освобождает RAII-страховка (ADR-0016 ред. 2).
  `retry_failed` возвращает в очередь `Failed`-записи после включения
  HTTP-обложек — политика фетчера переключается на лету (ADR-0025 ред. 4).
- сетевых полей у `OrbitApp` больше нет: исходящий fetch артворка живёт в
  `src/services/artwork.rs` вместе с клиентом, policy-проверками и бюджетами
  (ADR-0016 ред. 2, ADR-0025 ред. 4); в `app.rs` остаются только очередь задач и
  приём результатов.
- `toast` — стек до трёх сообщений по 3 секунды; переходы слоёв — ADR-0032.
- `transitions` — `Transitions` (ADR-0032): один вызов `frame` на кадр считает
  прогрессы присутствия зон (`ZoneFlags` → `FrameMotion`); значения читают зоны и
  каркасы стеклянных панелей. Не переживает скрытие окна.
- `settings_rect`, `settings_category` — последний прямоугольник окна настроек и
  открытая категория: пока окно гаснет, `layout.settings` уже `None`, и геометрию
  берут отсюда, чтобы стол не дёргался.

Всё остальное живёт в `Session`: подсистемы (`audio`, `geo`, `radio`, `media`, `library`,
`settings`, `runtime`), `globe_camera`, `visualizer_enabled`, `spectrum_enabled`,
`language`, `current_url`/`current_station`,
`search: SearchState` (строки запросов, `context`, `results`, `generation`, `pending`,
`empty`, `error`, каналы поиска и рулетки), `markers`, `library_tab`, `country_filter`,
`panel_open`, `settings_open`, `show_help`, `pending_volume`, `window: WindowState`
(видимость, размер, `intent`, `quitting`), `publish_gate: PublishGate` (вердикт
яруса 2 для MPRIS/SMTC/Now Playing, ADR-0025 ред. 4),
`notices`, `wake`, `commands`, `audio_events`. `visualizer_enabled` — пользовательский
переключатель визуализатора и кольца; это сохранённая настройка
(`Settings::visualizer_enabled`, по умолчанию `true`), из которой `Session::new` его и
инициализирует (ADR-0031); `spectrum_enabled` — фактическое
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
    /// Разрешённый язык: из него зоны берут строки каталога (ADR-0030).
    language: Language,
    window_mode: WindowMode,
    show_help: bool,
    /// Всё окно: по нему рисуется затемнение под окном настроек.
    viewport: egui::Rect,
}
```

`Frame::capture(&ctx, session, &mut visualizer, viewport)` вызывается до отрисовки зон:
`Palette::for_ui(ctx.theme(), accent)`, `session.settings.settings()`,
`session.audio.state()`, `session.audio.volume()`, `session.audio.spectrum()` →
`SpectrumTarget` (шесть агрегатов и `bands` через `BandTargets::from_db`),
`session.motion_reactivity()` и `settings.motion_style` для движения, сумма
`marker.count` в `markers_total`, `library.is_favorite` для
текущей станции, `current_url.is_some()` в `can_resume`,
`search.context.mode == SearchMode::Country` в `country_mode`,
`session.effective_language()` в `language` и
`DeskLayout::compute(viewport, panel_open, show_help, settings_open, session.window_mode())`
в `layout`. Там же считается шаг
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
│                     # rim_light, live, chip_fill/chip_stroke, text), configure_style (тема,
│                     # акцент и `Style::animation_time` под reduce motion), theme_preference,
│                     # accent_color, mix_colors, heat_color — рампа §3.2
├── animation.rs      # длительности и кривые переходов, `Presence`, `Transitions`/`FrameMotion`,
│                     # `alpha`/`slide_x`/`rise_y`/`stagger` (ADR-0032)
├── layout.rs         # DeskLayout: геометрия плавающих зон и окна настроек, LayoutDensity из
│                     # WindowMode, blocks_point для глобуса
├── capsule.rs        # капсулы Overlay (top с 16-полосной лентой, мини-глобус, избранное)
│                     # и Listening
├── widgets.rs        # кастомные контролы: pill, chip, search_field, tab_bar, live_badge,
│                     # eq_bars_in, viz_bars с общими `NARROW_VIZ_SIZE` 62×16 и
│                     # `NARROW_VIZ_BARS` 16 плюс прореживание `bucket_max` (капсула
│                     # Overlay и шапка панели), volume_slider_width (клавиатурный
│                     # шаг, колесо ±5 % и `SliderResponse { changed, committed }`), circle_button, avatar,
│                     # logo_mark, flag_emoji, glass_panel (стеклянный градиент),
│                     # focus_ring/circle_focus_ring, capsule_button, faded/faded_group/faded_palette
├── images.rs         # FaviconCache: машина состояний Queued/Loading/Ready/Failed (ADR-0016)
├── hud.rs            # верхний HUD: логотип, Country/Genre, поиск станции/страны,
│                     # индикатор эфира (live-бейдж), тема-луна, шестерёнка (открывает
│                     # настройки), кнопки списка/?/скрытия; шапка выпадающей панели
├── mode_switch.rs    # pill-переключатель «Country | Genre»; клик по Genre сразу
│                     # открывает popup выбора жанра (без отдельной кнопки)
├── station_filter.rs # локальные фильтры: текст, мультивыбор жанров (OR),
│                     # сортировка, плотность compact_rows, видимость строки
├── station_panel.rs  # правая плавающая панель: вкладки (LibraryTab из crate::session),
│                     # фильтры, список станций
├── station_row.rs    # единая строка списка: favicon-аватар, имя, город, теги, битрейт,
│                     # звезда; фокус, ↑/↓ по видимому списку, Space/Enter
├── transport.rs      # нижний transport: now-playing, prev/next, play/pause, stop, heart,
│                     # Radio Roulette, codec/kbps, громкость и мини-спектр
├── controls.rs       # контролы строк окна настроек: `dropdown` (ComboBox в стиле чипа со
│                     # своим шевроном), `switch_row` (тумблер 40×22, кликабельна вся
│                     # строка), `accent_swatch` (круглый свотч с кольцом выбора) и
│                     # `row` (подпись слева, контрол в фиксированном слоте справа,
│                     # подсказка под подписью)
├── settings_panel.rs # модальное окно настроек: категории слева, карточки справа (ADR-0031)
├── overlays.rs       # help-карточка управления и легенда карты стран с выбором метрики
├── globe_view.rs     # ввод камеры, проекция и вид маркеров, тепловая карта, гашение
│                     # движения под курсором, каналы callout страны и станции, звёздное небо
├── orbit_ring.rs     # орбитальное аудиокольцо Focus: корона, аврора, комета, искры (ADR-0029)
├── toast.rs          # стек до трёх сообщений на 3 секунды (ADR-0032)
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
  фавиконок — состояние в `FaviconCache`, спавн задачи — в `OrbitApp::poll_favicons`,
  а сама сеть и политика цели — `services::ArtworkFetcher`);
- надписи интерфейса приходят из `crate::i18n::catalog(language)` (ADR-0030), язык —
  `Session::effective_language()`; не переводятся стабильные контракты и внешние данные:
  CLI-вывод, IPC-ответы и `status`, заголовки окон, app-id, названия станций, стран и
  тегов Radio Browser, MPRIS-метаданные и технические детали ошибок;
- `src/app.rs` — единственная точка побочных эффектов: `apply_actions`/`apply_action`
  выполняют `Play`, `ToggleFavorite`, `SelectCountry`, `SwitchMode`, `SelectGenre`,
  `RunSearch`, `LoadMore`, `Roulette`, `ClearHistory`, `SetTheme`, `SetAccent`, `SetLanguage`,
  `ToggleSpectrum`, `SetVizBars`, `SetMotion`, `SetMotionStyle`, `SetAllowFlips`,
  `SetReduceMotion`, `PreviewVolume`, `CommitVolume`, `TogglePlayback`, `Stop`,
  `PlayRelative`, `Hide`, `Quit`, `TogglePanel`, `ToggleHelp`, `ToggleSettings`,
  `CloseSettings`, `SetWindowMode`, превращая каждое в вызов метода
  `Session`, и после каждого действия просят repaint.

`UiAction` — внутренний тип, не публичный контракт. `render` по-прежнему не импортирует
доменные типы: `globe_view` передаёт в `paint_globe` камеру и готовые маркеры
(`MarkerVisual`), а `GlobeMarker` приходят из `crate::session::markers`.

### 3.2. Раскладка «Orbital Desk»

Глобус занимает всё окно (`CentralPanel` без отступов, full-bleed со звёздным
небом), поверх него в том же кадре рисуются плавающие зоны — `ui.scope_builder` с
`max_rect` из `DeskLayout::compute(viewport, panel_open, help_open, settings_open, mode)`; порядок
отрисовки равен
порядку слоёв. Геометрия (`src/ui/layout.rs`): отступ 14 px, зазор 12 px,
HUD — 58 px сверху, панель — 360 px справа, transport — 84 px снизу (контролы + громкость +
40 px визуализатора, ТЗ §3.3), help-карточка 216×128 слева сверху (только когда открыта,
низ карточки отделён от последней строки полем 8 px), легенда 216×100 слева снизу. `panel`, `help` и `settings` — `Option<egui::Rect>`: закрытая зона не
рисуется и не перекрывает глобус. Окно настроек — центрированное модальное окно
`SETTINGS_MODAL = 760×544` (клампится до `viewport − 2 · отступ`), слева колонка
категорий 188 px, справа карточки (ADR-0031, ред. 2026-10-28). `DeskLayout::blocks_point` отсекает клики и колесо
глобуса внутри плавающих зон, включая окно настроек.

`WindowMode` (ADR-0021) задаёт `LayoutDensity`: `Full` — раскладка Focus выше,
`Compact` — HUD-строка с 16-полосной лентой (`widgets::NARROW_VIZ_BARS = 16`, bucket-max из модели
`VisualizerModel` стола, репейн 33 мс пока играет, при выключенном визуализаторе —
ноль аллокаций), мини-глобус и капсула избранного (до 8 строк `station_row`),
`Minimal` — одна строка now-playing (`src/ui/capsule.rs`). В капсулах звёздное небо и
country-режим выключены, `pulse` считается в `draw_layers` один раз за кадр и
передаётся только Listening; глобус от эфира не пульсирует. Focus-геометрия не
меняется — это закреплено тестами `DeskLayout`.

Четвёртый режим — `Panel` (ADR-0033): выпадающая поверхность без рамок
и заголовка, плотность `Panel`, окно 400×640. Внутри — шапка с поиском и
переключателем Country/Genre, список станций и подвал транспорта; в строке
режимов шапки стоят 16-полосная лента (`NARROW_VIZ_SIZE` 62×16 — тот же размер,
что у капсулы Overlay) и громкость `PANEL_VOLUME_WIDTH = 96` px (не рисуется ниже
`PANEL_VOLUME_MIN = 56` px), в строке бренда — `×`, «Открыть Focus», шестерёнка и
кубик рулетки; оформление остаётся в окне настроек. `Panel` не входит в
`WindowMode::ALL` и открывается IPC-командой `orbitfm panel` — её же посылает клик
бар-виджета (ADR-0033, ред. 2026-10-28); ярлык приложения зовёт `orbitfm mode
focus` и всегда даёт стол Focus.

| Зона | Содержимое |
|------|------------|
| HUD | логотип, `Country/Genre`, поиск (у страны — popup подсказок), индикатор эфира «● LIVE», луна, шестерёнка, кнопка списка, `?`, `×` в правом углу строки; станция и её логотип в HUD не рисуются |
| Панель | `Results / Favorites / History` с иконками, локальные фильтры (поле, выпадающий мультивыбор жанров, бейджи выбранных с ×, кнопки «показать/скрыть» и «очистить», `Sort by`, `Compact`), строки станций, «загружено из всего» и «Загрузить ещё» |
| Transport | favicon 44 px, имя/город/`Now Playing: теги`; группа контролов (`⏮ play ⏭`, stop, heart, `Radio Roulette`) — по центру строки; справа громкость и мини-спектр, у них остаток ширины; зоны текста, контролов и правой группы не пересекаются и клиппятся |
| Help-карточка | мыши-пиктограммы: Right-drag — orbit, Left click — country/station, Scroll — zoom |
| Легенда | выбор карты стран (`ComboBox`: Off, плотность, жанры, качество, тип, рейтинг), градиент Low→High и подпись источника заливки |
| Окно настроек | центр поверх scrim: шапка (знак, заголовок, `Esc`, ×), категории Appearance / Language / Audio / Motion / Window / About, карточки строк с `dropdown`, тумблерами и свотчами акцента; внизу — «Настройки сохраняются автоматически» |

Иконки — SVG Lucide из `assets/icons/` (ADR-0034): `ui/icons.rs` перечисляет набор
через `egui::include_image!`, перекрашивает белый штрих через `Image::tint` и
рисует `Image::paint_at` в фиксированном размере; hover-подсветка анимирует только
цвет и подложку. Загрузчик SVG ставится в `OrbitApp::new` через
`egui_extras::install_image_loaders`. Иллюстрация мыши для help-карточки остаётся
ручной (`icons::draw_mouse`), потому что ей нужны позы курсора.
Фон стеклянных зон — `widgets::glass_panel` (восемь полос градиента, верхний блик,
рамка) — рисуется до содержимого, потому что `egui::Frame` умеет только сплошную заливку.

`Toast` — стек до трёх сообщений по 3 секунды (четвёртое вытесняет самое старое,
дубликат обновляет стоящую запись): якорь считает `DeskLayout::toast_offset` по
плотности окна, поэтому стек висит над transport в Focus и над избранным в
Overlay (ADR-0032).
Help-карточка включена по умолчанию: `Settings::help_visible` (default `true`)
инициализирует `Session::show_help`; `?` в HUD и тумблер в окне настроек
переключают и сохраняют выбор, `Esc` скрывает и сохраняет `false`. Первый выбор
страны или запуск станции карточку больше не гасят (ADR-0036 § 5).

## 4. Порядок инициализации

```
IpcServer::try_bind → AudioEngine::new → GeoService::from_embedded
→ MediaControls::new(wake) → RadioBrowserClient::new → StationLibrary::load_default
→ SettingsStore::load_default → фоновый refresh_servers → задача audio events
→ wake → IpcServer::run → AppServices → run_session → Session::new
→ (restore_size → begin_window → settle_analysis → run_window → OrbitApp::new
   → … → end_window → [is_quitting] → [take_reopen] → close_settings → settle_analysis → park)
```

`take_reopen` (смена режима при открытом окне) уходит на новый прогон без парковки и
без снятия анализа; `settle_analysis` перед `park` снимает `@level`/`@spectrum`, чтобы
скрытый эфир не платил за невидимую визуализацию.

`try_bind` выполняется до создания аудио-ядра, чтобы второй экземпляр завершился, не
занимая аудиоустройство. Ошибки `AudioEngine::new` и загрузки гео-датасета фатальны;
ошибки MPRIS, библиотеки, настроек и прогрева каталога — только предупреждения. При
ошибке загрузки библиотеки создаётся `StationLibrary::ephemeral()`: изменения работают
до завершения процесса, повреждённый файл не перезаписывается. Аналогично ошибка
загрузки настроек даёт `SettingsStore::ephemeral()` с темой System и акцентом Blue.

`OrbitApp::new` вызывается на каждый прогон окна: применяет настройки оформления к
`egui::Context` (`palette::configure_style`), ставит загрузчик SVG-иконок
(`egui_extras::install_image_loaders`, ADR-0034), системный цветной emoji-шрифт
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
   `TrayCommand` от сути, `poll_search`, `poll_roulette`, `sync_media`, `sync_tray`
   и синхронизация `spectrum_enabled`
   с движком.
2. `drain_notices` — `Session::take_notices()` → `Toast::info`/`Toast::error`; текст
   строится из `i18n::catalog(effective_language())` по ключу `Notice`, техническая
   `detail` ошибки добавляется как есть (ADR-0030).
3. `poll_favicons` — `FaviconCache::begin_frame`/`poll`, `take_fetches` → задачи
   `ArtworkFetcher::fetch` в `session.runtime` (проверки цели, MIME и размера — внутри
   `services::artwork`, ADR-0025 ред. 4), декод в `catch_unwind`, слот страхуется
   `FetchGuard`, repaint через 120 мс, пока кэш занят; по закрытии партии — один
   `debug!` со счётчиками.
4. Размер окна: `inner_rect` из ввода → `Session::set_window_size([w, h])`.
5. `Session::take_intent()` → `ViewportCommand::Focus` (`UiIntent::Show`, окно уже
   открыто — остаётся вернуть фокус) или `ViewportCommand::Close` (`UiIntent::Close`).
6. `Escape` снимает самый верхний модальный слой: при открытом окне настроек —
   `session.close_settings()`, иначе при открытой карточке —
   `session.set_help_visible(false)` (ошибка записи — тост
   `NoticeKey::SettingNotSaved`). `Ctrl+,` (он же `Cmd+,`) вызывает
   `session.toggle_settings()`.
   `Ctrl+F` и `/` (когда фокус свободен) ставят фокус в поиск текущего экрана:
   страна — `orbitfm_hud_country`, панель — `orbitfm_panel_search`, иначе
   `orbitfm_hud_search`; клавиша забирается через `consume_key`, чтобы символ не
   попал в поле. `Space` без фокуса и при закрытом окне настроек — play/pause
   (`Session::toggle_playback`), на сфокусированной строке Space остаётся запуском
   станции.
7. Repaint-политика: `SPECTRUM_REPAINT` (≈33 мс) при включённом визуализаторе и
   `Playing`/`Buffering`. `draw_layers` добавляет кадры движения: 33 мс при
   `motion.intensity > 0.01`, иначе `MOTION_IDLE_REPAINT` (≈66 мс) — только если
   эффективная реактивность включена; при `Off`/Reduce motion лишних кадров нет.
   Импульсы камеры и маркеров запрашивает сам `globe_view`.

`ui()` вызывает `draw()`: `Frame::capture` → `egui::CentralPanel` с прозрачной заливкой →
`draw_layers` (глобус, HUD, панель, transport, help, легенда, капсулы, затем в `Order::Foreground`
скрим и окно настроек) → `Toast::draw` → `apply_actions`.
`clear_color()` возвращает `[0.0, 0.0, 0.0, 0.0]` — окно прозрачное, стекло зон рисует
само.

При закрытом окне UI-цикла нет вовсе: ни `logic()`, ни `ui()` не вызываются, кадры не
аллоцируются. События в этом состоянии читает `Session::park` — `tokio::select!` по
приёмникам IPC, MPRIS и аудио-событий; команды исполняются без UI, а `UiIntent::Show`
заканчивает парковку и `run_session` перезапускает прогон. Перед входом в `park`
`settle_analysis` снимает аудио-фильтры анализа: в простое остаются декод и вывод звука
(≈2 % CPU на живом эфире), без `@level`/`@spectrum`; новый показ ставит их обратно.

## 6. Глобус, маркеры и выбор страны

Зону контента делит `center`-область: `globe_view::show` получает прямоугольник
глобуса, камеру, маркеры, состояние спектра и `&mut CalloutState`, а возвращает
`GlobeResponse { actions, clicked_point }`.

1. ПКМ + drag → `GlobeInput::drag_delta`; колесо при наведении → `scroll_delta`;
   ЛКМ → `primary_clicked`. Ввод под плавающими зонами отсекается
   `DeskLayout::blocks_point`.
2. `GlobeCamera::advance(stable_dt, elapsed, &input, &motion, allow_flips, auto_spin)`
   обновляет ориентацию, инерцию, зум, музыкальный sway, покой под курсором и
   перевороты. `allow_flips` вычисляется здесь: настройка `allow_flips` ∧ `Full` ∧
   эффективная реактивность включена; `auto_spin` — `!реактивность.is_enabled()`
   ∧ `!reduce_motion` (вращение в простое только без музыки).
   `OrbitRing::paint` рисует кольцо до `paint_globe`, поэтому сфера перекрывает
   его внутреннюю часть (ADR-0029). `GlobeState.motion` пришёл из `MotionMixer`;
   `globe_view` один раз за кадр получает `GlobeMotion` с ослабленной при
   взаимодействии геометрией, гасит её `with_calm(camera.hover_calm())` и
   передаёт `deform` шейдеру, странам, маркерам, callout и hit-test.
3. Hover страны (только в режиме «Страна»):
   `screen_to_sphere_deformed(hover_pos, viewport, &deform)` →
   `country_at_sphere_point` (при ПКМ-drag hover отключается). `globe_view` рисует
   контур наведённой страны; выбранная страна получает акцентный контур 2 px и
   заливку-heatmap с альфой 26..96 от `log10` количества станций
   (`country_station_count`: счётчик `WorldBaseline` по ISO, при пустом снимке —
   сумма видимых маркеров; ADR-0036 § 4). Курсор `PointingHand` — над страной
   или маркером, `Grabbing` — при вращении.
4. При клике сначала проверяется ближайший маркер в радиусе 10 px: он
   приоритетнее выбора страны и возвращает `UiAction::SelectStation` —
   станция запускается, выделяется и прокручивается в списке (ADR-0036 § 3).
5. Если маркера нет и режим — «Страна», `draw_layers` берёт точку
   единичной сферы (`clicked_point`) и передаёт её в
   `GeoService::country_at_sphere_point`
   (`session.geo`). Попадание создаёт `UiAction::SelectCountry`; клик по
   океану или территории без валидного ISO-кода вызывает
   `Session::clear_country_selection`: гаснут подсветка, контекст страны,
   локальный фильтр, список и маркеры, а незавершённый запрос отбрасывается
   (ADR-0036, ревизия 2026-10-31). Выделенная станция (`selected_station`)
   сохраняется, разовый `reveal_station` — нет. Тот же путь
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
   курсор-указатель. Станция в эфире получает `colors.active`, кольцо и пульс;
   выбранная (`Session::selected_station`, ADR-0036) — белое кольцо
   `SELECTED_RING_*` отдельным слоем, поэтому выбор виден и на паузе.
   `paint_globe` получает `GlobeMotion`, затем
   `paint_country_highlights` и `paint_markers` рисуют egui-оверлей с той же
   деформацией; repaint
   запрашивается, пока камера `is_animating` или на сфере виден играющий маркер.
8. Callout — два независимых канала. `GlobeState` получает
   `country_callout` и `station_callout`; оба ведёт `update_callout`
   (уходящая карточка догорает до `CALLOUT_EPSILON`, `CALLOUT_FADE_SECONDS =
   0.15 с`, при недосчитанной альфе — repaint через 16 мс).
   - Страна: активная карточка — выбранная страна (`alpha → 1.0`), иначе
     наведённая (`alpha → 0.85`), в режиме «Жанр» — `None`. Якорь —
     `country_shape(iso).focus_point()`, карточка: `flag_emoji(iso)`, название
     13.5 px, «{N} stations» (`GlobeMarker.count` по ISO), пин `live` и
     стеклянная подложка.
   - Станция: карточка активного эфира (`current_station`), якорь — её
     координаты; заголовок — имя (до 32 символов), подпись — «место · kbps ·
     кодек». Плашка живёт, пока станция играет, и гаснет при Stop.
   - `paint_callout_channel` возвращает прямоугольник видимой карточки;
     второй канал получает его как `avoid` и переворачивает/сдвигает свою
     карточку, поэтому страна и станция **никогда не перекрываются**. Обе
     клампятся в зону между `layout.hud`, `layout.transport` и панелью
     станций.

Клик по океану или территории без валидного ISO-кода снимает выбранную страну
целиком — подсветку, контекст, фильтр, список и маркеры
(`Session::clear_country_selection`, ADR-0036, ревизия 2026-10-31). При
`globe_ready = false` вместо глобуса рисуется заглушка `3D globe unavailable`, ввод
игнорируется.

## 7. Аудиовизуализатор и кольцо Listening

- В правой части transport — cava-столбцы `viz_bars` (огибающая из шести агрегатов,
  не бинный FFT — ADR-0010, ADR-0022). Отдельного чипа `Visualizer` на панели
  больше нет: включение и число столбцов живут в окне настроек (Appearance).
  Спектр-фильтр включён при старте, поэтому столбцы оживают сразу с эфиром.
- Тумблер визуализатора вызывает `UiAction::ToggleSpectrum` → `Session::set_visualizer_enabled`, ядро делает
  `runtime.block_on(AudioEngine::set_spectrum_enabled)` только если меняется
  потребность в анализе; ошибка показывается тостом и не
  прерывает воспроизведение. Состояние сохраняется в настройках
  (`Settings::visualizer_enabled`, ADR-0031) и восстанавливается при старте.
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
  начинает вращаться со скоростью 0,055 рад/с, но только при выключенной
  реактивности и без Reduce motion; наведение плавно останавливает движение
  (τ 0,3 с) — подробности в ADR-0023 и ADR-0028.
- IPC-команды для визуализатора нет: управление доступно только из GUI. Полосы
  спектра по бинам по-прежнему не поддерживаются (ADR-0010, ADR-0022).
- При закрытом окне визуализатор не обновляется: он живёт кадрами прогона, а при
  показе строится заново из `Settings::viz_bars`.

## 8. Поиск и запуск станций

- «Найти»/Enter в свободном контексте (`UiAction::RunSearch` → `Session::start_search`)
  запускают `RadioBrowserClient::search` с `name`, `limit = 100`, `hide_broken = true`;
  выбранная страна/жанр при этом сбрасываются. Если контекст активен, поле поиска
  применяется локально к уже загруженному списку (`country_filter.text`), без сетевого
  запроса, и переключает вкладку на `Results` — иначе фильтр остался бы невидимым. Запрос выполняется в tokio-задаче (`Session::begin_search`), результат приходит
  через `mpsc` и будит GUI через `WakeHandle`.
- Запрос всегда один: `Session::begin_search` не сужает выдачу фильтром
  `has_geo_info`, поэтому список полный, а маркеры — его подмножество с
  координатами (`markers = build_markers(results.clone())`, ADR-0036, ревизия
  2026-10-28 (второе)). Отдельного гео-запроса и двойных лимитов нет; ошибка
  поиска очищает и список, и маркеры.
- Маркеры строятся в ядре (`session::markers::build_markers`) из станций списка:
  координаты должны быть конечными, лежать в `[-90, 90]`/`[-180, 180]` и не быть
  `(0, 0)`; станции с координатами, округлёнными до 2 знаков, объединяются,
  дубликаты по UUID отбрасываются.
- Маркеры рисуются одинаковыми (радиус 3 px, единый `palette.marker`); качество
  и рейтинг не кодируются цветом или размером, а показываются в подписи при
  наведении: место · kbps · CODEC · голоса. Играющая станция получает
  кольцо-пульс; выбранная — белое кольцо; наведение — светлое кольцо и подпись
  (`MarkerLabel`, ADR-0012).
- Список и глобус делят одно выделение: строка играющей или выбранной станции
  получает accent-плашку и accent-имя, маркер — кольцо выбора. Клик по маркеру
  (`UiAction::SelectStation`) запускает станцию, переключает панель на `Results`
  и один раз прокручивает строку в центр (`reveal_station` →
  `take_station_reveal`, `Response::scroll_to_me`); у первых и последних строк
  egui прижимает прокрутку к границам, и строка всё равно видна (ADR-0036 § 2–3).
- Поиск по стране использует тот же канал и отличается только `SearchParams`
  (`country_code`, без `name`, `limit = 100`). Заголовок панели `Country: …`
  остаётся до следующего поиска или смены режима.
- Список страны — **полная выдача** Radio Browser (`hide_broken=true`, без
  `has_geo_info`); маркеры строятся из тех же строк, но только с координатами,
  поэтому каждый маркер имеет строку, а строка без координат маркера не имеет
  (ADR-0036, обновление 2026-10-28 (второе)).
- Пагинация: первая страница — 100 строк, «загружено из всего» берёт всего из
  мирового снимка (`Session::country_total`). `Session::load_more` просит
  `offset = search.next_offset`, ответ с `append: true` дополняет список без
  повторных UUID и пересобирает маркеры; ошибка догрузки оставляет список и
  показывает тост `LoadMoreFailed`. При известном счётчике пагинация идёт до
  него, даже если сервер вернул короткую страницу; пустая страница закрывает её.
- Поиск по жанру: `tag = <тег>`, `limit = 200`. Пресеты и поле своего тега
  находятся в popup, который открывается **прямо кликом по пилюле `Genre`**
  (`ui/mode_switch.rs`), без отдельной кнопки выбора. Выбор запускает
  `Session::start_genre_search`, который очищает `genre_query` и сбрасывает
  локальные фильтры. Заголовок панели — `Genre: …`.
- `SearchMode` переключается только кнопками HUD (`UiAction::SwitchMode` →
  `Session::switch_mode`); смена режима очищает контекст, результаты, маркеры, фильтры и
  вкладку `Results`.
- Каждый запуск поиска увеличивает `search.generation`; `SearchResponse` несёт
  номер поколения, `Session::poll_search` отбрасывает ответы прошлого режима/жанра и не
  возвращает устаревшие маркеры.
- `search.revision` — отдельный счётчик изменений выборки (старт, успех, ошибка,
  смена режима). Вместе с `WorldBaseline::revision` он входит в ключ
  `ui::country_map::CountryHeatCache`: тепловая карта стран пересобирается при
  смене выборки, прихода снимка мира или смены метрики; `generation` для этого
  не годится, потому что не меняется при завершении запроса.
- Клик мимо маркера и стран снимает выбранную страну целиком:
  `SearchState::clear_country` очищает `context.country`, результаты и
  незавершённый запрос, `Session::clear_country_selection` добавляет сброс
  `country_filter`, маркеров и `reveal_station` (ADR-0036, ревизия 2026-10-31).
  Отдельного флага «подсветка скрыта, а список остался» больше нет.
- HUD «Страна» вызывает `GeoService::search_countries` (до 6 подсказок) при каждом
  изменении запроса; Enter выбирает первую, клик — выбранную. Оба пути создают
  `UiAction::SelectCountry`, `Session::start_country_search` очищает `country_query` и
  сбрасывает `country_filter`.
- Панель страны применяет `station_filter::filtered_results` к уже загруженным
  результатам: текст ищется в `name/tags/country/state`; выбранные жанры
  проверяют теги без учёта регистра по OR (достаточно одного совпадения);
  сортировка — по `votes`, `bitrate` или имени. Это локальные операции без сети;
  карточки показывают state/country, kbps, codec, теги и голоса. При пустом
  результате фильтров — «Quiet in this sector» / «No station matches the
  filters.» и кнопка `Reset filters`, сохраняющая выбранную сортировку и
  видимость строки фильтров.
- Строка фильтров: поле поиска, две иконки (`Funnel` — показать/скрыть фильтры,
  `FunnelX` — очистить), выпадающий мультивыбор жанров с `CloseOnClickOutside`
  и бейджи выбранных жанров с крестиком. Скрытие строки не сбрасывает выбор;
  очистка снимает текст и жанры, но оставляет сортировку и плотность.
- Пустой успешный ответ выставляет `search.empty`; UI показывает `No stations found`.
- Новый поиск очищает маркеры; ошибка поиска также очищает их.
- Выбор результата или IPC `play <uuid>` вызывает `Session::start_station`:
  проверяется непустой `url_resolved`, выполняется `audio.play`, сохраняются
  `current_url`/`current_station`/`selected_station`, асинхронно отправляется
  click (`Session::spawn_click`).
- `play <uuid>` сначала ищет станцию в последних результатах поиска
  (`Session::resolve_station`), затем вызывает `get_station` с таймаутом 8 с
  (`STATION_LOOKUP_TIMEOUT`); таймаут маппится в `RadioError::Network`.

### 8.1. Roulette

- Кнопка `Radio Roulette` в transport вызывает `Session::start_roulette`; пока
  `search.roulette_pending`, повторные нажатия игнорируются, кнопка заблокирована.
- `RadioBrowserClient::random_station(&exclude)` выполняется в tokio-задаче:
  пачка из 24 станций со случайным `offset`, фильтр по потоку/координатам и
  исключение текущего эфира и 12 последних записей истории; результат приходит
  через `mpsc` и будит GUI. Поэтому повторные нажатия дают разные станции даже
  при серверном кэше Radio Browser.
- `Session::poll_roulette` запускает станцию, при успехе перелетает камерой за 900 мс
  (`ROULETTE_FLY_MS`) к её координатам и отдаёт `Notice::Info(NoticeKey::RouletteStarted
  { station })`: метка `«имя — страна»` (`roulette_label`) подставляется в строку
  каталога `Roulette: {station}` через `i18n::fill`. Ошибка каталога или запуска —
  `NoticeKey::RouletteFailed` с технической `detail`, задача умерла без ответа —
  `RouletteTaskEnded`; всё это тосты, текущий эфир они не меняют.
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
- Окно настроек (шестерёнка в HUD, `Ctrl+,`, пункт `Settings…` в трее; ADR-0031) —
  модальное окно 760×544 с колонкой категорий: `Appearance` (тема
  `System`/`Dark`/`Light` — `controls::dropdown`, шесть пресетов акцента (`Orbit`, `Emerald`,
  `Amber`, `Violet`, `Rose`, `Ocean`) — круглые свотчи, тумблер визуализатора, число столбцов — dropdown), `Language`
  (dropdown, `System` и шестнадцать языков), `Audio` (громкость), `Motion` (`Off`/`Low`/`Normal`/`High`
  и стиль — dropdown, `Reduce motion` и перевороты — тумблеры), `Window` (режим окна — dropdown,
  `Show help` — тумблер) и `About` (версия, лицензия, app-id, «Выход»). Изменение применяется сразу и
  сохраняется через `SettingsStore`; ошибка записи показывается тостом и не прерывает
  воспроизведение. Строка настройки собирается `controls::row`: подпись слева, контрол в
  фиксированном слоте справа (220×30 для dropdown, 184×26 для свотчей), приглушённая
  подсказка под подписью. `Reduce motion` не перезаписывает `motion`: эффективная
  реактивность просто становится `Off`. Тема, акцент и язык после сохранения
  повторяют `palette::configure_style` — он же обнуляет `Style::animation_time` при
  reduce motion (ADR-0032). Громкость из окна настроек применяется к аудио на перетаскивании
  (`UiAction::PreviewVolume`) и сохраняется только при отпускании
  (`UiAction::CommitVolume`, ADR-0031); колесо над полосой даёт ±5 % за щелчок и
  пишет файл сразу, как стрелки, — дельта при этом потребляется, чтобы окно под
  курсором не прокручивалось.
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
- IPC-команд для оформления нет: настройка доступна из GUI и из меню трея
  (`TrayCommand::SetTheme` для темы, `TrayCommand::Settings` открывает настройки).

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

- `×` в HUD (tooltip «Hide window (audio keeps playing)») даёт `UiAction::Hide` → `Session::hide()` → `UiIntent::Close`; `logic()`
  шлёт `ViewportCommand::Close`, `run_native` возвращается (`run_and_return: true`),
  `Session::end_window` помечает окно закрытым, снимает `WakeHandle`, дописывает
  preview-громкость (`flush_pending_volume`, ADR-0031) и обновляет MPRIS с треем.
  Запрос композитора закрыть окно идёт тем же путём — никакого перехвата.
- В закрытом состоянии нет ни поверхности, ни кадров: `run_session` вызывает
  `Session::park()`, который читает IPC, MPRIS, команды трея и аудио-события. `show`/`toggle` в этом
  состоянии дают `UiIntent::Show`, `park` выходит с `ParkOutcome::Show`, и цикл
  повторяется: `restore_size()` → `begin_window(size)` → `run_window` (второй и каждый
  следующий `eframe::run_native` из того же главного потока). `Settings…` из меню трея
  идёт через `Session::open_settings`: `Listening` переключается на `Overlay`,
  ставится `settings_open`, а при скрытом окне создаётся `UiIntent::Show` (ADR-0031).
- Размеры применяются до создания окна: `NativeOptions.viewport` собирается из
  `Session::restore_size()` (`with_inner_size` + `with_min_inner_size`,
  `WINDOW_SIZE` 1100×800 по умолчанию, `WINDOW_MIN_SIZE` 720×480), а фактический размер
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

- Тепловая карта стран (ADR-0035): плотность считается по глобальным счётчикам
  `/json/countrycodes`, а жанры, качество, тип и рейтинг — по выборке панели или
  по мировому топу из 1000 станций, когда выборки нет. Страны без популярных
  станций в этих четырёх режимах не окрашены; источник подписан в легенде
- Снимок мира не кэшируется на диск: ≈1,1 МБ на запуск при включённой карте
- Roulette доступна только из GUI и всегда запускает воспроизведение; «только
  показать станцию без эфира» не поддерживается
- Hover страны вычисляется raycast-ом каждый кадр без кэша последней точки
- `Next`/`Previous` из MPRIS и кнопки `⏮`/`⏭` в transport идут по текущему видимому
  списку (`play_relative(±1)` с циклом): очередь — снимок панели, а не постоянный
  плейлист (ADR-0013 переопределён фазой 6 для этого поведения)
- На глобусе нет подписей стран, атмосфера ограничена Fresnel-ободом; маркеры
  группируются только по совпадающим координатам, кластеризации близких точек нет
- Клик мимо окна (overlay) ещё не реализован
- Кэш станций каталога живёт в памяти; на диске — избранное, история и
  офлайн-снимок до 500 станций (`stations-snapshot.json`, ADR-0043)
- Метаданные ICY (название трека внутри потока) не читаются
- FFT-визуализатор управляется только из GUI, не имеет IPC-команды и показывает
  агрегированные метрики вместо полос спектра
- Акцент выбирается только из шести пресетов, произвольный цвет не поддерживается
  (ADR-0014); тема System следует системной теме, которую сообщает eframe
- Светлая палитра производная от `egui::Theme::Light`; отдельной ручной настройки
  контрастности нет
- Интерфейсные глифы закрыты встроенными шрифтами `src/ui/fonts.rs` (ADR-0041):
  латиница, кириллица, арабский, CJK-подмножество и флаги. Деванагари (хинди) и
  редкие иероглифы в названиях станций по-прежнему берутся из системного шрифта
  через fontconfig; без него останутся tofu-квадратами
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
  ADR-0016, ADR-0017, ADR-0018, ADR-0019, ADR-0028, ADR-0029, ADR-0030, ADR-0031,
  ADR-0032
- `docs/modules/11-session.md`, `docs/interfaces/10-session.md` — контракт Application Core
- `docs/interfaces/07-app.md` — контракт presentation-слоя
- `docs/modules/13-i18n.md`, `docs/interfaces/12-i18n.md` — каталог строк и `NoticeKey`
- `docs/modules/14-ui-settings.md` — окно настроек и громкость
- `docs/modules/15-ui-motion.md` — переходы (`animation.rs`, `Transitions`)
- `docs/architecture/00-overview.md`
- `docs/ui/design-vision.md`, `docs/ui/implementation-plan.md`
- `docs/modules/09-library.md`
- `docs/modules/10-settings.md`
- `wm-configs/orbitfm-niri.kdl`, `wm-configs/orbitfm-hyprland.conf` — правила окон
