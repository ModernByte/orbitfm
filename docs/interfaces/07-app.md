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
3. Вести цикл показа и скрытия окна: `restore_size` → `begin_window` → `run_window` →
   `end_window` → `is_quitting` → `park` (§«Порядок инициализации», шаги 11–12).
   Маршрутизацией `IpcCommand` и `MediaCommand` занимается `Session`: очередь
   `mpsc::Receiver<IpcRequest>` и приёмник MPRIS принадлежат ядру и обслуживаются как из
   `Session::tick()` (окно открыто), так и из `Session::park()` (окно закрыто).
4. Держать ресурсы одного прогона: тему и акцент (`palette::configure_style`), системный
   цветной emoji-шрифт (ADR-0017), `GlobeRenderer`, вставленный в `callback_resources`
   wgpu-рендерера, `FaviconCache` с его текстурами, `reqwest::Client` для иконок,
   `SpectrumOverlay`, `CalloutState`, `VisualizerModel`, `Toast`.
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
   как именно показать `Notice`, решает UI, ядро про тосты ничего не знает.
9. При клике по глобусу в режиме «Страна»:
   - при попадании в маркер станции (радиус 10 px) `globe_view` возвращает
     `UiAction::Play`, и `apply_action` вызывает `Session::start_station` (запускается
     представитель группы);
   - иначе `globe_view` возвращает точку на сфере, полученную от
     `GlobeCamera::screen_to_sphere` (`GlobeResponse::clicked_point`);
   - `src/app.rs` спрашивает страну у `GeoService::country_at_sphere_point` и создаёт
     `UiAction::SelectCountry`;
   - решение принимает ядро: `Session::focus_country` мягко доворачивает камеру к центру
     страны (`country_shape().focus_point()`, 650 мс), `Session::start_country_search`
     запрашивает станции через `RadioBrowserClient` с фильтром `countrycode`;
   - UI отображает новое состояние `Session` в следующем кадре (заголовок страны,
     результаты, пустое состояние).

   В режиме «Глобальный жанр» клик по стране не меняет контекст (no-op): проверка страны
   выключена (`country_enabled = false`); маркеры станций остаются кликабельными.
10. Для каждого поиска ядро параллельно запрашивает гео-станции (`has_geo_info = true`)
    и строит маркеры (`session::markers::build_markers`); лимиты: название/страна — 100 на
    список и 100 на маркеры, жанр — 200 на список и 200 на маркеры; ошибка гео-запроса не
    влияет на список. Presentation-слой только рисует `Session::markers`.
11. Для выбранной страны или жанра локальные фильтры (`text`, `genre`, `sort`,
    `compact_rows`) хранит `Session::country_filter` и применяет их к результатам без
    сетевых запросов (`ui::station_filter::filtered_results`); повторный запуск поиска при
    активном контексте (`Session::start_search`) пишет запрос именно в `country_filter.text`.
12. Избранное и история живут в `StationLibrary`, к которому обращается `Session`:
    `Session::toggle_favorite`, `Session::clear_history`, запись в историю при успешном
    `Session::start_station`. UI рисует звёзды и вкладки по срезу состояния.
13. Тему и акцент сохраняет `SettingsStore` по команде ядра (`Session::set_theme`,
    `Session::set_accent`); presentation-слой применяет их к egui при старте прогона и
    сразу после изменения (`palette::configure_style`).
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
    11.1. Session::restore_size()    — последний известный размер или WINDOW_SIZE (920×660)
    11.2. Session::begin_window(size) — признак «окно открыто», сброс неисполненного намерения
    11.3. run_window(session, size)  — eframe::run_native ПОВТОРНО из главного потока
        (кэш событийного цикла — thread_local, другой поток недопустим);
        NativeOptions.viewport собирается из restore_size():
        with_title(WINDOW_TITLE), with_app_id(APP_ID), with_inner_size(size),
        with_min_inner_size(WINDOW_MIN_SIZE = 720×480), with_transparent(true);
        renderer: Renderer::Wgpu
    11.4. OrbitApp::new(cc, &mut session) — заимствует Session на весь прогон
    11.5. Session::end_window()      — root-вьюпорт закрыт: wake.clear() + sync_media()
    11.6. Session::is_quitting()     — выход по флагу, иначе …
    11.7. Session::park()            — простой без окна; ParkOutcome::Show → шаг 11.1,
                                      ParkOutcome::Quit → выход
12. OrbitApp::new                   — на каждый прогон: применяет тему и акцент к egui
                                      (`palette::configure_style`), ставит системный цветной
                                      emoji-шрифт для флагов (ADR-0017, деградация без
                                      шрифта), создаёт GlobeRenderer из wgpu_render_state и
                                      кладёт его в callback_resources, устанавливает
                                      WakeHandle (`wake.install` замыкания repaint)
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
| Toggle/Show/Hide | Через `WindowState` ядра и `UiIntent` для UI (ADR-0018). Скрытие: `Session::hide` (или `toggle` в состоянии «открыто») → `UiIntent::Close` → `logic()` шлёт `ViewportCommand::Close` → `run_native` возвращается → `Session::end_window` (окно реально уходит из раскладки, `wake.clear()`, `sync_media()`) → `Session::park`. Показать: `Session::show`/`toggle` в скрытом состоянии → `UiIntent::Show` → `park` выходит с `ParkOutcome::Show` → новый `run_window` с восстановленным размером; на niri 26.04 окно с полностью отрисованным деском появляется за ≈1.02 с от приёма команды (≈73 мс из них — сам IPC-запрос). `ViewportCommand::Visible`, `CancelClose` и сжатие поверхности до 1×1 не используются: на Wayland `Visible` — no-op (редакция 1 ADR отклонена live-проверкой) |
| Play          | `Session::resolve_station`: поиск в результатах последнего поиска, затем `radio.get_station` (таймаут 8 с); `audio.play(url_resolved)`, запись в историю, асинхронный `radio.send_click`, `sync_media` |
| Stop          | `Session::stop_playback` → `audio.stop()`; последний URL и станция сохраняются в `Session` для повторного запуска из MPRIS |
| Volume        | `Session::set_volume` → `audio.set_volume(level)`; витрина MPRIS обновляется `sync_media()` в том же кадре (`tick`) или сразу после команды в `park()` |
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
| SetVolume | `audio.set_volume()` |
| Next/Previous | шаг по текущему видимому списку станций (`play_relative(±1)`) с циклом (wrap); при пустом списке — no-op |

Все команды, кроме `show`/`toggle`/`quit`, работают и без окна: в припаркованном состоянии
их читает `Session::park` (`tokio::select!` по приёмникам IPC, MPRIS и аудио-событий), там же
после каждой исполненной команды вызывается `sync_media()`. Закрытие очереди IPC-запросов
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
   `poll_search`, `poll_roulette`, `sync_media`, синхронизация `spectrum_enabled` с движком.
2. `drain_notices` — `Session::take_notices()` → `Toast::info` / `Toast::error`.
3. `poll_favicons` — `FaviconCache::begin_frame`/`poll`, затем `take_fetches` →
   `reqwest`-задачи в `Session::runtime`; пока кэш занят — repaint через 120 мс.
4. Размер окна: `inner_rect` из ввода → `Session::set_window_size` (геометрия нужна,
   чтобы следующий показ собрал `ViewportBuilder` до создания окна).
5. `Session::take_intent()` → `ViewportCommand::Focus` (`UiIntent::Show`) или
   `ViewportCommand::Close` (`UiIntent::Close`).
6. `Escape` закрывает карточку управления (`session.show_help = false` + `mark_help_seen`).
7. Repaint-политика: при включённом FFT и `Playing`/`Buffering` — repaint через 33 мс
   (`SPECTRUM_REPAINT`); пульс маркеров запрашивает repaint сам `globe_view`.

`ui()` вызывает `draw()`: `Frame::capture` снимает читаемое состояние в снапшот,
`egui::CentralPanel` с прозрачной рамкой рисует `draw_layers` (глобус → HUD → панель →
transport → help → легенда) и `toast.draw`, затем `apply_actions` исполняет собранные
`UiAction`. `clear_color()` возвращает полностью прозрачный цвет — стекло зон рисует само.

При закрытом окне UI-цикла нет вовсе: ни `logic()`, ни `ui()` не вызываются, кадры не
аллоцируются, а события читает `Session::park`.

## GUI

Раскладка «Orbital Desk» (Этап 8, фазы 1–7): глобус занимает всё окно (full-bleed
со звёздным небом), поверх него рисуются плавающие стеклянные зоны — их геометрию
считает `DeskLayout::compute(viewport, panel_open, help_open, mode)`
(`src/ui/layout.rs`): HUD сверху (58 px), панель станций справа (312 px),
transport снизу (84 px: контролы + громкость + 40 px визуализатора), help-карточка
слева сверху (по требованию) и легенда плотности слева снизу. Клик и колесо
глобуса игнорируются внутри плавающих зон (`DeskLayout::blocks_point`). UI вынесен
в `src/ui/`; каждая зона через `ui.scope_builder(...).inner` возвращает собственный
`Vec<UiAction>`, `OrbitApp` склеивает их и применяет после отрисовки кадра. Все
надписи интерфейса — на английском (решение фазы 6), документация и комментарии —
на русском. `UiAction` — внутренний тип, не публичный контракт.

Режим окна (ADR-0021) задаёт плотность одного набора виджетов: `Full` для Focus,
`Compact` для Overlay и `Minimal` для Listening. `Compact` собирает HUD-строку
сверху, мини-глобус между зонами и капсулу избранного снизу (до 8 строк
`station_row`); `Minimal` — единственную строку now-playing с аватаром, пульсирующим
кольцом при Playing, heart/expand/close и play/pause (`src/ui/capsule.rs`).
Звёздное небо и country-режим рисуются только в `Full`; сглаженный `pulse`
считается один раз за кадр и уходит только в капсулу Listening. Возврат в Focus
из капсулы — кнопка expand (`UiAction::SetWindowMode(WindowMode::Focus)`), выбор
режима также доступен в разделе «Window» попапа шестерёнки HUD.

Доступность: кастомные контролы получают focus-ring и `WidgetInfo` с ролью и
подписью (`Role::Button`/`RadioButton`/`CheckBox`/`Tab`, `ListItem` у строки,
`slider` у громкости); громкость принимает стрелки и Home/End; строки списка
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
- в режиме «Жанр» кнопка выбора открывает popup с чипами Rock/Pop/Jazz/Electronic/
  News/Metal/Classical/Synthwave и полем своего тега (Enter или «Искать»);
  выбор запускает глобальный tag-поиск (`limit=200`, маркеры `limit=200`);
- в активном контексте (страна или жанр) подсказка поля поиска в HUD —
  `Filter stations…`, в свободном режиме — `Search stations…`; Enter запускает
  поиск (в режиме «Страна» Enter выбирает первую подсказку страны);
- панель фильтрует результаты локально по имени/тегам/стране/штату,
  жанровым чипам All/Rock/Pop/Jazz/Electronic/News/Metal и сортирует по
  популярности, битрейту или имени; фильтры не делают сетевых запросов;
- каждый поиск дополнительно запрашивает станции с координатами
  (`has_geo_info=true`) для маркеров: 100 для названия/страны, 200 для жанра;
  маркеры группируются по координатам с округлением до 2 знаков, дубликаты по
  UUID отбрасываются;
- все маркеры одного размера (3 px) и одного цвета `palette.marker`; качество
  (битрейт, кодек) и рейтинг (голоса) у группы берутся по максимуму и показываются
  в подписи при наведении; играющая станция получает кольцо с пульсом;
- ответы поиска несут номер поколения; ответ прошлого режима/жанра отбрасывается,
  поэтому старые маркеры не возвращаются после смены контекста;
- маркеры рисуются egui-оверлеем после `paint_globe`; наведение в радиусе 10 px
  показывает подпись, клик запускает станцию, текущая станция подсвечена;
- HUD: рисуемый логотип (планета с орбитальным кольцом) и подпись `OrbitFM /
  Global Internet Radio`, pill-переключатель `Country | Genre`, поле поиска,
  справа — now-playing с favicon-аватаром 38 px и LIVE-бейджем (имя станции и
  `state, country`, при отсутствии станции — метка воспроизведения), кнопка-луна
  (быстрый Dark/Light), шестерёнка (тема, акцент, число столбцов
  визуализатора, `Hide window`, `Quit`), кнопка списка, `?` (карточка управления по
  требованию) и `×` (Hide: процесс и эфир остаются живыми, это не выход); слайдер
  громкости живёт в transport, не в HUD; при узком окне логотип и подписи
  скрываются;
- панель: вкладки `Results / Favorites / History` с векторными иконками и
  accent-подчёркиванием активной; показываются, если есть результаты или
  сохранённые станции; новый поиск возвращает вкладку `Results`; локальные
  фильтры — поле `Search stations in …`, чипы `All/Rock/Pop/Jazz/Electronic/
  News/Metal`, сортировка `Sort by: Popularity/Bitrate/Name` и переключатель
  плотности `Compact`; строки списка — круглый favicon (или инициал), имя,
  `state, country`, теги-чипы (клик — глобальный поиск по тегу), справа
  битрейт/кодек и звезда; играющая строка выделена accent-рамкой с
  мини-эквалайзером; список прокручивается на всю высоту панели;
  сворачивание — кнопка списка в HUD или `×`;
- у каждой строки списка и у текущей станции в transport есть звезда: она добавляет
  станцию в избранное или убирает её оттуда; кнопки рисуются векторно и не зависят
  от глифов шрифта;
- успешный запуск станции записывается в начало истории; в истории доступна кнопка
  «Очистить историю»; подтверждение и ошибки библиотеки показываются тостом;
- transport: слева — favicon 44 px, имя станции, `state, country` и третья строка
  `Now Playing: <первые теги>` (метаданных трека из потока нет — используются теги
  станции); в центре — LIVE-бейдж с мини-эквалайзером, `⏮ / play-pause / ⏭`
  (prev/next идут по текущему видимому списку с циклом), stop, heart (избранное) и
  кнопка `Radio Roulette` с кубиком; справа налево — `CODEC kbps`, слайдер
  громкости, чип `Visualizer` и столбцы визуализатора (в простое —
  «дыхание»); без станции — `Nothing playing` / `No station yet`;
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
- тост показывает одно сообщение 3 секунды внизу по центру; help-карточка управления
  (правый клик — орбита, ЛКМ — страна/станция, колесо — zoom) не постоянна: она
  открывается сама только при первом запуске (`Settings::help_seen == false`) и далее
  по кнопке `?` в HUD или клавише `?`, закрывается `Esc`, повторной `?` и гаснет после
  первого выбора страны или запуска станции; легенда `Station density` слева снизу —
  градиент Low→High;
- состояние поиска (`Searching…`), ошибки каталога и `No stations found`
  отображаются в панели; `On map: N stations` показывает число пригодных станций;
- при отсутствии `wgpu_render_state` вместо глобуса показывается `3D globe unavailable`;
- чип `Visualizer` включает метрику спектра mpv: без неё столбцы переходят в
  idle-«дыхание», ошибка фильтра показывается тостом и не прерывает
  воспроизведение; число столбцов (16/24/32) выбирается в попапе шестерёнки
  (`UiAction::SetVizBars` → `Settings::viz_bars`, модель перестраивается на месте);
- сглаженный сигнал эфира (`0.75·level + 0.25·flux`) передаётся в кольцо
  Listening: оно пульсирует при Playing и затухает при Stop и выключенном
  визуализаторе; глобус от эфира не зависит, Fresnel-обод статичен;
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
`globe_ready`, `spectrum_overlay: SpectrumOverlay`, `orbit_ring: OrbitRing`,
`callout: CalloutState`, `visualizer: VisualizerModel`, `favicons: FaviconCache`,
`http: reqwest::Client`, `toast: Toast`. Подсистемами (`AudioEngine`, `GeoService`, `RadioBrowserClient`,
`MediaControls`, `StationLibrary`, `SettingsStore`, `GlobeCamera`, `tokio::runtime::Handle`)
владеет `Session`; к ним presentation-слой обращается через поля ядра
(`session.audio`, `session.geo`, `session.library`, `session.settings`, `session.runtime`)
и через методы ядра — `Session::start_station`, `focus_country`, `start_country_search`,
`start_genre_search`, `start_search`, `switch_mode`, `start_roulette`, `toggle_favorite`,
`clear_history`, `set_theme`, `set_accent`, `set_spectrum_enabled`, `set_viz_bars`,
`set_volume`, `play_relative`, `toggle_playback`, `stop_playback`, `hide`, `quit`,
`window_mode`/`set_window_mode`, `tick`, `take_intent`, `take_notices`,
`panel_open`/`show_help`.

Из `src/ui/` используются `UiAction`, `DeskLayout`, `LayoutDensity`,
`Palette`/`configure_style`, `GlobeState`/`GlobeResponse`, `OrbitRing`,
`TransportState`,
`PanelState`, `HudState`, `FaviconCache` и `decode_favicon`, `Toast`, капсулы
`capsule::{overlay_top, overlay_favorites, listening}`, `widgets::glass_panel`,
`overlays::help_card`/`density_legend`, `SearchMode` (ре-экспорт
`crate::session::search`). Из `src/render/` — `GlobeRenderer`, `SpectrumOverlay`,
`SpectrumTarget`, `VisualizerModel`; из `src/settings/` — `Settings`.

Подмена реализаций в тестах не требуется; модульные тесты
проверяют чистые функции (`format_status`, `playback_label`, `country_label`,
`truncate`, `filtered_results`, `flag_emoji`, `DeskLayout`),
строки карточек (`row_name`, `row_place`), геометрию иконок и логику состояний
`FaviconCache`. Клавиатурная навигация списка проверяется headless-кадрами egui:
↑/↓ переводят фокус на соседнюю строку, Space запускает станцию
(`src/ui/station_row.rs`).
