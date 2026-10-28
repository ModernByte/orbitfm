# Интерфейс: Presentation (OrbitApp)

Presentation-слой не экспортирует публичный трейт для других crate’ов — он является точкой
сборки кадра и цикла окна. Контракт Application Core (`Session`, `AppServices`, `WindowState`,
`Notice`, `UiIntent`, `WakeHandle`, `ParkOutcome`, маршрутизация `IpcCommand`/`MediaCommand`)
зафиксирован отдельно: `docs/interfaces/10-session.md` и `docs/modules/11-session.md`.

Тем не менее, фиксируем контракт поведения.

Окно = один прогон UI-цикла (ADR-0018). `OrbitApp<'a>` заимствует `&'a mut Session` ровно на
время одного `eframe::run_native`, поэтому «скрыть окно» — это честное закрытие
root-вьюпорта: цикл завершается, управление возвращается в `run_session`, а всё
долгоживущее (подсистемы, текущая станция, поиск, маркеры, камера, геометрия окна,
признак выхода) остаётся в `Session`. В eframe 0.37 `OrbitApp` реализует `logic()`, `ui()`
и `clear_color()`.

## Обязанности

1. Инициализировать все подсистемы в корректном порядке (`launch`, `src/main.rs`),
   передать их в `Session` как `AppServices` и при выходе остановить IPC-сервер.
2. Реализовывать `eframe::App` (`logic` + `ui` + `clear_color` в eframe 0.37) — ровно один
   прогон на одно окно.
3. Вести цикл показа и скрытия окна: `restore_size` → `begin_window` →
   `settle_analysis` → `run_window` → `end_window` → `is_quitting` →
   `take_reopen` → `close_settings` → `settle_analysis` → `park` (§«Порядок инициализации»,
   шаги 11–12).
   `settle_analysis` включает аудио-анализ на время открытого окна и снимает его в
   простое: скрытый эфир не тратит CPU на `@level`/`@spectrum` (ADR-0027). Drawer
   настроек закрывается на пути к парковке; `take_reopen` (смена режима) уходит на новый
   прогон раньше, поэтому drawer переживает только её (ADR-0031).
   Маршрутизацией `IpcCommand` и `MediaCommand` занимается `Session`: очередь
   `mpsc::Receiver<IpcRequest>` и приёмник MPRIS принадлежат ядру и обслуживаются как из
   `Session::tick()` (окно открыто), так и из `Session::park()` (окно закрыто).
4. Держать ресурсы одного прогона: тему и акцент (`palette::configure_style`), системный
   цветной emoji-шрифт (ADR-0017), `GlobeRenderer`, вставленный в `callback_resources`
   wgpu-рендерера, `FaviconCache` с его текстурами, `reqwest::Client` для иконок,
   `SpectrumOverlay`, `motion: MotionMixer`, `orbit_ring: OrbitRing`, `CalloutState`,
   `VisualizerModel`, `Toast`, `transitions: Transitions` с прогрессами переходов зон
   (ADR-0032) и `drawer_rect` — последний прямоугольник drawer'а, по которому гаснет
   панель настроек (ADR-0031, ADR-0032).
5. Устанавливать `WakeHandle` в начале прогона (`OrbitApp::new` → `wake.install`, замыкание
   зовёт `Context::request_repaint`) и оставлять его снятым после закрытия окна
   (`Session::end_window` → `wake.clear`), чтобы фоновые задачи не будили несуществующий цикл.
6. Сбирать кадр и исполнять ввод: `Frame::capture` снимает читаемое состояние в снапшот,
   `draw_layers` рисует зоны и собирает их `Vec<UiAction>`, `apply_actions` переводит
   действия в вызовы методов `Session`. Побочные эффекты — только в `apply_action`.
7. Транслировать намерения ядра в команды вьюпорта: `UiIntent::Show` →
   `ViewportCommand::Focus` (окно уже открыто), `UiIntent::Close` →
   `ViewportCommand::Close` (закрытие root-вьюпорта и возврат из `run_native`).
8. Показывать сообщения ядра: `Session::take_notices` → `Toast::info` / `Toast::error`;
   как именно показать `Notice`, решает UI, ядро про тосты ничего не знает. Текст
   строится из `i18n::catalog(session.effective_language())` по ключу `NoticeKey`,
   техническая `detail` ошибки дописывается как есть и не переводится (ADR-0030).
9. При клике по глобусу в режиме «Страна»:
   - при попадании в маркер станции (радиус 10 px) `globe_view` возвращает
     `UiAction::SelectStation`, и `apply_action` вызывает
     `Session::select_station` (выделение и прокрутка строки) и затем
     `Session::start_station` (запускается представитель группы, ADR-0036);
   - иначе `globe_view` возвращает точку на сфере, полученную от
     `GlobeCamera::screen_to_sphere_deformed` с текущей `GlobeMotion.deform`
     (`GlobeResponse::clicked_point`);
   - `src/app.rs` спрашивает страну у `GeoService::country_at_sphere_point` и создаёт
     `UiAction::SelectCountry`;
   - решение принимает ядро: `Session::focus_country` мягко доворачивает камеру к центру
     страны (`country_shape().focus_point()`, 650 мс), `Session::start_country_search`
     запрашивает станции через `RadioBrowserClient` с фильтром `countrycode`;
   - UI отображает новое состояние `Session` в следующем кадре (заголовок страны,
     результаты, пустое состояние).

   В режиме «Глобальный жанр» клик по стране не меняет контекст (no-op): проверка страны
   выключена (`country_enabled = false`); маркеры станций остаются кликабельными.
10. Для каждого поиска ядро выполняет один запрос без гео-фильтра (`has_geo_info`
    выключен) и строит маркеры из тех же строк, что и список
    (`session::markers::build_markers`): маркер получает только станция с корректными
    координатами, поэтому `<Session::markers> ⊆ Session::search.results` (ADR-0036 § 1,
    ревизия 2026-10-28). Список страны пагинируется: первая страница — 100 строк,
    подпись панели показывает «загружено из всего» по мировому счётчику
    (`Session::country_total`), кнопка «Показать ещё» вызывает `UiAction::LoadMore` →
    `Session::load_more` (offset, дедупликация по UUID), пока
    `search.has_more`; ошибка догрузки не очищает список, а отдаёт тост
    `LoadMoreFailed`. Лимиты: название и страна — 100, жанр — 200. Ошибка первого
    поиска очищает и список, и маркеры. Presentation-слой только рисует
    `Session::markers` и передаёт в панель `total_available`, `loading_more`,
    `has_more`.
11. Для выбранной страны или жанра локальные фильтры (`text`, мультивыбор `genres`
    с семантикой «любой из», видимость строки фильтров `filters_visible`, `sort`,
    `compact_rows`) хранит `Session::country_filter` и применяет их к результатам без
    сетевых запросов (`ui::station_filter::filtered_results`); повторный запуск поиска при
    активном контексте (`Session::start_search`) пишет запрос именно в `country_filter.text`.
12. Избранное и история живут в `StationLibrary`, к которому обращается `Session`:
    `Session::toggle_favorite`, `Session::clear_history`, запись в историю при успешном
    `Session::start_station`. UI рисует звёзды и вкладки по срезу состояния.
13. Настройки сохраняет `SettingsStore` по команде ядра (`Session::set_theme`,
    `Session::set_accent`, `Session::set_language`, `Session::set_visualizer_enabled`,
    `Session::set_viz_bars`, `Session::set_motion`, `Session::set_motion_style`,
    `Session::set_reduce_motion`, `Session::set_allow_flips`, `Session::set_volume`);
    presentation-слой применяет тему, акцент и язык к egui при старте прогона и сразу
    после изменения (`palette::configure_style` — он же обнуляет `Style::animation_time`
    при `Reduce motion`, ADR-0032). Громкость из drawer'а идёт двумя действиями:
    `UiAction::PreviewVolume` на перетаскивании (только аудио, `Session::preview_volume`)
    и `UiAction::CommitVolume` при отпускании (`Session::commit_volume`, ADR-0031).
    Колесо над слайдером даёт оба действия в одном кадре: дискретный шаг не имеет
    незавершённого preview (ADR-0031 § 7).
14. Обслуживать Roulette: `Session::start_roulette` запрашивает случайную станцию с
    координатами, `Session::poll_roulette` запускает её, доворачивает камеру и отдаёт
    `Notice`; отдельной IPC-команды нет.

## Порядок инициализации

Текущий (Этап 8, фазы 1–6 + ADR-0018):

```
1. IpcServer::try_bind()          — ошибка AlreadyRunning → повторная отправка команды
2. AudioEngine::new()             — ошибка фатальна для запуска
3. GeoService::from_embedded()    — ошибка фатальна: без границ стран нет поиска
4. MediaControls::new(wake)       — ошибка логируется, приложение работает без MPRIS
5. RadioBrowserClient::new()      — без сетевых обращений
6. StationLibrary::load_default() — ошибка логируется, используется ephemeral()
7. SettingsStore::load_default()  — ошибка логируется, используется ephemeral()
8. фоновая задача refresh_servers — прогрев пула; ошибка только логируется
9. фоновая задача: audio events → wake()
10. IpcServer::run(wake, shutdown) — accept-loop
11. run_session(commands, wake, AppServices) — Session::new + цикл окна (ADR-0018):
    11.1. Session::restore_size()    — сохранённый размер (пользовательский пишется только
                                       в `Focus`) либо дефолт режима: 1100×800 /
                                       OVERLAY 500×620 / LISTENING 420×152 / PANEL 400×640
    11.2. Session::begin_window(size) — признак «окно открыто», сброс неисполненного намерения
    11.3. Session::settle_analysis() — включить `@level`/`@spectrum` под открытое окно
    11.4. run_window(session, size)  — eframe::run_native ПОВТОРНО из главного потока
        (кэш событийного цикла — thread_local, другой поток недопустим);
        NativeOptions.viewport собирается из restore_size():
        with_title(Session::window_mode().title()), with_app_id(APP_ID),
        with_inner_size(size), with_min_inner_size(Session::window_min_size())
        (минимум зависит от `WindowMode`: Focus — WINDOW_MIN_SIZE 720×480),
        with_transparent(true);
        renderer: Renderer::Wgpu
    11.5. OrbitApp::new(cc, &mut session) — заимствует Session на весь прогон
    11.6. Session::end_window()      — root-вьюпорт закрыт: wake.clear() +
                                       flush_pending_volume() (дописывает прерванный drag
                                       громкости, ADR-0031) + sync_media() + sync_tray()
    11.7. Session::is_quitting()     — выход по флагу, иначе …
    11.8. Session::take_reopen()     — смена режима: новый прогон без парковки,
                                      анализ и открытый drawer остаются
    11.9. Session::close_settings()  — drawer закрывается до парковки (ADR-0031)
    11.10. Session::settle_analysis() — снять анализ: окно уходит в `park`
    11.11. Session::park()           — простой без окна; ParkOutcome::Show → шаг 11.1,
                                      ParkOutcome::Quit → выход
12. OrbitApp::new                   — на каждый прогон: применяет тему, акцент и язык к
                                      egui (`palette::configure_style`, включая
                                      `Style::animation_time` под reduce motion), ставит
                                      системный цветной emoji-шрифт для флагов (ADR-0017,
                                      деградация без шрифта), создаёт GlobeRenderer из
                                      wgpu_render_state и кладёт его в callback_resources,
                                      строит VisualizerModel по `Settings::viz_bars`,
                                      `Toast` и `Transitions`, устанавливает WakeHandle
                                      (`wake.install` замыкания repaint)
13. после выхода — shutdown_tx + ожидание IPC-сервера с таймаутом 2 с; сокет удаляется
```

`GeoService` создаётся до `MediaControls`, чтобы ошибка датасета проявилась раньше
инициализации D-Bus. Если `wgpu_render_state` недоступен, приложение продолжает
работать без 3D-глобуса (`globe_ready = false` — признак тоже одного прогона).

## Реакция на команды

Команды исполняет `Session` (`src/session/commands.rs`); presentation-слой отвечает только
за то, как отразить оконные намерения.

| Команда       | Действие |
|---------------|----------|
| Toggle/Show/Hide | Через `WindowState` ядра и `UiIntent` для UI (ADR-0018). Скрытие: `Session::hide` (или `toggle` в состоянии «открыто») → `UiIntent::Close` → `logic()` шлёт `ViewportCommand::Close` → `run_native` возвращается → `Session::end_window` (окно реально уходит из раскладки, `wake.clear()`, `sync_media()`) → `settle_analysis` (снимает `@level`/`@spectrum`) → `Session::park`. Показать: `Session::show`/`toggle` в скрытом состоянии → `UiIntent::Show` → `park` выходит с `ParkOutcome::Show` → новый `run_window` с восстановленным размером; на niri 26.04 окно с полностью отрисованным деском появляется за ≈1.02 с от приёма команды (≈73 мс из них — сам IPC-запрос). `ViewportCommand::Visible`, `CancelClose` и сжатие поверхности до 1×1 не используются: на Wayland `Visible` — no-op (редакция 1 ADR отклонена live-проверкой) |
| Play          | `Session::resolve_station`: поиск в результатах последнего поиска, затем `radio.get_station` (таймаут 8 с); `audio.play(url_resolved)`, запись в историю, асинхронный `radio.send_click`, `sync_media` |
| Stop          | `Session::stop_playback` → `audio.stop()`; последний URL и станция сохраняются в `Session` для повторного запуска из MPRIS |
| Volume        | `Session::set_volume` → `audio.set_volume(level)` + сохранение в `Settings::volume` (дискретная команда, pending-preview сбрасывается, ADR-0031); витрина MPRIS обновляется `sync_media()` в том же кадре (`tick`) или сразу после команды в `park()` |
| Status        | `status <state>`, при активной станции `stationuuid`/`name`, затем `volume <0.00>` и `window visible\|hidden` |
| Quit          | `Session::quit`: сначала ставится флаг `quitting`, затем `UiIntent::Close`; `run_session` после возврата `run_native` завершает процесс — останавливаются аудио и IPC-сервер, удаляется сокет |
| Raw | `error неизвестная команда: <text>` — префикс `error` CLI считает неудачей |

`MediaCommand` разбирает тот же `Session` (`src/session/playback.rs`):

| Команда | Действие |
|---------|----------|
| Play | снять паузу; если поток остановлен — повторно запустить последний URL (`resume_playback`) |
| Pause | поставить на паузу |
| PlayPause | переключить в зависимости от `PlaybackState` (при `Reconnecting` — no-op) |
| Stop | `audio.stop()` |
| SetVolume | `Session::set_volume` → `audio.set_volume()` и сохранение `Settings::volume` (ADR-0031) |
| Next/Previous | шаг по текущему видимому списку станций (`play_relative(±1)`) с циклом (wrap); при пустом списке — no-op |

Все команды, кроме `show`/`toggle`/`quit`, работают и без окна: в припаркованном состоянии
их читает `Session::park` (`tokio::select!` по приёмникам IPC, MPRIS, трея, Roulette и
аудио-событий), там же после каждой исполненной команды вызываются `sync_media()` и
`sync_tray()`. Команды трея (`Toggle/Show/PlayPause/Stop/Previous/Next/Roulette/VolumeUp/
VolumeDown/SetTheme/Settings/Quit`) разбирает `Session::execute_tray_command`; `Settings`
идёт через `Session::open_settings` — при скрытом окне это `UiIntent::Show`, поэтому
показ и drawer появляются уже в новом прогоне (ADR-0031).
Закрытие очереди IPC-запросов
(`Parked::QueueClosed` — приватный признак ядра) трактуется как выход.

Аудио-вызовы выполняются синхронно через `tokio::runtime::Handle::block_on` — и из
`logic()` (через `Session::tick`), и из `park()`; сами операции короткие (ответ аудио-потока с
таймаутом 3 с). Поиск станций выполняется асинхронно: результат приходит в
`mpsc`-канал, `WakeHandle::waker()` будит GUI (в припаркованном состоянии пробуждать нечего —
событие читается напрямую из очереди).

Ответ клиенту формируется в `Session::execute` синхронно с применением команды и
отправляется через `IpcRequest::try_respond`. Команда вьюпорта (`Focus`/`Close`) отправляется
presentation-слоем позже — в `logic()` следующего кадра либо на выходе из `park()`, — а её
применение оконным менеджером асинхронно и в ответе не подтверждается. `window` в `status`
отражает признак `Session::window.visible` (окно открыто/закрыто нами), а не состояние WM:
окно, свёрнутое композитором, считается открытым.

## Кадр: `logic()` и `ui()`

`logic()` (один раз за кадр, перед отрисовкой):

1. `Session::tick()` — аудио-события, очередь `IpcRequest` с ответами, `MediaCommand`,
   `TrayCommand`, `poll_search`, `poll_roulette`, `sync_media`, `sync_tray`,
   синхронизация `spectrum_enabled` с движком.
2. `drain_notices` — `Session::take_notices()` → `Toast::info` / `Toast::error`; текст
   берётся из `i18n::catalog(effective_language())` по ключу `NoticeKey` (ADR-0030).
3. `poll_favicons` — `FaviconCache::begin_frame`/`poll`, затем `take_fetches` →
   `reqwest`-задачи в `Session::runtime`; пока кэш занят — repaint через 120 мс.
4. Размер окна: `inner_rect` из ввода → `Session::set_window_size` (геометрия нужна,
   чтобы следующий показ собрал `ViewportBuilder` до создания окна).
5. `Session::take_intent()` → `ViewportCommand::Focus` (`UiIntent::Show`) или
   `ViewportCommand::Close` (`UiIntent::Close`).
6. `Escape` снимает верхний модальный слой: сначала drawer настроек
   (`Session::close_settings`), затем карточку управления
   (`Session::set_help_visible(false)`, ошибка — тост `SettingNotSaved`).
   `Ctrl+,` (`Cmd+,`) — `Session::toggle_settings` (ADR-0031).
7. Repaint-политика: при включённом визуализаторе и `Playing`/`Buffering` — repaint через 33 мс
   (`SPECTRUM_REPAINT`); `draw_layers` добавляет кадры движения (33 мс при активном
   эфире, 66 мс для idle-дыхания; при `Off`/Reduce motion — без лишних кадров;
   идущий переворот камеры также продлевает repaint; пульс маркеров запрашивает
   repaint сам `globe_view`.

`ui()` вызывает `draw()`: `Frame::capture` снимает читаемое состояние в снапшот,
`egui::CentralPanel` с прозрачной рамкой рисует `draw_layers` (один вызов
`Transitions::frame` на кадр считает прогрессы зон по `ZoneFlags`; глобус → HUD → панель →
transport → help → легенда → капсулы, затем в `Order::Foreground` скрим и drawer настроек)
и `toast.draw`, затем `apply_actions` исполняет собранные
`UiAction`. `clear_color()` возвращает полностью прозрачный цвет — стекло зон рисует само.

При закрытом окне UI-цикла нет вовсе: ни `logic()`, ни `ui()` не вызываются, кадры не
аллоцируются, а события читает `Session::park`.

## GUI

Раскладка «Orbital Desk» (Этап 8, фазы 1–7): глобус занимает всё окно (full-bleed
со звёздным небом), поверх него рисуются плавающие стеклянные зоны — их геометрию
считает `DeskLayout::compute(viewport, panel_open, help_open, settings_open, mode)`
(`src/ui/layout.rs`): HUD сверху (58 px), панель станций справа (360 px),
transport снизу (84 px: контролы + громкость + 40 px визуализатора), help-карточка
216×128 слева сверху (по требованию, с нижним полем 8 px), легенда плотности слева снизу и drawer настроек
справа (380 px, клампится до ширины окна) поверх scrim на весь viewport. Клик и колесо
глобуса игнорируются внутри плавающих зон (`DeskLayout::blocks_point`). UI вынесен
в `src/ui/`; каждая зона через `ui.scope_builder(...).inner` возвращает собственный
`Vec<UiAction>`, `OrbitApp` склеивает их и применяет после отрисовки кадра. Все
надписи интерфейса берутся из `crate::i18n::catalog(language)` (ADR-0030), язык —
`Session::effective_language()`; документация и комментарии — на русском.
`UiAction` — внутренний тип, не публичный контракт.

Режим окна (ADR-0021) задаёт плотность одного набора виджетов: `Full` для Focus,
`Compact` для Overlay и `Minimal` для Listening. `Compact` собирает HUD-строку
сверху (название, подзаголовок «{статус воспроизведения} · {регион, страна}» и 16
полос визуализатора справа от них),
мини-глобус между зонами и капсулу избранного снизу (до 8 строк
`station_row`); `Minimal` — единственную строку now-playing с аватаром, пульсирующим
кольцом при Playing, heart/expand/close и play/pause (`src/ui/capsule.rs`).
Звёздное небо и country-режим рисуются только в `Full`; сглаженный `pulse`
считается один раз за кадр и уходит только в капсулу Listening. Возврат в Focus
из капсулы — кнопка expand (`UiAction::SetWindowMode(WindowMode::Focus)`), выбор
режима также доступен в секции `Window` drawer'а настроек; `UiAction::SetWindowMode`
в `Listening` закрывает drawer — панель в капсулу не помещается (ADR-0031).

Доступность: кастомные контролы получают focus-ring и `WidgetInfo` с ролью и
подписью (`Role::Button`/`RadioButton`/`CheckBox`/`Tab`, `ListItem` у строки,
`slider` у громкости); громкость принимает стрелки и Home/End с шагом
`VOLUME_KEY_STEP` = 0.05, а колесо мыши над слайдером — тот же шаг на щелчок и
сразу как завершённый ввод (ADR-0031 § 7). `SliderResponse { changed, committed }`
разделяет изменение
и завершение (отпускание drag, клик, клавиша или щелчок колеса): первое даёт `UiAction::PreviewVolume`,
второе — `UiAction::CommitVolume` (ADR-0031); строки списка
focusable, ↑/↓ переводят фокус на соседа по видимому списку, `scroll_to_me`
удерживает его в кадре, Space/Enter запускают станцию штатным
`FAKE_PRIMARY_CLICKED` egui.

- глобус: ЛКМ выбирает маркер станции или страну, ПКМ + перетаскивание вращает,
  колесо масштабирует; наведение на страну показывает контур, выбранная страна —
  акцентный контур с заливкой-heatmap: альфа заливки растёт логарифмически от
  количества загруженных станций страны; курсор `PointingHand` над страной и
  маркером, `Grabbing` при вращении;
- клик по стране запускает поиск станций `countrycode=<ISO alpha-2>` (до 100,
  по голосам, `hide_broken=true`) и мягко доворачивает камеру к центру страны,
  в панели показывается контекстная строка `Country: Germany`; в режиме жанра
  панель показывает `Genre: …`, а клик по стране игнорируется; наведение на
  страну рисует тултип с цветным флагом-эмодзи (regional indicators от ISO
  alpha-2, рендер через ADR-0017), именем и числом станций выдачи API;
- HUD «Страна» ищет страну по названию или ISO-коду: до 6 подсказок, Enter
  выбирает первую, клик — любую; выбор очищает запрос и запускает поиск станций;
- переключатель `Страна | Жанр` задаёт `SearchMode` (тип определён в
  `src/session/search.rs`, `src/ui/mod.rs` даёт только ре-экспорт); смена режима
  (`Session::switch_mode`) инвалидирует текущий поиск, очищает результаты, маркеры
  и локальные фильтры;
- в режиме «Жанр» popup выбора открывается **прямо кликом по пилюле `Genre`**
  (отдельной кнопки нет) и содержит чипы Rock/Pop/Jazz/Electronic/News/Metal/
  Classical/Synthwave и поле своего тега (Enter или «Искать»); выбор запускает
  глобальный tag-поиск (`limit=200`; маркеры — подмножество строк с координатами,
  ADR-0036, ревизия 2026-10-28 (второе));
- в активном контексте (страна или жанр) подсказка поля поиска в HUD —
  `Filter stations…`, в свободном режиме — `Search stations…`; Enter запускает
  поиск (в режиме «Страна» Enter выбирает первую подсказку страны);
- панель фильтрует результаты локально по имени/тегам/стране/штату и
  мультивыбору жанров (OR: достаточно одного тега) и сортирует по
  популярности, битрейту или имени; фильтры не делают сетевых запросов;
- каждый поиск — один запрос без `has_geo_info`: список страны полный, маркеры —
  его подмножество с координатами (ADR-0036, ревизия 2026-10-28 (второе));
  маркеры группируются по координатам с округлением до 2 знаков, дубликаты по
  UUID отбрасываются;
- список страны пагинируется: подпись показывает «загружено из всего»
  (мировой счётчик), кнопка «Загрузить ещё» дополняет список без дубликатов;
  ошибка догрузки не стирает уже показанные строки;
- все маркеры одного размера (3 px) и одного цвета `palette.marker`; качество
  (битрейт, кодек) и рейтинг (голоса) у группы берутся по максимуму и показываются
  в подписи при наведении; играющая станция получает кольцо с пульсом, выбранная —
  белое кольцо выбора; строка играющей или выбранной станции несёт accent-плашку;
- ответы поиска несут номер поколения; ответ прошлого режима/жанра отбрасывается,
  поэтому старые маркеры не возвращаются после смены контекста;
- маркеры рисуются egui-оверлеем после `paint_globe`; наведение в радиусе 10 px
  показывает подпись, клик запускает станцию, выбирает её и один раз прокручивает
  строку в центр списка (`UiAction::SelectStation` → `Session::select_station`,
  `reveal_station`, ADR-0036 § 3);
- HUD: рисуемый логотип (планета с орбитальным кольцом) и подпись `OrbitFM /
  Global Internet Radio`, pill-переключатель `Country | Genre`, поле поиска,
  справа — индикатор эфира `● LIVE` и больше ничего: станция и её favicon из HUD
  убраны (имя и `state, country` показывают transport и список станций), `HudState`
  не несёт `now_playing`/`favicons`, а ключ `hud.idle` удалён из каталога (ADR-0030,
  ADR-0015 ред. 2026-10-28); кнопка-луна
  (быстрый Dark/Light), шестерёнка (открывает drawer настроек: `Appearance`, `Language`,
  `Audio`, `Motion`, `Window`, `About`; он же на `Ctrl+,` и в трее — `Settings…`,
  закрывается крестиком, `Esc`, кликом по скриму и уходом окна в `park`), кнопка списка,
  `?` (карточка управления по
  требованию) и `×` (Hide: процесс и эфир остаются живыми, это не выход), прижатая
  к правому краю строки с зазором 4 px от группы управления; слайдер
  громкости живёт в transport, не в HUD; при узком окне логотип и подписи
  скрываются;
- панель: вкладки `Results / Favorites / History` с векторными иконками и
  accent-подчёркиванием активной; показываются, если есть результаты или
  сохранённые станции; новый поиск возвращает вкладку `Results`; локальные
  фильтры — поле `Search stations in …`, кнопки-иконки `Funnel` (показать/
  скрыть строку фильтров) и `FunnelX` (очистить), выпадающий мультивыбор
  жанров с бейджами выбранных и крестиком, сортировка
  `Sort by: Popularity/Bitrate/Name` и переключатель плотности `Compact`;
  строки списка — круглый favicon (или инициал), имя, `state, country`,
  теги-чипы (клик — глобальный поиск по тегу), справа битрейт/кодек и звезда
  с запасом до кромки; играющая строка выделена accent-рамкой с
  мини-эквалайзером; список прокручивается на всю высоту панели; под списком —
  «загружено из всего» и кнопка «Загрузить ещё»; сворачивание — кнопка списка
  в HUD или `×`;
- у каждой строки списка и у текущей станции в transport есть звезда: она добавляет
  станцию в избранное или убирает её оттуда; кнопки рисуются векторно и не зависят
  от глифов шрифта;
- успешный запуск станции записывается в начало истории; в истории доступна кнопка
  `Clear history` (строка каталога `panel.clear_history`); подтверждение и ошибки
  библиотеки показываются тостом;
- transport: слева — favicon 44 px, имя станции, `state, country` и третья строка
  `Now Playing: <первые теги>` (метаданных трека из потока нет — используются теги
  станции); группа контролов — по центру строки: `⏮ / play-pause / ⏭`
  (prev/next идут по текущему видимому списку с циклом), stop, heart (избранное) и
  кнопка `Radio Roulette` с кубиком; справа налево — столбцы визуализатора (в
  простое — «дыхание»), слайдер громкости и `CODEC kbps`, если остаток ширины
  позволяет; ширина правой группы = остаток после центрированных контролов, поэтому
  полоса визуализатора ужимается вместе с окном; без станции — `Nothing playing` /
  `No station yet`; чип `Visualizer` из строки убран, включение полосы живёт в
  настройках; LIVE-бейдж и мини-эквалайзер тоже убраны: эфир показан чипом в HUD и
  карточкой станции на глобусе, а в строке они только отнимали место у контролов;
- фавиконки станций загружаются асинхронно (ADR-0016): `FaviconCache` в UI хранит
  состояния, `OrbitApp` спавнит `reqwest`-запросы (до 8 одновременно, кэш до 256)
  в `Session::runtime`, декод `image` приводит к 64×64, текстура поднимается в
  GUI-потоке; сбой кэшируется, вместо фавикона — круг с инициалом; после закрытия
  окна кэш теряется вместе с текстурами и при следующем показе загружается заново;
- `Radio Roulette` запрашивает случайную станцию с координатами (до трёх попыток),
  запускает её, перелетает камерой к точке за 900 мс и показывает тост с названием
  и страной; ошибка показывается тостом и не прерывает текущий эфир; кнопка
  заблокирована, пока запрос выполняется;
- MPRIS: artist — «штат, страна» (или одно из них), иначе теги; artUrl — favicon,
  title и URL станции без изменений;
- тост показывает до трёх сообщений по 3 секунды стопкой внизу по центру: новый слой у
  якоря, предыдущие поднимаются, дубликат обновляет существующую запись, а не добавляет
  слой (ADR-0032); help-карточка управления
  (правый клик — орбита, ЛКМ — страна/станция, колесо — zoom) включена по умолчанию:
  `Settings::help_visible` (default `true`) инициализирует `Session::show_help`;
  `?` в HUD и чекбокс в секции `Window` drawer'а переключают и сохраняют выбор,
  `Esc` скрывает карточку и сохраняет `false`; первый выбор страны или запуск
  станции её больше не гасят (ADR-0036 § 5); легенда карты стран слева снизу —
  `ComboBox` режима по умолчанию «Без карты» (`CountryMetric::Off`), градиент
  Low→High и подпись источника заливки (ADR-0035);
- состояние поиска (`Searching…`), ошибки каталога и `No stations found`
  отображаются в панели; `On map: N stations` показывает число пригодных станций;
- при отсутствии `wgpu_render_state` вместо глобуса показывается `3D globe unavailable`;
- чип `Visualizer` включает метрику спектра mpv: без неё столбцы переходят в
  idle-«дыхание», ошибка фильтра показывается тостом и не прерывает
  воспроизведение; состояние чипа — сохранённая настройка `Settings::visualizer_enabled`
  (ADR-0031). Число столбцов (16/24/32) выбирается `dropdown`'ом в секции `Appearance`
  drawer'а настроек (`UiAction::SetVizBars` → `Settings::viz_bars`, модель перестраивается
  на месте); капсула Overlay показывает ровно 16 полос и от `viz_bars` не зависит —
  срез считается bucket-max по уровням модели (`src/ui/capsule.rs`), а при
  `visualizer_enabled == false` полосы не аллоцируются вовсе, чтобы строка не прыгала;
- секция `Motion` drawer'а настроек: реактивность Off/Low/Normal/High и стиль
  движения — `dropdown`'ы, `Reduce motion` и `Allow flips` — `checkbox`
  (`UiAction::SetMotionStyle` →
  `Session::set_motion_style`, `UiAction::SetAllowFlips` →
  `Session::set_allow_flips`); переключатель переворотов недоступен при
  `Off`/Reduce motion; ошибка сохранения показывается тостом;
- сглаженный сигнал эфира (`0.75·level + 0.25·flux`) передаётся в кольцо
  Listening: оно пульсирует при Playing и затухает при Stop и выключенном
  визуализаторе; глобус живёт по ADR-0028 (squash, рябь, стиль и `flash` от
  onset), фонового аудио-пульса Fresnel нет;
- в Focus за планетой рисуется орбитальное аудиокольцо (ADR-0029): те же шесть
  агрегатов питают корону из 120 столбцов, аврору, комету и искры на onset;
  кольцо появляется при живом эфире и включённом визуализаторе, гаснет при
  Stop/Pause и уходе из Focus, в Overlay/Listening не рисуется, hit-testing
  не меняет; дефолтная дистанция камеры 3.2 оставляет для него место;
- при отсутствии `wgpu_render_state` ввод глобуса игнорируется, вместо сферы
  рисуется заглушка;
- при выборе или наведении страны над глобусом рисуется country callout (ТЗ §1):
  флаг, название, «{N} stations», leader line к якорю и пин; в режиме «Жанр»
  карточка не появляется;
- при активном воспроизведении статус содержит UUID и имя станции.

## Зависимости

`OrbitApp<'a>` владеет только кадрами и per-run ресурсами: `session: &'a mut Session`,
`globe_ready`, `spectrum_overlay: SpectrumOverlay`, `motion: MotionMixer`,
`orbit_ring: OrbitRing`,
`callout: CalloutState`, `visualizer: VisualizerModel`, `favicons: FaviconCache`,
`http: reqwest::Client`, `toast: Toast`, `transitions: Transitions`,
`drawer_rect: Option<egui::Rect>`. Подсистемами (`AudioEngine`, `GeoService`, `RadioBrowserClient`,
`MediaControls`, `StationLibrary`, `SettingsStore`, `GlobeCamera`, `tokio::runtime::Handle`)
владеет `Session`; к ним presentation-слой обращается через поля ядра
(`session.audio`, `session.geo`, `session.library`, `session.settings`, `session.runtime`,
`session.panel_open`, `session.show_help`, `session.settings_open`)
и через методы ядра — `Session::start_station`, `focus_country`, `start_country_search`,
`start_genre_search`, `start_search`, `switch_mode`, `start_roulette`, `toggle_favorite`,
`clear_history`, `set_theme`, `set_accent`, `set_language`, `effective_language`,
`set_visualizer_enabled`, `set_viz_bars`,
`set_motion`, `set_motion_style`, `set_allow_flips`, `set_reduce_motion`,
`set_volume`, `preview_volume`, `commit_volume`, `play_relative`, `toggle_playback`,
`stop_playback`, `hide`, `quit`, `open_settings`, `close_settings`, `toggle_settings`,
`window_mode`/`set_window_mode`, `motion_reactivity`, `tick`, `take_intent`, `take_notices`,
`set_help_visible`, `select_station`, `take_station_reveal`.

Из `src/ui/` используются `UiAction`, `DeskLayout`, `LayoutDensity`,
`Palette`/`configure_style`, `GlobeState`/`GlobeResponse`, `OrbitRing`,
`TransportState`,
`PanelState`, `HudState`, `FaviconCache` и `decode_favicon`, `Toast`, капсулы
`capsule::{overlay_top, overlay_favorites, listening}`, `widgets::glass_panel`,
`widgets::volume_slider_width` (с колесом над слайдером) и
`SliderResponse`, примитивы строк настроек `controls::{dropdown, checkbox, row}`,
`settings_panel::{show, SettingsState}`, `animation` (`Transitions`, `ZoneFlags`,
`FrameMotion`, `alpha`, `slide_x`, `rise_y`, `stagger`, `Presence`),
`overlays::help_card`/`country_legend`, `ui::country_map::CountryHeatCache`,
`SearchMode` (ре-экспорт
`crate::session::search`), `playback_label`/`country_label`/`single_line`/`truncate`.
Подписи зон приходят из `crate::i18n` (`catalog`, `theme_label`, `accent_label`,
`language_label`, `motion_label`, `style_label`, `fill`) — ADR-0030. Из `src/render/` —
`GlobeRenderer`, `MotionMixer`,
`SpectrumOverlay`, `SpectrumTarget`, `VisualizerModel`; из `src/settings/` —
`Settings`, `MotionReactivity`, `MotionStyle`.

Подмена реализаций в тестах не требуется; модульные тесты
проверяют чистые функции (`format_status`, `playback_label`, `country_label`,
`truncate`, `filtered_results`, `flag_emoji`, `DeskLayout`),
строки карточек (`row_name`, `row_place`), геометрию иконок и логику состояний
`FaviconCache`. Клавиатурная навигация списка проверяется headless-кадрами egui:
↑/↓ переводят фокус на соседнюю строку, Space запускает станцию
(`src/ui/station_row.rs`).
