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

Текущее состояние (Этап 8, фаза 1):

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
    spectrum_enabled: bool,
    current_url: Option<String>,
    current_station: Option<Station>,
    search_query: String,
    search_country: Option<Country>,
    search_results: Vec<Station>,
    markers: Vec<GlobeMarker>,
    library_tab: LibraryTab,
    search_pending: bool,
    search_empty: bool,
    search_rx: Option<mpsc::UnboundedReceiver<Result<SearchOutcome, RadioError>>>,
    search_error: Option<String>,
    panel_open: bool,
    hint_until: f64,
    toast: Toast,
    runtime: tokio::runtime::Handle,
    quitting: bool,
}
```

`globe_ready` — результат создания `GlobeRenderer` из `CreationContext::wgpu_render_state`.
`spectrum_enabled` синхронизируется с аудио-ядром в `logic()`, `spectrum_overlay` хранит
сглаженное состояние оверлея; ошибки включения фильтра показываются тостом.
`search_country` хранит страну, выбранную на глобусе, и сбрасывается при поиске по
названию; `search_empty` отмечает успешный ответ без станций. `SearchOutcome` —
внутренний тип с результатами основного и гео-запроса; `GlobeMarker` (в
`ui/globe_view.rs`) хранит представителя группы, координаты и количество станций.

`current_url` заполняется из `AudioEvent::Started` и из запуска станции; `Stopped`/`Error`
его больше не очищают — последний URL нужен MPRIS для повторного запуска.
`current_station` хранит метаданные активной станции и источник для `status` и MPRIS.
`media` — `Option`, потому что MPRIS может быть недоступен (нет D-Bus); приложение в этом
случае продолжает работать.

`library` хранит избранное и историю (см. §9), `library_tab` — активную вкладку списка
(`Results`/`Favorites`/`History`, по умолчанию `Results`). Ошибки чтения/записи
библиотеки и настроек показываются тостами и не остаются в состоянии приложения.

`settings` хранит тему и акцент (см. §10). `panel_open` управляет видимостью drawer,
`hint_until` — срок жизни подсказки первого запуска, `toast` — одно текущее сообщение.

### 3.1. UI-слой (Фаза 1)

UI вынесен из `app.rs` в `src/ui/`:

```
src/ui/
├── mod.rs            # UiAction, playback_label, single_line, truncate
├── palette.rs        # Palette, configure_style, theme_* / accent_*
├── hud.rs            # верхний HUD: поиск, now-playing, громкость, меню
├── station_panel.rs  # правый drawer: вкладки, строки станций, избранное/история
├── transport.rs      # нижний transport: play/pause, stop, звезда, детали, FFT
├── globe_view.rs     # ввод камеры, маркеры, FFT-оверлей, подсказка
├── toast.rs          # одно сообщение на 3 секунды
└── icons.rs          # векторные иконки star/play/stop
```

Правила:

- UI-модули получают только нужные срезы состояния, возвращают `Vec<UiAction>`;
- текстовое поле поиска и активная вкладка редактируются напрямую через `&mut`;
- UI не вызывает `AudioEngine`, `RadioBrowserClient`, `StationLibrary` и `SettingsStore`;
- `app.rs` — единственная точка побочных эффектов: `apply_actions` выполняет
  `Play`, `ToggleFavorite`, `SelectCountry`, `RunSearch`, `ClearHistory`, `SetTheme`,
  `SetAccent`, `ToggleSpectrum`, `SetVolume`, `TogglePlayback`, `Stop`, `Hide`, `Quit`,
  `TogglePanel`.

`UiAction` — внутренний тип, не публичный контракт. `render` по-прежнему не импортирует
доменные типы: `globe_view` передаёт в `paint_globe` камеру и готовые маркеры.

### 3.2. Focus-раскладка

`draw()` строит три зоны внутри `CentralPanel` с отступом 8 px:

| Зона | Размер | Содержимое |
|------|--------|------------|
| HUD | 58 px, не более 20% высоты окна | логотип, поиск и «Найти», now-playing, громкость, «Список», «Настройки» |
| Контент | остальная высота | глобус на всю ширину; правый drawer 310 px, не более 42% ширины |
| Transport | 48 px, не более 15% высоты окна | play/pause, stop, звезда, codec/kbps/страна, FFT |

Минимальные высоты: HUD — 48 px, transport — 40 px. При ширине окна < 900 px drawer
становится `Area` поверх глобуса (foreground, 310 px) с кнопкой «×»; глобус занимает
всю ширину контента. Drawer сворачивается кнопкой «Список» в HUD.

Иконки play/pause, stop и звезды рисуются через `Painter` в `ui/icons.rs`
(треугольник, прямоугольники, triangle-fan mesh) и не зависят от набора глифов
шрифта egui.

`Toast` показывает одно сообщение 3 секунды внизу по центру над transport.
Подсказка первого запуска живёт 10 секунд и исчезает после первого взаимодействия
с глобусом; в файл флаг не сохраняется.

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
5. Синхронизирует состояние с MPRIS (`sync_media`).
6. Перехватывает закрытие окна (`close_requested`) и превращает его в Hide, если это не
   `quit`.

`ui()` рисует Focus-раскладку только при видимом окне. В eframe 0.37 скрытое окно не
вызывает `ui()`, но `logic()` продолжает выполняться при запрошенном repaint. Поэтому
IPC- и MPRIS-команды обрабатываются и в скрытом состоянии; пробуждение выполняют
`WakeFn` из IPC-сервера, MPRIS-колбэка, задача аудио-событий и завершённый поиск.

`draw()` собирает `Vec<UiAction>` от `hud`, `globe_view`, `station_panel` и `transport`,
затем применяет их в `apply_actions`; побочные эффекты выполняются только после
отрисовки кадра.

## 6. Глобус, маркеры и выбор страны

Зону контента делит `center`-область: `globe_view::show` получает прямоугольник
глобуса, камеру, маркеры и FFT-состояние, а возвращает
`GlobeResponse { actions, clicked_point, interacted }`.

1. ПКМ + drag → `GlobeInput::drag_delta`; колесо при наведении → `scroll_delta`;
   ЛКМ → `primary_clicked`.
2. `GlobeCamera::update(stable_dt, &input)` обновляет ориентацию, инерцию и зум.
3. При клике сначала проверяется ближайший маркер в радиусе 10 px: он приоритетнее
   выбора страны и возвращает `UiAction::Play`.
4. Если маркера нет, `screen_to_sphere` даёт точку сферы; `app.rs` передаёт её в
   `GeoService::country_at_sphere_point` и при попадании создаёт
   `UiAction::SelectCountry`.
5. Найденная страна передаётся в `start_country_search`: `search_country`
   устанавливается, выполняется запрос `countrycode=<ISO alpha-2>`, `limit = 30`,
   `hide_broken = true`.
6. Маркеры проецируются через `project_on_sphere`; наведение показывает подпись и
   курсор-указатель, активная станция подсвечивается. `paint_globe` добавляет paint
   callback, затем `paint_markers` рисует egui-оверлей; пока камера `is_animating`,
   запрашивается repaint.

Клик по океану или территории без валидного ISO-кода ничего не меняет. При
`globe_ready = false` вместо глобуса рисуется заглушка, ввод игнорируется.
Любое взаимодействие с глобусом скрывает подсказку первого запуска.

## 7. FFT-визуализатор

- В правой части transport — чекбокс «FFT» с подсказкой; по умолчанию выключен.
- Переключение вызывает `AudioEngine::set_spectrum_enabled` через
  `runtime.block_on`; ошибка показывается тостом и не прерывает воспроизведение.
- Каждый кадр `logic()` синхронизирует `spectrum_enabled` с движком. В `ui()`
  `AudioEngine::spectrum()` преобразуется в `SpectrumTarget` и передаётся в
  `SpectrumOverlay::update`; оверлей рисуется в нижней части области глобуса.
- Пока оверлей виден или визуализатор включён при `Playing`/`Buffering`, запрашивается
  repaint с интервалом ~33 мс.
- IPC-команды для FFT нет: управление доступно только из GUI. Полосы спектра по бинам
  не поддерживаются (ADR-0010).

## 8. Поиск и запуск станций

- «Найти»/Enter запускают `RadioBrowserClient::search` с `name`, `limit = 30`,
  `hide_broken = true`; `search_country` сбрасывается. Запрос выполняется в
  tokio-задаче, результат приходит через `mpsc` и будит GUI.
- Параллельно с основным запросом выполняется гео-запрос с теми же фильтрами,
  `has_geo_info = true`, `limit = 60`; его ошибка логируется и не влияет на список.
  Результат используется только для маркеров.
- Маркеры строятся из гео-станций: координаты должны быть конечными, лежать в
  `[-90, 90]`/`[-180, 180]` и не быть `(0, 0)`; станции с координатами, округлёнными
  до 2 знаков, объединяются, дубликаты по UUID отбрасываются.
- Поиск по стране использует тот же канал и отличается только `SearchParams`
  (`country_code`, без `name`); заголовок `Страна: …` остаётся до следующего
  поиска по названию.
- Пустой успешный ответ выставляет `search_empty`; UI показывает «Станции не найдены».
- Новый поиск очищает маркеры; ошибка основного запроса также очищает их.
- Выбор результата или IPC `play <uuid>` вызывает `start_station`:
  проверяется непустой `url_resolved`, выполняется `audio.play`, сохраняются
  `current_url`/`current_station`, асинхронно отправляется click.
- `play <uuid>` сначала ищет станцию в последних результатах поиска, затем вызывает
  `get_station` с таймаутом 8 с; таймаут маппится в `RadioError::Network`.

## 9. Избранное и история

- `StationLibrary` загружается в `main` до запуска GUI; ошибка загрузки даёт
  эфемерную библиотеку без файла (ADR-0013).
- Успешный `start_station` (строка результатов, избранного или истории, маркер,
  IPC `play`) вызывает `record_played`: станция перемещается в начало истории,
  история ограничена 20 записями. Возобновление из MPRIS (`Play`/`PlayPause`)
  историю не меняет.
- Переключение звезды вызывает `toggle_favorite` и запрашивает repaint; звезда есть
  у текущей станции (transport) и у каждой строки списка (drawer). Ошибка сохранения
  показывается тостом.
- Под строкой поиска и маркеров показываются вкладки «Результаты»,
  «Избранное (N)», «История (N)», если есть результаты или сохранённые станции.
  Список прокручивается в drawer на всю доступную высоту; выбор строки запускает
  станцию.
- Новый поиск сбрасывает вкладку на «Результаты»; вкладка «История» содержит кнопку
  «Очистить историю» (`clear_history`) с подтверждением тостом.
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
- Обычные маркеры получают `palette.marker`, активный маркер сохраняет тёплый
  янтарный цвет.
- IPC-команд для оформления нет: настройка доступна только из GUI.

## 11. Синхронизация с MPRIS

`sync_media()` вызывается каждый кадр, но `MediaControls::set_playback` дедуплицирует
неизменившееся состояние. Соответствие:

- `Stopped`/`Error` → трек отсутствует, `Stopped`;
- `Paused` → трек есть, `Paused`;
- `Playing`/`Buffering`/`Reconnecting` → трек есть, `Playing`.

Метаданные берутся из `current_station`: `name` → title, `tags` → artist, `favicon` →
artUrl, `url_resolved` → `xesam:url`. Без станции используется URL потока.

## 12. Жизненный цикл окна

- Закрытие крестиком → `ViewportCommand::CancelClose` + `ViewportCommand::Visible(false)`,
  `window_visible = false`
- Команда `toggle` / `show` → `Visible(true)` + `Minimized(false)` + `Focus`; если окно
  свёрнуто оконным менеджером, `toggle` восстанавливает его, а не скрывает
- Команда `quit` → `quitting = true` + `ViewportCommand::Close`; после выхода из GUI main
  останавливает IPC-сервер, аудио-поток и удаляет сокет

## 13. Текущие ограничения

- Roulette, режимы Country/GlobalGenre, hover/контур страны, fly-to и heatmap ещё не
  реализованы: это фазы 2–5 Этапа 8 (ADR-0015)
- `Next`/`Previous` из MPRIS не переключают станции: история не является очередью
  воспроизведения, для переключения нужен отдельный плейлист (ADR-0013)
- На глобусе нет подписей стран и атмосферы; маркеры группируются только по
  совпадающим координатам, кластеризации близких точек нет
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

- ADR-0002, ADR-0005, ADR-0006, ADR-0010, ADR-0012, ADR-0013, ADR-0014, ADR-0015
- `docs/architecture/00-overview.md`
- `docs/ui/design-vision.md`, `docs/ui/implementation-plan.md`
- `docs/modules/09-library.md`
- `docs/modules/10-settings.md`
