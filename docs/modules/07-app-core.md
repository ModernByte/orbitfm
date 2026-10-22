# Модуль: Application Core

**Путь в коде:** `src/app.rs`, `src/main.rs`  
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

Текущее состояние (Этап 6):

```rust
pub struct AppState {
    pub window_visible: bool,
}

pub struct AppServices {
    pub audio: AudioEngine,
    pub geo: GeoService,
    pub radio: RadioBrowserClient,
    pub media: Option<MediaControls>,
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
    globe_camera: GlobeCamera,
    globe_ready: bool,
    spectrum_overlay: SpectrumOverlay,
    spectrum_enabled: bool,
    spectrum_error: Option<String>,
    current_url: Option<String>,
    current_station: Option<Station>,
    search_query: String,
    search_country: Option<Country>,
    search_results: Vec<Station>,
    search_pending: bool,
    search_empty: bool,
    search_rx: Option<mpsc::UnboundedReceiver<Result<SearchOutcome, RadioError>>>,
    search_error: Option<String>,
    markers: Vec<StationMarker>,
    runtime: tokio::runtime::Handle,
    quitting: bool,
}
```

`globe_ready` — результат создания `GlobeRenderer` из `CreationContext::wgpu_render_state`.
`spectrum_enabled` синхронизируется с аудио-ядром в `logic()`, `spectrum_overlay` хранит
сглаженное состояние оверлея, `spectrum_error` — последняя ошибка включения фильтра.
`search_country` хранит страну, выбранную на глобусе, и сбрасывается при поиске по
названию; `search_empty` отмечает успешный ответ без станций. `SearchOutcome` —
внутренний тип с результатами основного и гео-запроса; `StationMarker` хранит
представителя группы, координаты и количество станций.

`current_url` заполняется из `AudioEvent::Started` и из запуска станции; `Stopped`/`Error`
его больше не очищают — последний URL нужен MPRIS для повторного запуска.
`current_station` хранит метаданные активной станции и источник для `status` и MPRIS.
`media` — `Option`, потому что MPRIS может быть недоступен (нет D-Bus); приложение в этом
случае продолжает работать.

## 4. Порядок инициализации

```
IpcServer::try_bind → AudioEngine::new → GeoService::from_embedded
→ MediaControls::new(wake) → RadioBrowserClient::new → фоновый refresh_servers
→ задача audio events → wake → IpcServer::run → eframe → OrbitApp::new
```

`try_bind` выполняется до создания аудио-ядра, чтобы второй экземпляр завершился, не
занимая аудиоустройство. Ошибки `AudioEngine::new` и загрузки гео-датасета фатальны;
ошибки MPRIS и прогрева каталога — только предупреждения.

`OrbitApp::new` создаёт `GlobeRenderer` из `cc.wgpu_render_state` и вставляет его в
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

`ui()` рисует карточку только при видимом окне. В eframe 0.37 скрытое окно не вызывает
`ui()`, но `logic()` продолжает выполняться при запрошенном repaint. Поэтому IPC- и
MPRIS-команды обрабатываются и в скрытом состоянии; пробуждение выполняют `WakeFn` из
IPC-сервера, MPRIS-колбэка, задача аудио-событий и завершённый поиск.

## 6. Глобус, маркеры и выбор страны

В `ui()` область глобуса выделяется через `allocate_exact_size` с
`Sense::click_and_drag()`:

1. ПКМ + drag → `GlobeInput::drag_delta`; колесо при наведении → `scroll_delta`;
   ЛКМ → `primary_clicked`.
2. `GlobeCamera::update(stable_dt, &input)` обновляет ориентацию, инерцию и зум.
3. При клике сначала проверяется ближайший маркер в радиусе 10 px: он приоритетнее
   выбора страны и запускает представителя группы через `start_station`.
4. Если маркера нет, `screen_to_sphere` даёт точку сферы,
   `GeoService::country_at_sphere_point` — страну.
5. Найденная страна передаётся в `start_country_search`: `search_country`
   устанавливается, выполняется запрос `countrycode=<ISO alpha-2>`, `limit = 30`,
   `hide_broken = true`.
6. Маркеры проецируются через `project_on_sphere`; наведение показывает подпись и
   курсор-указатель, активная станция подсвечивается. `paint_globe` добавляет paint
   callback, затем `paint_markers` рисует egui-оверлей; пока камера `is_animating`,
   запрашивается repaint.

Клик по океану или территории без валидного ISO-кода ничего не меняет. При
`globe_ready = false` вместо глобуса рисуется заглушка, ввод игнорируется.

## 7. FFT-визуализатор

- В заголовке карточки — чекбокс «FFT» с подсказкой; по умолчанию выключен.
- Переключение вызывает `AudioEngine::set_spectrum_enabled` через
  `runtime.block_on`; ошибка показывается в карточке и не прерывает воспроизведение.
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

## 9. Синхронизация с MPRIS

`sync_media()` вызывается каждый кадр, но `MediaControls::set_playback` дедуплицирует
неизменившееся состояние. Соответствие:

- `Stopped`/`Error` → трек отсутствует, `Stopped`;
- `Paused` → трек есть, `Paused`;
- `Playing`/`Buffering`/`Reconnecting` → трек есть, `Playing`.

Метаданные берутся из `current_station`: `name` → title, `tags` → artist, `favicon` →
artUrl, `url_resolved` → `xesam:url`. Без станции используется URL потока.

## 10. Жизненный цикл окна

- Закрытие крестиком → `ViewportCommand::CancelClose` + `ViewportCommand::Visible(false)`,
  `window_visible = false`
- Команда `toggle` / `show` → `Visible(true)` + `Minimized(false)` + `Focus`; если окно
  свёрнуто оконным менеджером, `toggle` восстанавливает его, а не скрывает
- Команда `quit` → `quitting = true` + `ViewportCommand::Close`; после выхода из GUI main
  останавливает IPC-сервер, аудио-поток и удаляет сокет

## 11. Текущие ограничения

- `Next`/`Previous` из MPRIS не переключают станции (нет списка/истории)
- На глобусе нет подписей стран и атмосферы; маркеры группируются только по
  совпадающим координатам, кластеризации близких точек нет
- Клик мимо окна (overlay) ещё не реализован
- Кэш станций только in-memory; дискового кэша нет
- Метаданные ICY (название трека внутри потока) не читаются
- FFT-визуализатор управляется только из GUI, не имеет IPC-команды и показывает
  агрегированные метрики вместо полос спектра

## 12. Связанные документы

- ADR-0002, ADR-0005, ADR-0006, ADR-0010, ADR-0012
- `docs/architecture/00-overview.md`
