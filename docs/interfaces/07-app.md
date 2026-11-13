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
   простое: скрытый эфир не тратит CPU на `@level`/`@spectrum` (ADR-0027). Окно
   настроек закрывается на пути к парковке; `take_reopen` (смена режима) уходит на новый
   прогон раньше, поэтому оно переживает только её (ADR-0031).
   Маршрутизацией `IpcCommand` и `MediaCommand` занимается `Session`: очередь
   `mpsc::Receiver<IpcRequest>` и приёмник MPRIS принадлежат ядру и обслуживаются как из
   `Session::tick()` (окно открыто), так и из `Session::park()` (окно закрыто).
4. Держать ресурсы одного прогона: тему и акцент (`palette::configure_style`), встроенные
   fallback-шрифты (`ui::fonts::install`: Latin/Cyrillic, CJK, Arabic, emoji; системный
   `NotoColorEmoji` остаётся предпочтительным, ADR-0041, ADR-0017), `GlobeRenderer`,
   вставленный в `callback_resources` wgpu-рендерера, `FaviconCache` с его текстурами
   и процедурными обложками (`src/ui/procedural.rs`, ADR-0044), `ArtworkFetcher` для иконок
   (`services::artwork`, ADR-0016 ред. 2),
   `SpectrumOverlay`, `motion: MotionMixer`, `orbit_ring: OrbitRing`, `CalloutState`,
   `station_callout: CalloutState`, `fan: FanState` (веер станций кластера, ADR-0065),
   `VisualizerModel`, `Toast`, `transitions: Transitions`
   с прогрессами переходов зон (ADR-0032), `settings_category`,
   `integrations: IntegrationPanel` (снимок и очередь операций, ADR-0042) и
   `onboarding: onboarding::View` (шаг и отклики мастера, ADR-0041).
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
   - при попадании в кластер экранных маркеров (радиус 10 px) `globe_view`
     действует по составу: кластер из ≥ 2 станций в Focus закрепляет веер
     (ADR-0065 § 3), выбор идёт кликом по чипу или строке карточки «+N»;
     в капсулах клик выбирает представителя. Одиночный маркер возвращает
     `UiAction::SelectStation`, и `apply_action` вызывает
     `Session::select_station` (выделение и прокрутка строки) и затем
     `Session::start_station` (ADR-0036);
   - иначе `globe_view` возвращает точку на сфере, полученную от
     `GlobeCamera::screen_to_sphere_deformed` с текущей `GlobeMotion.deform`
     (`GlobeResponse::clicked_point`);
   - `src/app.rs` спрашивает страну у `GeoService::country_at_sphere_point` и создаёт
     `UiAction::SelectCountry`;
   - точка без валидной страны (океан, территория без ISO) создаёт
     `UiAction::ClearCountrySelection`, и `apply_action` вызывает
     `Session::clear_country_selection`: гаснут подсветка, контекст, локальный фильтр,
     список и маркеры, незавершённый запрос отбрасывается; выделенная станция
     сохраняется (ADR-0036, ревизия 2026-10-31);
   - решение принимает ядро: `Session::focus_country` мягко доворачивает камеру к центру
     страны (`country_shape().focus_point()`, 650 мс), `Session::start_country_search`
     запрашивает станции через `RadioBrowserClient` с фильтром `countrycode`;
   - UI отображает новое состояние `Session` в следующем кадре (заголовок страны,
     результаты, пустое состояние).

   В режиме «Глобальный жанр» клик по стране не меняет контекст (no-op): проверка страны
   выключена (`country_enabled = false`); маркеры станций остаются кликабельными.
10. Контекстный поиск (страна или жанр) делает **два** запроса (ADR-0065 § 1):
    список без `has_geo_info` (полный, пагинируемый) и отдельный слой карты
    `has_geo_info=true`, `hidebroken=true`, `offset = 0`,
    `limit = MAP_MARKER_LIMIT = 800` и `order` из `settings.catalog_order`.
    Ответ слоя карты наполняет `Session::markers` независимо от пагинации
    списка; поиск по имени без контекста строит маркеры из загруженных строк
    (`session::markers::build_markers`). Поэтому
    `<Session::markers> ⊆ Session::search.results` не гарантируется: станция
    слоя карты может ещё не быть в загруженной странице — выбор и эфир работают,
    `reveal_station` не расходуется (ADR-0065 § 1). Список страны пагинируется:
    первая страница — 100 строк, подпись панели показывает «загружено из всего»
    по мировому счётчику (`Session::country_total`), кнопка «Показать ещё»
    вызывает `UiAction::LoadMore` → `Session::load_more` (offset, дедупликация по
    UUID), пока `search.has_more`; ошибка догрузки не очищает список, а отдаёт
    тост `LoadMoreFailed`. Лимиты: название и страна — 100, жанр — 200. Ошибка
    первого поиска очищает и список, и маркеры. Presentation-слой только рисует
    `Session::markers` и передаёт в панель `total_available`, `loading_more`,
    `has_more`.
11. Для выбранной страны или жанра локальные фильтры (`text`, мультивыбор `genres`
    с семантикой «любой из», видимость строки фильтров `filters_visible`, `sort`,
    `compact_rows`) хранит `Session::country_filter` и применяет их к результатам без
    сетевых запросов (`ui::station_filter::filtered_results`); повторный запуск поиска при
    активном контексте (`Session::start_search`) пишет запрос именно в `country_filter.text`
    и переключает вкладку на `Results`. Клик по маркеру (`Session::select_station`)
    сбрасывает `country_filter`, чтобы строка выбранной станции не осталась скрытой.
12. Избранное и история живут в `StationLibrary`, к которому обращается `Session`:
    `Session::toggle_favorite`, `Session::clear_history`, запись в историю при успешном
    `Session::start_station`. UI рисует звёзды и вкладки по срезу состояния.
13. Настройки сохраняет `SettingsStore` по команде ядра (`Session::set_theme`,
    `Session::set_accent`, `Session::set_language`, `Session::set_visualizer_enabled`,
    `Session::set_viz_bars`, `Session::set_motion`, `Session::set_motion_style`,
    `Session::set_reduce_motion`, `Session::set_allow_flips`, `Session::set_volume`);
    presentation-слой применяет тему, акцент и язык к egui при старте прогона и сразу
    после изменения (`palette::configure_style` — он же обнуляет `Style::animation_time`
    при `Reduce motion`, ADR-0032). Громкость из окна настроек идёт двумя действиями:
    `UiAction::PreviewVolume` на перетаскивании (только аудио, `Session::preview_volume`)
    и `UiAction::CommitVolume` при отпускании (`Session::commit_volume`, ADR-0031).
    Колесо над слайдером даёт оба действия в одном кадре: дискретный шаг не имеет
    незавершённого preview (ADR-0031 § 8).
14. Обслуживать Roulette: `Session::start_roulette` запрашивает случайную станцию с
    координатами, `Session::poll_roulette` запускает её, доворачивает камеру и отдаёт
    `Notice`; отдельной IPC-команды нет.
15. Показывать мастер первого запуска (ADR-0041): `Session::onboarding_open()` держит
    модальный слой поверх стола, пока пользователь не нажмёт «Пропустить» или «Готово»;
    пока он открыт, `Escape` и горячие клавиши стола не срабатывают. Шаги «Язык и тема»,
    «Ваш рабочий стол» и «Интеграции» дают `UiAction::{SetLanguage, SetTheme,
    ApplyWindowRules, OpenIntegrations, FinishOnboarding}`. `ApplyWindowRules` вызывает
    `Session::apply_window_rules`: Hyprland — `hyprctl keyword` с фиксированными
    аргументами, niri — готовый KDL-сниппет с кнопкой «Копировать», KDE/GNOME/иное — без
    действий; конфиги пользователя не редактируются. `FinishOnboarding` и
    `OpenIntegrations` завершают мастер (`Session::finish_onboarding`), второй ещё и
    открывает категорию `Integrations`. Арабский язык зеркалит раскладку
    (`Layout::right_to_left`).
16. Управлять интеграциями из окна настроек (ADR-0042): вкладка `Integrations` показывает
    группы Bars / Environments / Windows / Browsers (ADR-0051) и состояния `NotInstalled` /
    `Installed` / `UpdateAvailable` / `Problem`; действия `UiAction::IntegrationInstall/Uninstall/
    EnableWaybar/Reload` уходят в `IntegrationPanel` и исполняются по одной фоновой
    операции за раз. Правка конфигов — только JSONC CST (`jsonc-parser`) с сохранением
    комментариев и форматирования, одноразовый backup `<имя>.orbitfm.bak`, повторный
    разбор перед атомарной записью; симлинки отклоняются. Из чужих конфигов оболочек
    установка правит ровно одну строку: `include` цели `niri` с таким же одноразовым
    backup `config.kdl.orbitfm.bak`; остальные цели раскладывают только свои файлы,
    а `noctalia` не пишет ничего и регистрирует источник оболочки командой
    (ADR-0051). Перезапуск — `waybar-msg cmd reload`, targeted `SIGUSR2` или
    `polybar-msg cmd restart`; `killall`/`pkill` не используются. Операции требуют
    явного нажатия в GUI, установка native-host берёт абсолютный путь к бинарнику.
17. Показывать офлайн-данные и экономный режим (ADR-0043): при `search.offline` панель
    помечает выдачу как локальную, при `session.world.offline()` легенда карты подписывает
    источник снимком; `Session::economy_stream()` включает индикатор `ECO` рядом с
    битрейтом. Кнопка «Очистить офлайн-снимок» в категории `About` даёт
    `UiAction::ClearOfflineSnapshot` → `Session::clear_offline_snapshot`; успех и ошибка
    приходят тостами `SnapshotCleared`/`SnapshotClearFailed`.

## Порядок инициализации

Текущий (Этап 8, фазы 1–6 + ADR-0018):

```
1. IpcServer::try_bind()          — ошибка AlreadyRunning → повторная отправка команды
2. AudioEngine::new()             — ошибка фатальна для запуска
3. GeoService::from_embedded()    — ошибка фатальна: без границ стран нет поиска
4. MediaControls::new(wake)       — ошибка логируется, приложение работает без MPRIS
5. RadioBrowserClient::new()      — без сетевых обращений
6. StationLibrary::load_default() — ошибка чтения логируется, используется ephemeral();
                                       повреждённый JSON уезжает в карантин (ADR-0062)
7. SettingsStore::load_default()  — то же; повреждённые настройки не включают мастер
                                       первого запуска
8. фоновая задача refresh_servers — прогрев пула; ошибка только логируется
9. фоновая задача: audio events → wake()
10. IpcServer::run(wake, shutdown) — accept-loop
11. run_session(commands, wake, AppServices) — Session::new + цикл окна (ADR-0018):
    11.1. Session::restore_size()    — сохранённый размер (пользовательский пишется только
                                       в `Focus`) либо дефолт режима: 1100×800 /
                                       OVERLAY 500×620 / LISTENING 420×152 / PANEL 400×640
    11.2. Session::begin_window(size) — признак «окно открыто», сброс неисполненного намерения
    11.3. Session::settle_analysis() — включить `@level`/`@spectrum` под открытое окно
    11.4. run_window(session, size, surface) — eframe::run_native ПОВТОРНО из главного
        потока (кэш событийного цикла — thread_local, другой поток недопустим);
        NativeOptions.viewport собирается из `RootSurface`:
        with_title(surface.title()), with_app_id(APP_ID),
        with_inner_size(size), with_min_inner_size(surface.min_size())
        (минимум зависит от поверхности: Focus — WINDOW_MIN_SIZE 720×480,
        Settings — размер варианта, Warning — 360×280),
        with_transparent(surface.is_transparent()), with_decorations(!borderless);
        renderer: Renderer::Wgpu
    11.5. OrbitApp::new(cc, &mut session, surface) — заимствует Session на весь прогон
    11.6. Session::end_window()      — root-вьюпорт закрыт: wake.clear() +
                                       flush_pending_volume() (дописывает прерванный drag
                                       громкости, ADR-0031) + sync_media() + sync_tray()
    11.7. Session::is_quitting()     — выход по флагу, иначе …
    11.8. Session::take_reopen()     — смена режима: новый прогон без парковки,
                                      анализ и открытое окно настроек остаются
    11.9. Session::close_settings()  — окно настроек закрывается до парковки (ADR-0031)
    11.10. Session::settle_analysis() — снять анализ: окно уходит в `park`
    11.11. Session::park()           — простой без окна; ParkOutcome::Show → шаг 11.1,
                                      ParkOutcome::Settings → окно настроек,
                                      ParkOutcome::Warn → окно предупреждения,
                                      ParkOutcome::Quit → выход (ADR-0053, ADR-0058)
12. OrbitApp::new                   — на каждый прогон: применяет тему, акцент и язык к
                                      egui (`palette::configure_style`, включая
                                      `Style::animation_time` под reduce motion), ставит
                                      встроенные fallback-шрифты (`ui::fonts::install`,
                                      ADR-0041) и системный цветной emoji-шрифт для флагов
                                      (ADR-0017, деградация без шрифта), создаёт
                                      GlobeRenderer из wgpu_render_state и кладёт его в
                                      callback_resources, строит VisualizerModel по
                                      `Settings::viz_bars`, `Toast`, `Transitions`,
                                      `IntegrationPanel` и `onboarding::View`,
                                      устанавливает WakeHandle (`wake.install` замыкания
                                      repaint)
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
| Play          | `Session::begin_play`: локальный поиск в результатах и офлайн-снимке (`local_station`) — ответ в том же кадре; иначе `PendingPlay` и `radio.get_station` в tokio-задаче (таймаут 8 с), ответ после `start_station` (ADR-0063); `audio.play(url_resolved)`, запись в историю, асинхронный `radio.send_click`, `sync_media` |
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
| Next/Previous | шаг по текущему видимому списку станций (`play_relative(±1)`); с `queue_wrap` края замыкаются, без него шаг за край — no-op; при пустом списке — no-op |

Все команды, кроме `show`/`toggle`/`quit`, работают и без окна: в припаркованном состоянии
их читает `Session::park` (`tokio::select!` по приёмникам IPC, MPRIS, трея, Roulette и
аудио-событий), там же после каждой исполненной команды вызываются `sync_media()` и
`sync_tray()`. Команды трея (`Toggle/Show/PlayPause/Stop/Previous/Next/Roulette/VolumeUp/
VolumeDown/SetTheme/Settings/Quit`) разбирает `Session::execute_tray_command`; `Settings`
идёт через `Session::open_settings` — при скрытом окне это `UiIntent::Show`, поэтому
показ и окно настроек появляются уже в новом прогоне (ADR-0031).
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

1. `Session::tick()` — аудио-события, `poll_economy`, очередь `IpcRequest` с ответами,
   `MediaCommand`, `TrayCommand`, `poll_search`, `poll_companion_search`,
   `poll_stream_import`, `poll_roulette`, `poll_world`/`start_world`, `sync_media`,
   `sync_tray`, синхронизация `spectrum_enabled` с движком, `publish_status`.
2. `drain_notices` — `Session::take_notices()` → `Toast::info` / `Toast::error`; текст
   берётся из `i18n::catalog(effective_language())` по ключу `NoticeKey` (ADR-0030).
3. `poll_favicons` — `FaviconCache::begin_frame`/`poll`, затем `take_fetches` →
   задачи `ArtworkFetcher::fetch` в `Session::runtime` (сеть, DNS и перепроверка
   хоппов — внутри `services::artwork`, ADR-0025 ред. 4); пока кэш занят — repaint
   через 120 мс.
4. `poll_integrations` — готовый результат фоновой операции `IntegrationPanel`:
   тост об успехе/ручном перезапуске/ошибке, снимок интеграций обновляется сам
   (ADR-0042).
5. Размер окна: `inner_rect` из ввода → `Session::set_window_size` (геометрия нужна,
   чтобы следующий показ собрал `ViewportBuilder` до создания окна).
6. `Session::take_intent()` → `ViewportCommand::Focus` (`UiIntent::Show`) или
   `ViewportCommand::Close` (`UiIntent::Close`); на macOS `Hide` превращается в
   `Visible(false)` + `mark_hidden`, `Show` — в `Visible(true)` + `mark_shown`
   (ADR-0026).
7. Модальные слои: пока `Session::onboarding_open()`, `Escape`, `Ctrl+,`, `/`,
   `Ctrl+F` и Space не срабатывают — мастер держит фокус до «Пропустить» или
   «Готово» (ADR-0041). Иначе `Escape` снимает верхний слой: сначала окно настроек
   (`Session::close_settings`), затем карточку управления
   (`Session::set_help_visible(false)`, ошибка — тост `SettingNotSaved`);
   `Ctrl+,` (`Cmd+,`) — `Session::toggle_settings` (ADR-0031).
8. Repaint-политика: при включённом визуализаторе и `Playing`/`Buffering` — repaint через 33 мс
   (`SPECTRUM_REPAINT`); `draw_layers` добавляет кадры движения (33 мс при активном
   эфире, 66 мс для idle-дыхания; при `Off`/Reduce motion — без лишних кадров;
   идущий переворот камеры также продлевает repaint; пульс маркеров запрашивает
   repaint сам `globe_view`.

`ui()` вызывает `draw()`: `Frame::capture` снимает читаемое состояние в снапшот,
`egui::CentralPanel` с прозрачной рамкой рисует `draw_layers` (один вызов
`Transitions::frame` на кадр считает прогрессы зон по `ZoneFlags`; глобус → HUD → панель →
transport → help → легенда → капсулы, затем в `Order::Foreground` скрим, окно настроек
и мастер первого запуска) и `toast.draw`, затем `apply_actions` исполняет собранные
`UiAction`. `clear_color()` возвращает полностью прозрачный цвет — стекло зон рисует само.

При закрытом окне UI-цикла нет вовсе: ни `logic()`, ни `ui()` не вызываются, кадры не
аллоцируются, а события читает `Session::park`.

## GUI

Раскладка «Orbital Desk» (Этап 8, фазы 1–7): глобус занимает всё окно (full-bleed
со звёздным небом), поверх него рисуются плавающие стеклянные зоны — их геометрию
считает `DeskLayout::compute(viewport, panel_open, help_open, mode)`
(`src/ui/layout.rs`): HUD сверху (58 px), панель станций справа (360 px),
transport снизу (84 px: контролы + громкость + 40 px визуализатора), help-карточка
216×128 слева сверху (по требованию, с нижним полем 8 px) и легенда плотности слева
снизу. Окно настроек больше не модалка стола: это отдельная root-поверхность
(`RootSurface::Settings`, ADR-0053), у которой нет ни scrim, ни `settings_rect`;
стол под ней не рисуется. Мастер первого запуска остаётся модалкой 560×468 поверх
стола (ADR-0041), сцена под ней остаётся на месте. Клик и колесо глобуса
игнорируются внутри плавающих зон (`DeskLayout::blocks_point`). Окно предупреждения
таймера сна — тоже отдельная root-поверхность (`RootSurface::Warning`, ADR-0058):
капсула без рамки, кольцо отсчёта и кнопка «Отменить». Оба дочерних вьюпорта
(настройки и предупреждение) собираются билдерами `settings_viewport_builder`
и `warning_viewport_builder` в `src/app.rs` и несут не только заголовок, но и
`with_app_id(APP_ID)`: Wayland не наследует app-id от родителя, а правила
WM матчат пару «класс + заголовок» (ADR-0051, ADR-0053). UI вынесен
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
режима также доступен в категории `Window` окна настроек; `UiAction::SetWindowMode`
в `Listening` закрывает окно настроек — в капсуле нет его зоны (ADR-0031).

Доступность: кастомные контролы получают focus-ring и `WidgetInfo` с ролью и
подписью (`Role::Button`/`RadioButton`/`CheckBox`/`Tab`, `ListItem` у строки,
`slider` у громкости); громкость принимает стрелки и Home/End с шагом
`VOLUME_KEY_STEP` = 0.05, а колесо мыши над слайдером — тот же шаг на щелчок и
сразу как завершённый ввод (ADR-0031 § 8). `SliderResponse { changed, committed }`
разделяет изменение
и завершение (отпускание drag, клик, клавиша или щелчок колеса): первое даёт `UiAction::PreviewVolume`,
второе — `UiAction::CommitVolume` (ADR-0031); строки списка
focusable, ↑/↓ переводят фокус на соседа по видимому списку, `scroll_to_me`
удерживает его в кадре, Space/Enter запускают станцию штатным
`FAKE_PRIMARY_CLICKED` egui.

- глобус: ЛКМ выбирает маркер станции, чип веера или страну; наведение на
  кластер ≥ 2 станций дольше 200 мс раскрывает веер, клик по кластеру
  закрепляет его (ADR-0065 § 3). ПКМ + перетаскивание вращает, колесо
  масштабирует; наведение на страну показывает контур, выбранная страна —
  акцентный контур с заливкой-heatmap: альфа заливки растёт логарифмически от
  количества загруженных станций страны; курсор `PointingHand` над страной,
  маркером и чипом, `Grabbing` при вращении;
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
  глобальный tag-поиск списка (`limit=200`) и отдельный слой карты
  (`has_geo_info=true`, `limit=800`, ADR-0065/0066);
- в активном контексте (страна или жанр) подсказка поля поиска в HUD —
  `Filter stations…`, в свободном режиме — `Search stations…`; Enter запускает
  поиск (в режиме «Страна» Enter выбирает первую подсказку страны);
- панель фильтрует результаты локально по имени/тегам/стране/штату и
  мультивыбору жанров (OR: достаточно одного тега) и сортирует по
  популярности, битрейту или имени; фильтры не делают сетевых запросов;
- контекстный поиск (страна/жанр) делает два запроса: список без
  `has_geo_info` (полный, пагинируемый) и слой карты с `has_geo_info=true`
  (`MAP_MARKER_LIMIT = 800`, ADR-0065 § 1); поиск по имени без контекста строит
  маркеры из загруженных строк. Маркеры группируются по координатам с
  округлением до 2 знаков, дубликаты по UUID отбрасываются, затем presentation
  склеивает близкие на экране маркеры в кластеры;
- список страны пагинируется: подпись показывает «загружено из всего»
  (мировой счётчик), кнопка «Загрузить ещё» дополняет список без дубликатов;
  ошибка догрузки не стирает уже показанные строки;
- одиночный маркер — точка 3 px цвета `palette.marker`; с 2–4 станциями
  добавляется тонкое кольцо, с 5+ — кольцо и счётчик; маркеры ближе 16 px
  экрана склеиваются в один кластер-чип (ADR-0065 § 2). Качество (битрейт, кодек)
  и рейтинг (голоса) у группы берутся по максимуму и показываются в подписи и
  веере; играющая станция получает пульс, выбранная — кольцо выбора; строка
  играющей или выбранной станции несёт accent-плашку;
- ответы поиска несут номер поколения; ответ прошлого режима/жанра отбрасывается,
  поэтому старые маркеры не возвращаются после смены контекста;
- маркеры рисуются egui-оверлеем после `paint_globe`; наведение в радиусе 10 px
  показывает подпись, клик по одиночному маркеру запускает станцию, выбирает её
  и один раз прокручивает строку в центр списка; клик по кластеру ≥ 2 станций
  закрепляет веер, а выбор идёт по чипу, строке карточки «+N» или клавиатурой
  (`UiAction::SelectStation` → `Session::select_station`, `reveal_station`,
  ADR-0036 § 3, ADR-0065 § 3);
- HUD: рисуемый логотип (планета с орбитальным кольцом) и подпись `OrbitFM /
  Global Internet Radio`, pill-переключатель `Country | Genre`, поле поиска,
  справа — индикатор эфира `● LIVE` и больше ничего: станция и её favicon из HUD
  убраны (имя и `state, country` показывают transport и список станций), `HudState`
  не несёт `now_playing`/`favicons`, а ключ `hud.idle` удалён из каталога (ADR-0030,
  ADR-0015 ред. 2026-10-28); кнопка-луна
  (быстрый Dark/Light), шестерёнка (открывает окно настроек: `Appearance`, `Language`,
  `Audio`, `Motion`, `Window`, `Integrations`, `About`; оно же на `Ctrl+,` и в трее —
  `Settings…`, закрывается крестиком, `Esc`, кликом по скриму и уходом окна в `park`),
  кнопка списка, `?` (карточка управления по
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
  (prev/next идут по текущему видимому списку; края замыкаются при `queue_wrap`), stop, heart (избранное) и
  кнопка `Radio Roulette` с кубиком; справа налево — столбцы визуализатора (в
  простое — «дыхание»), слайдер громкости и `CODEC kbps`, если остаток ширины
  позволяет; ширина правой группы = остаток после центрированных контролов, поэтому
  полоса визуализатора ужимается вместе с окном; без станции — `Nothing playing` /
  `No station yet`; чип `Visualizer` из строки убран, включение полосы живёт в
  настройках; LIVE-бейдж и мини-эквалайзер тоже убраны: эфир показан чипом в HUD и
  карточкой станции на глобусе, а в строке они только отнимали место у контролов;
- фавиконки станций загружаются асинхронно (ADR-0016, ред. 2): `FaviconCache` в UI хранит
  состояния, `OrbitApp` спавнит `ArtworkFetcher::fetch`-задачи (до 8 одновременно, кэш до 256)
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
  `?` в HUD и тумблер в категории `Window` окна настроек переключают и сохраняют выбор,
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
  (ADR-0031). Число столбцов (16/24/32) выбирается `dropdown`'ом в категории `Appearance`
  окна настроек (`UiAction::SetVizBars` → `Settings::viz_bars`, модель перестраивается
  на месте); капсула Overlay показывает ровно 16 полос и от `viz_bars` не зависит —
  срез считается bucket-max по уровням модели (`src/ui/capsule.rs`), а при
  `visualizer_enabled == false` полосы не аллоцируются вовсе, чтобы строка не прыгала;
- категория `Motion & accessibility` окна настроек: реактивность Off/Low/Normal/High и
  стиль движения — `dropdown`'ы, `Reduce motion` и `Allow flips` — `switch_row`
  (`UiAction::SetMotionStyle` →
  `Session::set_motion_style`, `UiAction::SetAllowFlips` →
  `Session::set_allow_flips`); переключатель переворотов недоступен при
  `Off`/Reduce motion; ошибка сохранения показывается тостом;
- категория `Integrations` (ADR-0042): четыре группы — Bars (`Waybar`, `Polybar`,
  `i3blocks`, `swaybar`, `i3status-rust`, `XFCE`, `Cinnamon`, `MATE`, `LXQt`),
  Environments (`KDE Plasma 6`, `GNOME`, `Omarchy`, `Noctalia`), Windows (`niri`,
  `Hyprland`) и Browsers
  (`Chrome / Chromium` — Chrome, Chromium, Brave, Edge, Vivaldi — и `Firefox`);
  состояние каждой цели `NotInstalled`/`Installed`/`UpdateAvailable`/`Problem`
  («Требует внимания»), кнопки «Установить»/«Удалить», для Waybar — «Включить в
  конфиг», для установленного — «Перезапустить». У установленных целей `noctalia`,
  `niri` и `hyprland` строка дополнительно показывает оставшийся ручной шаг —
  строку `source` для Hyprland, добавленную строку `include` для niri и
  регистрацию источника с виджетом для Noctalia (`handoff_note`,
  ADR-0051). Снимок обновляется при открытии
  страницы, операции идут по одной в фоне; успех, необходимость ручного
  перезапуска и ошибки приходят тостами. Правки Waybar — только JSONC CST с
  сохранением комментариев, одноразовым `<имя>.orbitfm.bak` и повторным разбором
  перед записью; native-host получает абсолютный путь к бинарнику; `killall`/
  `pkill` не используются;
- категория `About` содержит кнопку «Очистить офлайн-снимок»: она удаляет
  локальный кэш станций и до конца сеанса запрещает его перезапись, живой каталог
  и текущий эфир не затрагиваются (ADR-0043);
- мастер первого запуска (ADR-0041) показывается только при отсутствующем файле
  настроек: шаг «Язык и тема» применяет выбор сразу, шаг «Ваш рабочий стол»
  показывает окружение и предлагает runtime-правила (Hyprland) или готовый
  KDL-сниппет с кнопкой «Копировать» (niri), шаг «Интеграции» ведёт в категорию
  `Integrations`. Арабский язык зеркалит раскладку; `Escape` и горячие клавиши
  стола под мастером не работают;
- офлайн-выдача и экономный режим видны в интерфейсе (ADR-0043): панель помечает
  список как локальный (`search.offline`), легенда карты — источник заливки
  снимком (`world.offline()`), рядом с битрейтом текущей станции появляется `ECO`;
  вместо серых квадратов обложек рисуется детерминированная процедурная обложка
  из UUID и палитры станции (ADR-0044), без новых сетевых запросов;
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
`surface: RootSurface` (стол, настройки или предупреждение — ADR-0053, ADR-0058),
`globe_ready`, `spectrum_overlay: SpectrumOverlay`, `motion: MotionMixer`,
`orbit_ring: OrbitRing`,
`callout: CalloutState`, `station_callout: CalloutState`, `fan: FanState`,
`visualizer: VisualizerModel`, `favicons: FaviconCache`, `artwork: ArtworkFetcher`,
`toast: Toast`,
`transitions: Transitions`, `settings_category`,
`integrations: IntegrationPanel`, `onboarding: onboarding::View`,
`country_metric: CountryMetric`, `country_heat: CountryHeatCache`. Подсистемами
(`AudioEngine`, `GeoService`, `RadioBrowserClient`,
`MediaControls`, `StationLibrary`, `SettingsStore`, `GlobeCamera`, `tokio::runtime::Handle`)
владеет `Session`; к ним presentation-слой обращается через поля ядра
(`session.audio`, `session.geo`, `session.library`, `session.settings`, `session.runtime`,
`session.panel_open`, `session.show_help`, `session.settings_open`,
`session.search.offline`, `session.world.offline()`)
и через методы ядра — `Session::start_station`, `focus_country`, `start_country_search`,
`start_genre_search`, `start_search`, `switch_mode`, `start_roulette`, `toggle_favorite`,
`clear_history`, `set_theme`, `set_accent`, `set_language`, `effective_language`,
`set_visualizer_enabled`, `set_viz_bars`,
`set_motion`, `set_motion_style`, `set_allow_flips`, `set_reduce_motion`,
`set_volume`, `preview_volume`, `commit_volume`, `play_relative`, `toggle_playback`,
`stop_playback`, `hide`, `quit`, `open_settings`, `close_settings`, `toggle_settings`,
`window_mode`/`set_window_mode`, `motion_reactivity`, `tick`, `take_intent`, `take_notices`,
`set_help_visible`, `select_station`, `take_station_reveal`, `load_more`, `country_total`,
`retry_search`, `onboarding_open`, `onboarding_desktop`, `finish_onboarding`,
`apply_window_rules`, `clear_offline_snapshot`, `economy_stream`,
`set_artwork_allow_http`.

Из `src/ui/` используются `UiAction`, `DeskLayout`, `LayoutDensity`,
`Palette`/`configure_style`, `GlobeState`/`GlobeResponse`, `OrbitRing`,
`TransportState`,
`PanelState`, `HudState`, `FaviconCache` и `decode_favicon`, `Toast`, капсулы
`capsule::{overlay_top, overlay_favorites, listening}`, `widgets::glass_panel`,
`widgets::volume_slider_width` (с колесом над слайдером) и
`SliderResponse`, примитивы строк настроек `controls::{dropdown, switch_row,
accent_swatch, row}`,
`settings_panel::{show, SettingsState, SettingsCategory}`, `onboarding`
(`show`, `View`, `State`, `RuleState`), `fonts::install`, `animation` (`Transitions`, `ZoneFlags`,
`FrameMotion`, `alpha`, `slide_x`, `rise_y`, `stagger`, `Presence`),
`overlays::help_card`/`country_legend`, `ui::country_map::CountryHeatCache`,
`SearchMode` (ре-экспорт
`crate::session::search`), `playback_label`/`country_label`/`single_line`/`truncate`.
Подписи зон приходят из `crate::i18n` (`catalog`, `theme_label`, `accent_label`,
`language_label`, `motion_label`, `style_label`, `fill`) — ADR-0030. Из `src/render/` —
`GlobeRenderer`, `MotionMixer`,
`SpectrumOverlay`, `SpectrumTarget`, `VisualizerModel`; из `src/settings/` —
`Settings`, `MotionReactivity`, `MotionStyle`; из `src/integration/` —
`IntegrationPanel`, `PanelAction`, `PanelEvent`, `PanelOutcome`, `Target`,
`TargetGroup` и `status` (ADR-0042).

Подмена реализаций в тестах не требуется; модульные тесты
проверяют чистые функции (`format_status`, `playback_label`, `country_label`,
`truncate`, `filtered_results`, `flag_emoji`, `DeskLayout`),
строки карточек (`row_name`, `row_place`), геометрию иконок и логику состояний
`FaviconCache`. Клавиатурная навигация списка проверяется headless-кадрами egui:
↑/↓ переводят фокус на соседнюю строку, Space запускает станцию
(`src/ui/station_row.rs`). Мастер первого запуска покрыт тестами
`step_index_never_leaves_the_wizard`, `arabic_switches_the_composition_direction`
и `wizard_text_stacks_vertically_in_both_directions` (`src/ui/onboarding.rs`);
встроенные шрифты — `embedded_fonts_stay_within_the_agreed_budget` и
`every_embedded_file_is_a_real_font` (`src/ui/fonts.rs`, бюджет 6 MiB);
процедурные обложки — детерминизмом UUID, геометрией и палитрой
(`same_uuid_gives_the_same_cover`, `uuid_drives_geometry`,
`genre_votes_and_bitrate_repaint_the_palette`, `src/ui/procedural.rs`);
интеграции — CST-вставкой с сохранением комментариев и форматирования,
идемпотентностью и единственным backup, отказом битым конфигам, симлинкам и
отсутствующему `modules-right`, поиском конфига без расширения
(`src/integration/waybar.rs`), правилами обнаружения окружения, фиксированными
аргументами Hyprland и идемпотентной строкой `include` niri
(`src/integration/wm.rs`), фиксированным argv моста Noctalia
(`src/integration/noctalia.rs`), выбором ручного перезапуска для
неподдерживаемых целей (`src/integration/reload.rs`).
