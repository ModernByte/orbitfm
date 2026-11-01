# Интерфейс: Application Core (`Session`)

Application Core — слой, который переживает закрытие окна (ADR-0018). На Wayland
скрыть окно можно только закрыв root-вьюпорт, поэтому `eframe::run_native`
перезапускается из главного потока, а `OrbitApp` живёт ровно один прогон; всё
долгоживущее состояние принадлежит `Session`, которым владеет `run_session`
(`src/main.rs`).

Публичного трейта наружу нет: Core — точка сборки, а не расширяемая библиотека
(так же, как до ADR-0018 было у `OrbitApp`, `docs/interfaces/07-app.md`). Все
элементы пакета имеют видимость `pub(crate)` — контракт действует внутри crate,
между `src/session/`, presentation-слоем (`src/app.rs`, `src/ui/*`), модулем
трея (`src/tray/`, `docs/interfaces/11-tray.md`) и каталогом строк (`src/i18n/`,
`docs/interfaces/12-i18n.md`).

## 1. Контракт типа

| Элемент | Сигнатура | Поведение |
|---------|-----------|-----------|
| `Session::new` | `pub(crate) fn new(commands: mpsc::Receiver<IpcRequest>, wake: WakeHandle, services: AppServices) -> Self` | Собирает ядро из подсистем; подписывается на `audio.subscribe()`, восстанавливает громкость из настроек, берёт `visualizer_enabled` и `show_help` из них же, разворачивает `language = stored.language.resolve()`, вычисляет `onboarding_open = !settings.existed_at_load() && !stored.onboarding_done`, определяет `onboarding_desktop`, подставляет офлайн-снимок (`load_snapshot`), запускает `start_world`, помечает окно `visible = true`, `settings_open = false` и публикует первый `status` кадр |
| `tick` | `pub(crate) fn tick(&mut self)` | Один кадр фонового обслуживания; вызывается UI-циклом при открытом окне |
| `take_intent` | `pub(crate) fn take_intent(&mut self) -> Option<UiIntent>` | Забирает и снимает запрос к окну |
| `take_notices` | `pub(crate) fn take_notices(&mut self) -> VecDeque<Notice>` | Забирает всю очередь сообщений (очередь очищается) |
| `notify_info` | `pub(crate) fn notify_info(&mut self, key: NoticeKey)` | Ставит `Notice::Info(key)` в конец очереди |
| `notify_error` | `pub(crate) fn notify_error(&mut self, key: NoticeKey)` | Ставит `Notice::Error { key, detail: None }` в конец очереди |
| `notify_error_detail` | `pub(crate) fn notify_error_detail(&mut self, key: NoticeKey, detail: impl Into<String>)` | Ошибка с технической подробностью: `detail` не переводится (ADR-0030) |
| `effective_language` | `pub(crate) fn effective_language(&self) -> Language` | Язык, на котором UI рисует строки; `System` здесь не встречается — он развёрнут при старте и при смене настройки |
| `open_settings` | `pub(crate) fn open_settings(&mut self)` | Открывает окно настроек: в `Listening` сначала переводит окно в `Overlay`, при закрытом окне ставит `UiIntent::Show` (ADR-0031) |
| `close_settings` | `pub(crate) fn close_settings(&mut self)` | Закрывает окно настроек (`settings_open = false`) |
| `toggle_settings` | `pub(crate) fn toggle_settings(&mut self)` | Открыто → `close_settings`, закрыто → `open_settings` |
| `onboarding_open` | `pub(crate) fn onboarding_open(&self) -> bool` | Показывать ли мастер первого запуска: вычислен в `Session::new`, гасится `finish_onboarding` (ADR-0041) |
| `onboarding_desktop` | `pub(crate) fn onboarding_desktop(&self) -> Desktop` | Определённое на старте окружение для шага «Ваш рабочий стол» (`integration::wm::Desktop`) |
| `finish_onboarding` | `pub(crate) fn finish_onboarding(&mut self) -> Result<(), String>` | Завершение или пропуск мастера: `onboarding_open = false` и атомарная запись `Settings::onboarding_done`; ошибка записи возвращается вызывающему |
| `apply_window_rules` | `pub(crate) fn apply_window_rules(&self) -> Result<(), String>` | Runtime-правила окна на время сеанса: Hyprland — `hyprctl keyword windowrulev2` с фиксированными аргументами; niri и остальные — ошибка (сниппет показывается, конфиги не правятся) |
| `begin_window` | `pub(crate) fn begin_window(&mut self, size: [f32; 2])` | Отмечает окно открытым, сбрасывает `intent` и `reopen`, запоминает размер (только в `Focus`) |
| `end_window` | `pub(crate) fn end_window(&mut self)` | Отмечает окно закрытым, снимает `intent`, `wake.clear()`, `flush_pending_volume()`, `sync_media()`, `sync_tray()` |
| `set_window_size` | `pub(crate) fn set_window_size(&mut self, size: [f32; 2])` | Сохраняет текущий размер окна; в капсульных режимах значение отбрасывается (ADR-0021 п. 4) |
| `restore_size` | `pub(crate) fn restore_size(&self) -> [f32; 2]` | В `Focus` — сохранённый размер либо `WINDOW_SIZE`; в любом капсульном режиме (`Overlay`, `Listening`, `Panel`) — всегда пресет режима |
| `window_min_size` | `pub(crate) fn window_min_size(&self) -> [f32; 2]` | Нижний лимит для `ViewportBuilder` текущего режима |
| `window_mode` | `pub(crate) fn window_mode(&self) -> WindowMode` | Текущий режим окна (ADR-0021) |
| `set_window_mode` | `pub(crate) fn set_window_mode(&mut self, mode: WindowMode)` | Меняет режим и гарантирует показ: тот же режим → `Show`; новый режим при открытом окне → `Close` + `reopen`, при закрытом → `Show` |
| `take_reopen` | `pub(crate) fn take_reopen(&mut self) -> bool` | Забирает единичный признак «пересоздать окно с геометрией нового режима» |
| `is_reopening` | `pub(crate) fn is_reopening(&self) -> bool` | Идёт ли пересоздание окна из-за смены режима (macOS: не превращать `close_requested` в Hide) |
| `window_visible` | `pub(crate) fn window_visible(&self) -> bool` | Видимо ли окно пользователю: на macOS скрытое окно живёт в run loop (ADR-0026) |
| `mark_hidden` | `pub(crate) fn mark_hidden(&mut self)` | macOS: окно скрыто — закрыть настройки, зафиксировать `pending_volume`, обновить анализ, MPRIS и трей без ожидания кадра |
| `mark_shown` | `pub(crate) fn mark_shown(&mut self)` | macOS: окно снова видимо — вернуть анализ и обновить витрины |
| `hide` | `pub(crate) fn hide(&mut self)` | `UiIntent::Hide`, только если окно открыто и намерение ещё не стоит; иначе ничего. На Linux/Windows презентация переводит `Hide` в `ViewportCommand::Close`, на macOS — в `Visible(false)` |
| `show` | `pub(crate) fn show(&mut self)` | Ставит `UiIntent::Show` |
| `toggle` | `pub(crate) fn toggle(&mut self)` | Открыто → `Hide`/`Close`, закрыто → `Show` |
| `toggle_dropdown` | `pub(crate) fn toggle_dropdown(&mut self, mode: WindowMode)` | Переключатель-выпадающий: открытая поверхность того же режима скрывается, иначе `set_window_mode(mode)` (ADR-0033; пути `panel` и `overlay`) |
| `quit` | `pub(crate) fn quit(&mut self)` | `quitting = true` и `UiIntent::Close` |
| `is_quitting` | `pub(crate) fn is_quitting(&self) -> bool` | Признак явного выхода |
| `status_text` | `pub(crate) fn status_text(&self) -> String` | Строка ответа IPC-команды `status` |
| `status_json` | `pub(crate) fn status_json(&self) -> String` | Ответ IPC-команды `status --json`: одна строка JSON схемы 1 (ADR-0037) |
| `publish_status` | `pub(crate) fn publish_status(&self)` | Публикует полный кадр `status_json` в `StatusFeed` подписчикам `events` (ADR-0039); повторы гасит `send_if_modified` |
| `attach_tray` | `pub(crate) fn attach_tray(&mut self)` | macOS: создать иконку и меню на главном потоке; ошибка — `warn!` и работа без трея. На других платформах — no-op |
| `park` | `pub(crate) fn park(&mut self) -> ParkOutcome` | Простой без окна: ждёт IPC/MPRIS/трей/аудио/рулетку и исполняет их без UI |
| `execute` | `pub(crate) fn execute(&mut self, command: &IpcCommand) -> String` | Исполняет IPC-команду и возвращает строку ответа; общее для окна и парковки |
| `process_commands` | `pub(crate) fn process_commands(&mut self)` | Вычерпывает очередь `IpcRequest` (`try_recv`) и отвечает через `try_respond` |
| `process_audio_events` | `pub(crate) fn process_audio_events(&mut self)` | Вычерпывает `AudioEvent` за кадр |
| `apply_audio_event` | `pub(crate) fn apply_audio_event(&mut self, event: AudioEvent)` | Реакция на одно аудио-событие |
| `process_media_commands` | `pub(crate) fn process_media_commands(&mut self)` | Вычерпывает очередь `MediaCommand` |
| `execute_media_command` | `pub(crate) fn execute_media_command(&mut self, command: MediaCommand)` | Реакция на одну медиа-команду |
| `process_tray_commands` | `pub(crate) fn process_tray_commands(&mut self)` | Вычерпывает очередь `TrayCommand` (`try_recv`), путь открытого окна |
| `execute_tray_command` | `pub(crate) fn execute_tray_command(&mut self, command: TrayCommand)` | Реакция на одну команду трея; общее для окна и парковки |
| `sync_tray` | `pub(crate) fn sync_tray(&mut self)` | Публикует `TraySnapshot` в SNI, только если снимок изменился |
| `resolve_station` | `pub(crate) fn resolve_station(&self, station_uuid: &str) -> Result<Station, RadioError>` | Станция по UUID: `search.results`, затем `world.sample()` (офлайн-снимок), затем `radio.get_station` с таймаутом `STATION_LOOKUP_TIMEOUT` |
| `start_station` | `pub(crate) fn start_station(&mut self, station: Station) -> Result<(), StationStartError>` | Проверка `NoUrl`/`PrivateUrl`, `audio.play(url_resolved)`, `note_station_selected`, `selected_station`, история, click, MPRIS (ADR-0043) |
| `select_station` | `pub(crate) fn select_station(&mut self, station: &Station)` | Клик по маркеру до запуска: `selected_station`, разовый `reveal_station`, сброс `country_filter`, вкладка `Results` (ADR-0036) |
| `take_station_reveal` | `pub(crate) fn take_station_reveal(&mut self) -> Option<String>` | Забирает просьбу прокрутки один раз; `OrbitApp::draw_layers` передаёт UUID строке |
| `spawn_click` | `pub(crate) fn spawn_click(&self, station_uuid: String)` | Фоновая отправка click в Radio Browser |
| `stop_playback` | `pub(crate) fn stop_playback(&mut self) -> Result<(), String>` | `audio.stop()` |
| `toggle_playback` | `pub(crate) fn toggle_playback(&mut self) -> Result<(), String>` | Пауза/восстановление по `PlaybackState` |
| `resume_playback` | `pub(crate) fn resume_playback(&mut self) -> Result<(), String>` | Снятие паузы или повторный запуск последнего URL |
| `set_audio_paused` | `pub(crate) fn set_audio_paused(&mut self, paused: bool) -> Result<(), String>` | Пауза/продолжение потока |
| `set_volume` | `pub(crate) fn set_volume(&mut self, volume: f64) -> Result<(), String>` | Дискретная громкость (трей, MPRIS, IPC): снять `pending_volume`, применить к аудио-ядру и сохранить сразу |
| `nudge_volume` | `pub(crate) fn nudge_volume(&mut self, delta: f64) -> Result<(), String>` | Шаг громкости: `audio.volume() + delta` с округлением до сотых и `clamp` к `0.0..=1.0`, дальше `set_volume`; общий для трея, media и IPC `volume-up`/`volume-down` |
| `preview_volume` | `pub(crate) fn preview_volume(&mut self, volume: f64) -> Result<(), String>` | Перетаскивание слайдера: звук меняется сразу, файл — нет; значение ждёт `commit_volume` или `flush_pending_volume` (ADR-0031) |
| `commit_volume` | `pub(crate) fn commit_volume(&mut self, volume: f64) -> Result<(), String>` | Завершение перетаскивания, клик по треку, клавиатурный шаг и щелчок колеса: применить и записать в `SettingsStore` (колесо — дискретный ввод, pending не открывается, ADR-0031 § 8) |
| `flush_pending_volume` | `pub(crate) fn flush_pending_volume(&mut self)` | Дозаписать незакрытый preview в настройки; зовётся из `end_window`, ошибка — только `warn!` |
| `visible_queue` | `pub(crate) fn visible_queue(&self) -> Vec<Station>` | Снапшот текущего видимого списка — очередь prev/next |
| `visible_queue_len` | `pub(crate) fn visible_queue_len(&self) -> usize` | Число станций в очереди без клонирования; питает `can_go_next`/`can_go_previous` MPRIS |
| `play_relative` | `pub(crate) fn play_relative(&mut self, step: i8)` | Шаг по снапшоту с циклом; сбой запуска — `NoticeKey::StationWithoutUrl` или `NoticeKey::PlaybackFailed` |
| `sync_media` | `pub(crate) fn sync_media(&mut self)` | Публикует состояние в MPRIS; URL проходят гейт `publish_gate` (ярус 1 + вердикт яруса 2, ADR-0025 ред. 4), сам вызов синхронен |
| `toggle_favorite` | `pub(crate) fn toggle_favorite(&mut self, station: &Station)` | Избранное; ошибка — `notify_error_detail(NoticeKey::FavoritesNotSaved, …)` |
| `clear_history` | `pub(crate) fn clear_history(&mut self)` | Очистка истории; успех — `NoticeKey::HistoryCleared`, ошибка — `NoticeKey::ClearHistoryFailed` |
| `focus_country` | `pub(crate) fn focus_country(&mut self, country: &Country)` | Перелёт камеры к центру страны |
| `start_search` | `pub(crate) fn start_search(&mut self)` | Поиск по имени из `search.query`; при активном контексте — локальный фильтр и вкладка `Results` |
| `start_country_search` | `pub(crate) fn start_country_search(&mut self, country: Country)` | Список станций страны |
| `clear_country_selection` | `pub(crate) fn clear_country_selection(&mut self)` | Клик мимо стран: снимает контекст страны, фильтр, список и маркеры; выделенная станция сохраняется (ADR-0036, ревизия 2026-10-31) |
| `start_genre_search` | `pub(crate) fn start_genre_search(&mut self, tag: String)` | Глобальный поиск по тегу |
| `switch_mode` | `pub(crate) fn switch_mode(&mut self, mode: SearchMode)` | Смена режима поиска и сброс контекста |
| `retry_search` | `pub(crate) fn retry_search(&mut self)` | Повтор упавшего запроса: контекст страны, затем жанра, иначе `start_search` |
| `load_more` | `pub(crate) fn load_more(&mut self)` | Догрузка следующей страницы по `last_params`/`next_offset`; ошибка не очищает список, а даёт `NoticeKey::LoadMoreFailed` |
| `country_total` | `pub(crate) fn country_total(&self) -> Option<u32>` | Мировой счётчик станций выбранной страны; `None` без страны, снимка или при офлайн-данных (ADR-0043) |
| `poll_search` | `pub(crate) fn poll_search(&mut self)` | Забор ответа фонового поиска; ответ может быть собран из офлайн-снимка (`SearchState::offline`) |
| `start_roulette` | `pub(crate) fn start_roulette(&mut self)` | Запрос случайной станции |
| `poll_roulette` | `pub(crate) fn poll_roulette(&mut self)` | Забор результата рулетки через `try_recv` |
| `apply_roulette` | `pub(crate) fn apply_roulette(&mut self, outcome: Option<Result<Station, RadioError>>)` | Применение результата рулетки: общий путь UI-кадра и припаркованного цикла |
| `observe_economy` | `pub(crate) fn observe_economy(&mut self, event: &AudioEvent)` | Передаёт аудио-событие экономному режиму: профиль буфера и, со второй попытки реконнекта, подбор более лёгкого потока (ADR-0043) |
| `note_station_selected` | `pub(crate) fn note_station_selected(&mut self, station_uuid: &str)` | Новый выбор станции: снимает ECO у чужой записи, возвращает буфер к умолчаниям и снова разрешает один подбор |
| `poll_economy` | `pub(crate) fn poll_economy(&mut self)` | Кадровое обслуживание: таймер буферизации (3 с) и ответ фонового поиска альтернативы |
| `note_reconnect_attempt` | `pub(crate) fn note_reconnect_attempt(&mut self, attempt: u32)` | Со второй и последующих попыток реконнекта запускает однократный подбор лёгкого потока |
| `economy_stream` | `pub(crate) fn economy_stream(&self) -> bool` | Показывать ли индикатор ECO: текущая станция — подобранная лёгкая замена (ADR-0043 § 4) |
| `load_snapshot` | `pub(crate) fn load_snapshot(&mut self)` | Подставляет офлайн-снимок до первого сетевого запроса; повторный вызов при живых данных — no-op (ADR-0043) |
| `start_world` | `pub(crate) fn start_world(&mut self)` | Запрашивает снимок мира (счётчики стран и мировой топ); повторные вызовы бесплатны, пока идёт задача, есть живые данные или исчерпаны три попытки |
| `poll_world` | `pub(crate) fn poll_world(&mut self)` | Забирает ответ снимка мира; живой ответ снимает `offline` и запускает фоновую запись офлайн-снимка |
| `clear_offline_snapshot` | `pub(crate) fn clear_offline_snapshot(&mut self)` | Удаляет офлайн-снимок с диска (отсутствие файла — успех), запрещает запись до конца сеанса и очищает офлайн-данные карты; успех — `SnapshotCleared`, ошибка — `SnapshotClearFailed` |
| `set_theme` | `pub(crate) fn set_theme(&mut self, theme: ThemeMode) -> Result<(), String>` | Сохранение темы в `SettingsStore` |
| `set_accent` | `pub(crate) fn set_accent(&mut self, accent: AccentColor) -> Result<(), String>` | Сохранение акцента |
| `set_language` | `pub(crate) fn set_language(&mut self, language: Language) -> Result<(), String>` | Сохраняет язык, тут же разворачивает `System` в `self.language` и вызывает `sync_tray()`: меню и строки UI меняются без перезапуска (ADR-0030) |
| `set_viz_bars` | `pub(crate) fn set_viz_bars(&mut self, bars: u8) -> Result<(), String>` | Число столбцов визуализатора (ADR-0019) |
| `set_visualizer_enabled` | `pub(crate) fn set_visualizer_enabled(&mut self, enabled: bool) -> Result<(), String>` | Пользовательский переключатель визуализатора и кольца: сохраняет настройку (сбой — `NoticeKey::VisualizerNotSaved` и `warn!`), затем меняет аудио-анализ по `analysis_needed`, при отказе движка поле откатывается к его значению (ADR-0028, ADR-0031) |
| `set_artwork_allow_http` | `pub(crate) fn set_artwork_allow_http(&mut self, allow: bool) -> Result<(), String>` | Сохраняет opt-in HTTP-обложек (ADR-0025 ред. 4); саму политику фетчера и повтор `Failed` переключает presentation-слой через `ArtworkFetcher::set_allow_http` |
| `set_motion` | `pub(crate) fn set_motion(&mut self, motion: MotionReactivity) -> Result<(), String>` | Сохраняет реактивность движения и пересчитывает необходимость анализа |
| `set_motion_style` | `pub(crate) fn set_motion_style(&mut self, style: MotionStyle) -> Result<(), String>` | Сохраняет характер движения (`Auto` или один из шести стилей) |
| `set_allow_flips` | `pub(crate) fn set_allow_flips(&mut self, allow: bool) -> Result<(), String>` | Сохраняет разрешение переворотов камеры (ADR-0028) |
| `set_reduce_motion` | `pub(crate) fn set_reduce_motion(&mut self, reduce: bool) -> Result<(), String>` | Сохраняет Reduce motion и пересчитывает необходимость анализа |
| `motion_reactivity` | `pub(crate) fn motion_reactivity(&self) -> MotionReactivity` | Эффективная реактивность: Reduce motion → `Off`; Focus — настройка, Overlay — не выше `Low`, Listening — `Off` |
| `analysis_needed` | `pub(crate) fn analysis_needed(&self) -> bool` | `window.visible && (visualizer_enabled \|\| (motion.is_enabled() && !reduce_motion))` — анализ нужен хотя бы одному потребителю, но только при открытом окне |
| `settle_analysis` | `pub(crate) fn settle_analysis(&mut self)` | Переключает анализ под текущую видимость окна; `run_session` зовёт после `begin_window` и перед `park`, ошибка — только `warn!` (ADR-0027) |
| `set_help_visible` | `pub(crate) fn set_help_visible(&mut self, visible: bool) -> Result<(), String>` | Меняет `show_help` и сохраняет `Settings::help_visible`; ошибка — `SettingNotSaved`; по умолчанию карточка включена (ADR-0036) |

Поля `Session` — `pub(crate)`: presentation читает их напрямую (`markers`,
`search.results`, `settings_open`, `world.offline()`/`search.offline`,
`onboarding_open()`) и редактирует через `&mut`-поля
(`search.query`, `panel_open`, `show_help`, `library_tab`, `country_filter`,
`globe_camera`), но побочные эффекты обязан выполнять через методы.
`visualizer_enabled` — пользовательский переключатель визуализатора и кольца;
`spectrum_enabled` — фактическое состояние аудио-анализа, которое может быть
включено и при выключенном визуализаторе, если движение глобуса его требует.
`language`, `settings_open`, `onboarding_open` и `pending_volume` правятся
только методами Core (`set_language`,
`open_settings`/`close_settings`/`toggle_settings`, `finish_onboarding`,
`preview_volume`/`commit_volume`). `economy` и `world` — внутренние поля
`src/session/`: UI не меняет их напрямую, а читает производные признаки через
`economy_stream()` и `WorldBaseline::offline()`.

## 2. Типы-сообщения между Core и presentation

```rust
/// Сообщение для пользователя; как его показать, решает UI (`src/session/notice.rs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Notice {
    Info(NoticeKey),
    Error {
        key: NoticeKey,
        /// Техническая подробность: не переводится (ADR-0030).
        detail: Option<String>,
    },
}

/// Причина сообщения; тексты живут в `i18n::Catalog::notices`, ядро про язык
/// ничего не знает.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NoticeKey {
    HistoryNotSaved,
    FavoritesNotSaved,
    HistoryCleared,
    ClearHistoryFailed,
    SnapshotCleared,
    SnapshotClearFailed,
    PlaybackFailed,
    StationWithoutUrl,
    SearchTaskEnded,
    LoadMoreFailed,
    RouletteStarted { station: Option<String> },
    /// Станция заменена более лёгким потоком (ADR-0043 § 4).
    EconomyStream { bitrate: u32 },
    RouletteFailed,
    RouletteTaskEnded,
    ThemeNotSaved,
    AccentNotSaved,
    MotionNotSaved,
    SettingNotSaved,
    VolumeNotSaved,
    LanguageNotSaved,
    VisualizerNotSaved,
    FftUnavailable,
}

/// Причина, по которой станция не запустилась; UI выбирает сообщение по ключу.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StationStartError {
    /// В записи станции пустой `url_resolved`.
    NoUrl,
    /// Запись Radio Browser ведёт на приватный или служебный адрес.
    PrivateUrl,
    /// Отказ аудио-ядра; техническая подробность не переводится.
    Audio(String),
}

/// Действие с окном, которое Core просит выполнить presentation-слой.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UiIntent {
    Show,
    Close,
}

/// Что делать главному циклу после простоя.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParkOutcome {
    Show,
    Quit,
}

/// Состояние окна (ADR-0018, ADR-0021); `src/session/window.rs`.
#[derive(Debug, Default)]
pub(crate) struct WindowState {
    visible: bool,
    size: Option<[f32; 2]>,
    mode: WindowMode,
    reopen: bool,
    intent: Option<UiIntent>,
    quitting: bool,
}

/// Режим отображения окна (ADR-0021). `Panel` — поверхность бар-виджета (ADR-0033).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum WindowMode {
    #[default]
    Focus,
    Overlay,
    Listening,
    Panel,
}

impl WindowMode {
    /// Режимы, доступные переключателю раскладок в настройках: `Panel` вне
    /// списка, это поверхность, а не раскладка стола; зовёт её команда `panel`
    /// (ADR-0033, ред. 2026-10-28).
    pub(crate) const ALL: [Self; 3] = [Self::Focus, Self::Overlay, Self::Listening];

    /// Размер для `ViewportBuilder` нового окна.
    pub(crate) fn default_size(self) -> [f32; 2];
    pub(crate) fn min_size(self) -> [f32; 2];
    pub(crate) fn title(self) -> &'static str;
    /// Имя режима в IPC-протоколе и в ответе `status`.
    pub(crate) fn as_str(self) -> &'static str;
    /// Декорации снимает всё, что не обычный стол: у `Overlay`, `Listening` и
    /// `Panel` нет CSD-полосы и рамки (ADR-0021 п. 4, ADR-0033).
    pub(crate) fn is_borderless(self) -> bool;
}

/// Разделяемое с фоновыми задачами пробуждение GUI-цикла.
#[derive(Clone, Default)]
pub(crate) struct WakeHandle { /* notifier: Arc<Mutex<Option<Notifier>>> */ }

type Notifier = Arc<dyn Fn() + Send + Sync>;

impl WakeHandle {
    pub(crate) fn install(&self, notifier: impl Fn() + Send + Sync + 'static);
    pub(crate) fn clear(&self);
    pub(crate) fn waker(&self) -> WakeFn;
}

/// Подсистемы, создаваемые в `main` и передаваемые в Core.
pub(crate) struct AppServices {
    pub(crate) audio: AudioEngine,
    pub(crate) geo: GeoService,
    pub(crate) radio: RadioBrowserClient,
    pub(crate) media: Option<MediaControls>,
    pub(crate) tray: Option<TrayHandle>,
    pub(crate) library: StationLibrary,
    pub(crate) settings: SettingsStore,
    /// Публикация кадров `events` (ADR-0039).
    pub(crate) status: StatusFeed,
    /// Публикация кадров `search` для попапа (ADR-0039, ревизия 0.16).
    pub(crate) search: SearchFeed,
    pub(crate) runtime: tokio::runtime::Handle,
}

/// Вкладка панели библиотеки: какой список сейчас видимый.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum LibraryTab {
    #[default]
    Results,
    Favorites,
    History,
}

pub(crate) const WINDOW_SIZE: [f32; 2] = [1100.0, 800.0];
pub(crate) const WINDOW_MIN_SIZE: [f32; 2] = [720.0, 480.0];
pub(crate) const WINDOW_TITLE: &str = "OrbitFM";
pub(crate) const OVERLAY_SIZE: [f32; 2] = [500.0, 620.0];
pub(crate) const OVERLAY_MIN_SIZE: [f32; 2] = [360.0, 480.0];
pub(crate) const OVERLAY_TITLE: &str = "OrbitFM · Overlay";
pub(crate) const LISTENING_SIZE: [f32; 2] = [420.0, 152.0];
pub(crate) const LISTENING_MIN_SIZE: [f32; 2] = [360.0, 120.0];
pub(crate) const LISTENING_TITLE: &str = "OrbitFM · Listening";
pub(crate) const PANEL_SIZE: [f32; 2] = [400.0, 640.0];
pub(crate) const PANEL_MIN_SIZE: [f32; 2] = [360.0, 480.0];
pub(crate) const PANEL_TITLE: &str = "OrbitFM · Panel";
```

| Тип | Назначение |
|-----|-----------|
| `Notice` | Одностороннее сообщение Core → пользователю; UI показывает тостом (`OrbitApp::drain_notices`), строку берёт каталог по `NoticeKey` (ADR-0030) |
| `NoticeKey` | Причина сообщения: ядро называет её, а не текст; `RouletteStarted { station }` несёт подпись станции, `EconomyStream { bitrate }` — битрейт подобранной замены, `SnapshotCleared`/`SnapshotClearFailed` — итог очистки офлайн-снимка |
| `UiIntent::Show` | Окно уже открыто: presentation шлёт `ViewportCommand::Focus` (на Wayland — no-op, фокус даёт композитор) |
| `UiIntent::Close` | Закрыть root-вьюпорт; скрытие и выход различает `WindowState::quitting` |
| `ParkOutcome` | Результат `park()`: снова открыть окно или завершить процесс |
| `WindowState` | Приватное для Core состояние окна: `visible`, `size`, `mode`, `reopen`, `intent`, `quitting` |
| `WindowMode` | Режим окна из `docs/adr/0021`: `Focus` — полный Orbital Desk, `Overlay` и `Listening` — капсулы. Хранится в Core, а не в `SettingsStore`, чтобы `mode` работал из парковки. Локализованные подписи режимов даёт `mode_label` в `src/ui/settings_panel.rs` по каталогу `i18n`, у самого типа метода `label()` нет |
| `StationStartError` | Причина, по которой станция не запустилась (`NoUrl` / `PrivateUrl` / `Audio`); UI по ней выбирает `NoticeKey`, IPC печатает `Display` как `error <текст>` |
| `WakeHandle` | Слот замыкания пробуждения: presentation ставит его при старте прогона и снимает при закрытии; ядро работает с ним как с `WakeFn` |
| `AppServices` | Готовые подсистемы из `launch`; `Session` забирает их в собственность |
| `LibraryTab` | Определяет очередь `prev`/`next` и список, показанный в панели |
| `WINDOW_*` / `OVERLAY_*` / `LISTENING_*` | Геометрия и заголовок каждого режима (ADR-0021 п. 4): `run_window` берёт `mode.title()`, `window_min_size()` и размер из `restore_size()`; заголовки капсул — ключи WM-правил из `wm-configs/` |

### Инвариант: Core не знает про egui

- методы `Session` не принимают `&egui::Context` и ничего не рисуют;
- геометрия окна хранится как `[f32; 2]`, а не `egui::Vec2`;
- ошибки возвращаются как `Result<_, String>` (`set_volume`, `preview_volume`,
  `commit_volume`, `stop_playback`, `toggle_playback`, `resume_playback`,
  `set_audio_paused`, `set_theme`, `set_accent`, `set_language`, `set_viz_bars`,
  `set_visualizer_enabled`, `set_artwork_allow_http`, `set_motion`,
  `set_motion_style`, `set_allow_flips`, `set_reduce_motion`), а не как
  доменный тип UI; запуск станции —
  `Result<(), StationStartError>`;
- способ сообщить пользователю выбирает вызывающий: UI показывает `Toast`
  (`Notice` по ключу каталога, а также собственные `Err` от методов),
  припаркованный цикл пишет `warn!` (`execute_media_command`, `set_help_visible`,
  команды трея);
- `GlobeCamera` — единственное поле-исключение по происхождению: тип из
  `src/render/`, добавленный в Core намеренно (ADR-0018 п. 2), чтобы позиция
  камеры переживала закрытие окна; вращением и зумом камеры управляет
  presentation (`ui::globe_view`), Core вызывает только `fly_to`.

## 3. Контракт жизненного цикла

`run_session` (`src/main.rs`) владеет `Session` и крутит окно:

| Шаг | Вызов | Кто и зачем |
|-----|-------|-------------|
| 0 | `session.set_window_mode(mode)` | Режим первого окна: `launch` передаёт его из `mode <режим>`, когда экземпляр ещё не запущен (ADR-0021 п. 5); иначе `WindowMode::Focus` |
| 1 | `session.restore_size()` | Главный цикл берёт размер **до** создания окна: на Wayland просьбу о размере уже созданного окна композитор вправе отбросить. В капсульных режимах размер — пресет режима |
| 2 | `session.begin_window(size)` | Отмечает окно открытым, снимает leftover-`intent` и `reopen`, фиксирует размер; сразу за ним `settle_analysis()` включает аудио-анализ |
| 3 | `run_window(&mut session, size)` | `eframe::run_native` с `ViewportBuilder` (`with_title(mode.title())`, `with_inner_size`, `with_min_inner_size(mode.min_size())`, `with_transparent`, `Renderer::Wgpu`) и `OrbitApp::new(cc, session)`; возврат означает, что окно закрыто |
| 4 | `session.end_window()` | Presentation-прогон мёртв: `visible = false`, `intent = None`, `wake.clear()`, `flush_pending_volume()`, `sync_media()`, `sync_tray()` |
| 5 | `session.is_quitting()` | Если выход запрошен — `run_session` возвращает `Ok(())`, дальше `launch` останавливает IPC-сервер |
| 6 | `session.take_reopen()` | Смена режима при открытом окне: признак стоит → сразу шаг 1 с геометрией нового режима, минуя парковку (ADR-0021 п. 3) |
| 7 | `session.park()` | Простой без окна; `ParkOutcome::Show` → шаг 1, `ParkOutcome::Quit` → завершение. Перед парковкой цикл снимает окно настроек (`close_settings()`; reopen-ветка шага 6 его сохраняет, ADR-0031 п. 6) и выключает анализ через `settle_analysis()` |

Окно обязано умирать между прогонами, потому что закрытие root-вьюпорта —
единственный честный на Wayland способ убрать окно из раскладки (ADR-0018,
контекст п. 1–4). Поэтому `WakeHandle` живёт вместе с прогоном: `install` в
`OrbitApp::new`, `clear` в `end_window`, и в припаркованном состоянии
внешние источники (IPC-сервер, playwire, фоновые задачи) будят пустой слот без
эффекта, а события читаются очередями напрямую.

Порядок `tick()` (каждый кадр `OrbitApp::logic`): `process_audio_events` →
`poll_economy` → `process_commands` → `process_media_commands` →
`process_tray_commands` → `poll_search` → `poll_companion_search` →
`poll_stream_import` → `poll_roulette` → `poll_world` → `start_world` →
`sync_media` → `sync_tray` → синхронизация `spectrum_enabled` с движком →
`publish_status`. `poll_economy` стоит сразу после аудио-событий: таймер
буферизации и ответ подбора альтернативы должны отработать в том же кадре, что
и событие (ADR-0043).

### 3.1. Офлайн-снимок и экономный режим в жизненном цикле

`Session::new` подставляет офлайн-снимок **до** `start_world`: карта, маркеры,
поиск и рулетка получают локальные данные, не дожидаясь сети. Живой ответ
`poll_world` заменяет их и запускает фоновую запись снимка; `save_world_snapshot`
пропускается, если снимок моложе TTL или пользователь очистил его в этом сеансе
(`snapshot_disabled`). `clear_offline_snapshot` удаляет файл и до конца сеанса
запрещает запись заново.

Экономный режим живёт целиком в `tick()`/`park()`: `apply_audio_event` передаёт
событие в `observe_economy`, `poll_economy` проверяет таймер буферизации и
забирает ответ фонового поиска альтернативы. Аудио-ядро получает только
`set_cache_profile(true/false)`; команда без ответа, ошибка свойства логируется.
Состояние сбрасывается в `note_station_selected` при каждом новом выборе станции.
Ни один из этих путей не меняет IPC-, MPRIS- или `status`-контракты: меняется
только буфер libmpv, выбранная станция и индикатор ECO в presentation.

## 4. Реакция на IPC-команды

`Session::execute` (`src/session/commands.rs`) — единственная точка разбора;
она не знает, есть ли окно. Здесь же `impl From<WindowModeArg> for WindowMode` и
`impl From<ThemeArg> for ThemeMode`: wire-типы остаются в `src/ipc/`, доменные —
в Core, зависимость направлена от Core к инфраструктуре (ADR-0021 п. 5,
ADR-0037).

| `IpcCommand` | Действие | Ответ |
|--------------|----------|-------|
| `Toggle` | `toggle()` | `ok` |
| `Show` | `show()` | `ok` |
| `Hide` | `hide()` (закрытое окно не запрашивает ничего) | `ok` |
| `Panel` | `toggle_dropdown(WindowMode::Panel)` — поверхность, которую зовёт клик бар-виджета (ADR-0033, ред. 2026-10-28) | `ok` |
| `Overlay` | `toggle_dropdown(WindowMode::Overlay)` — капсула Overlay (CLI и «Компоновка») | `ok` |
| `Status` | `status_text()` | см. § 6 |
| `StatusJson` | `status_json()` — снимок схемы 1 (ADR-0037) | JSON одной строкой, см. § 6.1 |
| `ModeQuery` | `window_mode()` | `mode <focus\|overlay\|listening\|panel>`, без побочных эффектов |
| `SetMode { mode }` | `set_window_mode(mode.into())` — из `WindowModeArg` (ADR-0021 п. 5) | `ok` |
| `Quit` | `quit()` | `ok` |
| `Play { station_uuid }` | `resolve_station` → `start_station` | `ok`, иначе `error <ошибка>` |
| `Stop` | `stop_playback()` | `ok`, иначе `error <ошибка>` |
| `Volume { level }` | `set_volume(level)` | `ok`, иначе `error <ошибка>` |
| `PlayPause` | `toggle_playback()` | `ok`, иначе `error <ошибка>` |
| `Next` / `Previous` | `play_relative(±1)` по видимой очереди | `ok`, иначе `error <ошибка>` |
| `Roulette` | `start_roulette()` (повтор игнорируется, пока ответ в пути) | `ok` |
| `VolumeUp` / `VolumeDown` | `nudge_volume(±VOLUME_STEP)` с `clamp` к `0.0..=1.0` | `ok`, иначе `error <ошибка>` |
| `Theme { mode }` | `set_theme(ThemeMode::from(mode))` | `ok`, иначе `error <ошибка>` |
| `Settings` | `open_settings()`: показывает окно, если оно скрыто; в `Listening` сначала переводит в `Overlay` | `ok` |
| `Raw(raw)` | нет действия | `error неизвестная команда: <raw>` |

Ответ клиенту формируется после фактического исполнения команды и отправки
`request.try_respond(response)`; применение команды оконным менеджером
асинхронно и в ответе не подтверждается. Префикс `error` — сигнал неудачи для
CLI (`handle_response`).

## 5. Реакция на `MediaCommand`

`Session::execute_media_command` (`src/session/playback.rs`); ошибки только
логируются через `warn!`, кроме `Next`/`Previous`, где сбой запуска становится
`Notice` по ключу `NoticeKey::StationWithoutUrl` или `NoticeKey::PlaybackFailed`
из `play_relative`.

| Команда | Действие |
|---------|----------|
| `Stop` | `stop_playback()` |
| `SetVolume(volume)` | `set_volume(volume)` |
| `Play` | `resume_playback()` — снять паузу или заново запустить `current_url` |
| `Pause` | `set_audio_paused(true)` |
| `PlayPause` | по `PlaybackState`: `Paused` → снять паузу; `Playing`/`Buffering` → пауза; `Stopped`/`Error` → `resume_playback`; `Reconnecting` → ничего |
| `Next` | `play_relative(1)` |
| `Previous` | `play_relative(-1)` |

## 6. Формат ответа `status`

`format_status` (`src/session/mod.rs`) собирает строку без завершающего перевода:

```
status <stopped|playing|paused|buffering|reconnecting|error>
stationuuid <stationuuid>
name <имя в одну строку>
volume <0.00…1.00, два знака>
window <visible|hidden>
mode <focus|overlay|listening|panel>
```

- строки `stationuuid` и `name` добавляются, только пока состояние не
  `Stopped`/`Error` и известна `current_station`;
- `volume` — `self.audio.volume()` с форматом `{:.2}`;
- `window` отражает `WindowState::visible`, которым управляет OrbitFM, а не
  состояние композитора: в припаркованном состоянии всегда `window hidden`;
- `mode` — `WindowState::mode`: он есть и у скрытого окна и применится следующим
  показом (ADR-0021 п. 5);
- `stationuuid` и `name` пропускаются через `single_line`: последовательности
  любых пробельных символов (включая `\n` и `\t`) сворачиваются в один пробел,
  поэтому каждая пара «ключ значение» остаётся одной строкой;
- текст ошибки в `status` не включается — `Error(_)` даёт `status error`.

### 6.1. `status --json` (схема 1)

`status_json()` (`src/session/mod.rs`) сериализует `StatusPayload` через
`serde_json` в одну компактную строку; полная схема и правило совместимости —
`docs/interfaces/05-ipc.md` § 4. Поля: `schema`, `app`, `version`, `state`,
`station` (полные метаданные или `null`), `volume` (округление до тысячных),
`window { visible, mode }`, `theme`, `language`, `can { previous, next }`,
`ui` — локализованные подписи `TrayText` из `i18n::catalog`. `window.visible`
на macOS отражает `mark_hidden`, а не только закрытие вьюпорта (ADR-0026).
Схема растёт только при удалении или переименовании полей; добавление полей
её не меняет (ADR-0037).

## 7. Припаркованное состояние

`park()` (`src/session/park.rs`) — цикл на `runtime.block_on(wait_on(...))`:

| Ветка `select!` | Событие | Реакция |
|-----------------|---------|---------|
| `commands.recv()` | `Some(request)` | `execute` + `try_respond` — без UI |
| `commands.recv()` | `None` | `Parked::QueueClosed` → `self.quit()` |
| `audio_events.recv()` | `Ok(event)` | `apply_audio_event` |
| `audio_events.recv()` | `Err(_)` | `continue` — ждать дальше |
| `media_recv(media)` | `Some(command)` | `execute_media_command` |
| `media_recv(media)` | `None` | `continue`; без `MediaControls` ветка — `std::future::pending()` и не готова никогда |
| `tray_recv(tray)` | `command` | `execute_tray_command` |
| `tray_recv(tray)` | мёртвый `TrayHandle` | `*tray = None` и ждать дальше; без трея ветка — `std::future::pending()` |
| `roulette_recv(roulette)` | `Some`/`None` | `apply_roulette` (`None` — задача исчезла); без рулетки ветка не готова никогда |

После каждого события — `sync_media()` + `sync_tray()`, затем проверка:
`is_quitting()` → возврат `ParkOutcome::Quit`; `intent == Some(UiIntent::Show)` →
возврат `ParkOutcome::Show` (его ставят `show`, `toggle` из закрытого состояния,
`TrayCommand::Show`/`Toggle` и `TrayCommand::Settings`, который открывает окно
настроек через `open_settings()`).
Команда `hide` в припаркованном состоянии `intent` не ставит (`WindowState::hide`
срабатывает только при `visible`), поэтому цикл остаётся в простое.
`Parked::QueueClosed` означает, что IPC-сервер закрыл очередь: держать процесс
без смысла нечем, это выход как по `quit`.

Команды исполняются **вне** async-контекста: `wait_on` возвращает готовое
событие из `block_on`, а `execute`/`apply_audio_event`/`execute_media_command`/
`execute_tray_command` вызываются уже в синхронном коде, потому что аудио-ядро
зовёт `Handle::block_on` (`play`, `stop`, `set_volume`, `set_paused`).

На macOS `park()` не вызывается: скрытое окно остаётся жить в run loop, те же
события приходят в logic-проходы `tick()`, а `park()` остаётся аварийным путём,
если цикл завершился не по `Close` (ADR-0026).

## 8. Зависимости

`Session` владеет подсистемами и каналами:

| Что владеет | Тип | Источник |
|-------------|-----|----------|
| Аудио-ядро и приёмник событий | `AudioEngine`, `broadcast::Receiver<AudioEvent>` | `AppServices::audio`, подписка в `Session::new` |
| Очередь IPC-запросов | `mpsc::Receiver<IpcRequest>` | `IpcServer::commands()` |
| Гео-сервис | `GeoService` | `AppServices::geo` |
| Клиент Radio Browser | `RadioBrowserClient` (клонируется в фоновые задачи) | `AppServices::radio` |
| Медиа-контролы | `Option<MediaControls>` | `AppServices::media`; `None` — MPRIS недоступен |
| Трей | `Option<TrayHandle>` | `AppServices::tray` из `tray::spawn`; `None` — SNI-хоста нет или сборка без фичи `tray` |
| Библиотека и настройки | `StationLibrary`, `SettingsStore` | `AppServices` |
| Публикатор кадров `events` | `StatusFeed` | `AppServices::status`; получатель `watch` — у IPC-сервера (ADR-0039) |
| Публикатор кадров `search` | `SearchFeed` | `AppServices::search`; подписка `events` для попапа (ADR-0039, ревизия 0.16) |
| Tokio-рантайм | `tokio::runtime::Handle` | `AppServices::runtime` |
| Пробуждение GUI | `WakeHandle` | создаётся в `launch`, делится с IPC/MPRIS/треем/поиском |
| Камера глобуса | `GlobeCamera` | поле `Session` (ADR-0018 п. 2) |
| Снимок мира и офлайн-снимок | `WorldBaseline` (`src/session/world.rs`) | поле `Session`; `services::snapshot` — файл `$XDG_DATA_HOME/orbitfm/cache/stations-snapshot.json` (ADR-0043) |
| Экономный режим | `EconomyState` (`src/session/economy.rs`) | поле `Session`; профиль применяет `AudioEngine::set_cache_profile` (ADR-0043) |
| Окружение мастера | `Desktop` (`src/integration/wm.rs`) | определяется в `Session::new`, нужен шагу «Ваш рабочий стол» (ADR-0041) |

Чистые помощники Core берёт у соседей: `crate::ui::single_line`,
`crate::ui::station_filter::{CountryFilterState, filtered_results}` (без egui) —
и у `crate::render::GlobeCamera`. Типы трея (`TrayCommand`, `TrayHandle`,
`TraySnapshot`, `VOLUME_STEP`) задаёт `src/tray/mod.rs` — контракт в
`docs/interfaces/11-tray.md`. Язык — тип `crate::settings::Language`: Core хранит
его разрешённым и отдаёт через `effective_language()`, а каталог строк
(`src/i18n/`, `docs/interfaces/12-i18n.md`) читает presentation и трей, не ядро.

## 9. Что не является контрактом

- `UiAction` (`src/ui/mod.rs`) — внутренний язык UI-зон, не покидает presentation;
- `GlobeMarker` (`src/session/markers.rs`) — тип данных агрегации: его читает
  `ui::globe_view` и превращает в `render::MarkerVisual`; размер (3 px) и единый
  цвет задаёт `src/render/markers.rs` (ADR-0012), а качество и рейтинг UI
  показывает в подписи `MarkerLabel`;
- `SearchState`, `SearchResponse` (`src/session/search.rs`) —
  внутреннее состояние поиска; наружу идут только поля `Session::search`,
  читаемые UI (`query`, `country_query`, `genre_query`, `context`, `results`,
  `pending`, `empty`, `error`, `offline`, `roulette_pending`, `revision`,
  `has_more`, `loading_more`). `Session::clear_country_selection` — единственный
  способ снять выбранную страну: контекст, фильтр, список и маркеры гаснут
  вместе (ADR-0036, ревизия 2026-10-31);
  `Session::{load_more, country_total, retry_search}` — единственные входы
  пагинации и повтора: `load_more` двигает `offset` по `next_offset`,
  `country_total` читает мировой снимок для подписи «загружено из всего» и
  молчит на офлайн-данных;
- `EconomyState` и `WorldBaseline` (`src/session/economy.rs`, `world.rs`) —
  внутренние типы Core: presentation читает только `economy_stream()` и
  `WorldBaseline::offline()`/`sample()`/`counts()`, но не меняет их напрямую;
  формат файла снимка задаёт `src/services/snapshot.rs` (ADR-0043);
- `ui::onboarding::View`, `IntegrationPanel` и `IntegrationsView` — состояние
  presentation-слоя; Core отдаёт только `onboarding_open()`/
  `onboarding_desktop()`/`finish_onboarding()`/`apply_window_rules()`, а
  установку интеграций исполняет `src/integration/` (ADR-0041, ADR-0042);
- `Parked`, `wait_on`, `media_recv`, `tray_recv`, `roulette_recv`,
  `format_status`, `analysis_required`, `mpris_artist`, `station_geo`,
  `station_popularity`, `country_search_params`, `roulette_label`,
  `nudge_volume`, `tray_snapshot`, `sync_analysis`, `apply_and_persist_volume` —
  приватные функции;
- `WindowMode::label()` не существует: локализованные подписи режимов даёт
  `mode_label` (`src/ui/settings_panel.rs`) по каталогу `i18n`, как и подписи тем,
  акцентов, реактивности и стилей (`src/i18n/labels.rs`);
- `WindowState` и его поля `visible`/`size`/`mode`/`reopen`/`intent`/`quitting` —
  приватны для `src/session/` (`window.rs`) и наружу доступны только через методы
  `Session`; `WindowMode::default_size`/`min_size`/`title`/`as_str` — тоже часть
  Core, presentation читает режим через `window_mode()`;

## 10. Тесты, фиксирующие контракт

- `wake_handle_is_quiet_without_a_window`,
  `analysis_is_suspended_without_a_window`,
  `analysis_follows_visualizer_and_motion_settings` (`src/session/mod.rs`)
- `toggle_alternates_between_close_and_show`,
  `hide_of_a_closed_window_requests_nothing` (`src/session/window.rs`)
- `mode_switch_of_a_closed_window_only_shows_it`,
  `mode_switch_of_an_open_window_reopens_it`,
  `same_mode_requests_show_without_reopen`,
  `each_mode_carries_its_geometry_and_title`,
  `modes_are_listed_from_full_to_minimal`,
  `user_size_is_remembered_only_while_focused` (`src/session/window.rs`)
- `status_reports_hidden_window`, `status_reports_error_state_as_error`,
  `status_includes_station_metadata`, `status_sanitizes_multiline_station_name`
- `unknown_command_reply_is_marked_as_error`, `wire_round_trip_keeps_command_identity`,
  `mode_argument_maps_to_domain_mode`
- `parses_mode_as_query_or_switch`, `round_trips_through_wire_format` (`src/ipc/protocol.rs`)
- `mpris_artist_prefers_location_over_tags`
- `switching_mode_resets_country_and_genre`, `activating_genre_clears_country_and_vice_versa`,
  `stale_search_response_is_dropped`, `roulette_label_includes_country`,
  `country_request_lists_only_that_country`,
  `pagination_follows_the_world_counter_until_exhausted`,
  `appended_page_keeps_the_existing_list_and_skips_duplicates`,
  `offline_results_filter_by_country_name_and_tag`,
  `offline_results_follow_name_offset_and_limit`,
  `offline_results_drop_stations_without_coordinates_when_required`,
  `roulette_snapshot_fallback_skips_excluded_stations`
- `normalized_names_ignore_case_and_punctuation`,
  `lighter_prefers_strictly_smaller_bitrate_but_accepts_unknown_current`,
  `alternative_prefers_the_lightest_public_duplicate`,
  `alternative_ignores_current_entry_and_missing_names`,
  `alternative_accepts_any_bitrate_when_current_is_unknown`,
  `buffering_enters_economy_after_three_seconds`,
  `reconnect_enables_buffer_from_the_second_attempt`,
  `started_restores_defaults_and_keeps_eco_marker`,
  `selecting_another_station_resets_eco_and_buffer`,
  `stopped_and_errors_clear_eco_and_timers`,
  `alternative_is_searched_once_per_station` (`src/session/economy.rs`)
- `counts_from_snapshot_follow_station_country_codes`,
  `snapshot_seeds_counts_and_marks_the_map_offline` (`src/session/world.rs`);
  `roundtrip_keeps_the_top_voted_public_stations`,
  `snapshot_is_capped_at_five_hundred_and_keeps_the_most_voted`,
  `corrupt_or_foreign_snapshots_are_ignored`,
  `snapshot_rechecks_urls_when_loading`,
  `records_are_clipped_to_keep_the_cache_small`,
  `stale_snapshot_is_reported_but_still_loaded` (`src/services/snapshot.rs`)
- `existing_file_disables_the_first_run_wizard` (`src/settings/mod.rs`) —
  существующий файл настроек отключает мастер даже при
  `onboarding_done == false`; отсутствующий файл даёт умолчания
  (`missing_file_uses_defaults`)
- `marker_groups_stations_with_shared_coordinates`,
  `markers_skip_missing_invalid_and_zero_coordinates`, `markers_deduplicate_by_uuid`,
  `popularity_signal_is_votes_only`, `grouped_markers_keep_the_loudest_station`
- контракты трея — в тестах `src/tray/` (`clicks_publish_a_command_and_wake_the_core`,
  `status_is_active_only_when_something_happens`, `tooltip_shows_the_station_or_silence`,
  `menu_offers_window_playback_volume_theme_settings_and_quit`, `theme_radio_marks_the_current_mode`,
  `play_pause_check_follows_the_stream`,
  `every_label_comes_from_the_catalog_for_the_snapshot_language`) и `src/tray/mod.rs`
  (`snapshot_defaults_to_hidden_and_idle`, `snapshots_compare_by_content` — по снимку
  `sync_tray` решает, публиковать ли обновление; сравниваются все пять полей,
  включая `language`)

## 11. Связанные документы

- ADR-0018: Скрытие окна = закрытие вьюпорта с перезапуском UI-цикла
- ADR-0020: Иконка в системном трее (StatusNotifierItem на ksni)
- ADR-0021: Режимы окна (`WindowMode`) и IPC-глагол `mode`
- ADR-0030: Локализация интерфейса (`NoticeKey` вместо текста в ядре)
- ADR-0031: Панель настроек и персистентность (модальное окно, громкость
  commit-on-release)
- ADR-0041: Встроенные шрифты и мастер первого запуска
- ADR-0042: Управление интеграциями из GUI (AST/JSONC, backup, reload без kill)
- ADR-0043: Офлайн-снимок станций и адаптация к плохой сети
- `docs/modules/11-session.md` — модульная спецификация пакета
- `docs/modules/16-integrations.md` — обнаружение окружения и runtime-правила
  (`integration::wm`)
- `docs/interfaces/11-tray.md`, `docs/modules/12-tray.md` — контракт и модуль трея
- `docs/interfaces/12-i18n.md`, `docs/modules/13-i18n.md` — каталоги строк и
  подписи (`labels`)
- `docs/interfaces/07-app.md` — контракт presentation-слоя (`OrbitApp`)
- `docs/interfaces/05-ipc.md`, `docs/interfaces/06-media.md` — протоколы команд
- `docs/architecture/00-overview.md` § 4, § 4.1
