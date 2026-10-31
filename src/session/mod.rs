//! Application Core: подсистемы и состояние, которые переживают закрытие окна.
//!
//! `Session` владеет аудио, гео, Radio Browser, MPRIS, треем, библиотекой и
//! настройками, а также всем долгоживущим состоянием. Он не знает про egui:
//! наружу session отдаёт [`Notice`] и [`UiIntent`], а решение открыть или закрыть
//! окно принимает presentation-слой (`src/app.rs`). Именно так скрытие остаётся
//! честным закрытием root-вьюпорта (ADR-0018), а воспроизведение, IPC, MPRIS и
//! трей продолжают работать.

pub(crate) mod commands;
pub(crate) mod heat;
pub(crate) mod markers;
pub(crate) mod notice;
pub(crate) mod park;
pub(crate) mod playback;
pub(crate) mod search;
pub(crate) mod tray;
pub(crate) mod window;
pub(crate) mod world;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use tokio::sync::{broadcast, mpsc};

use crate::audio::{AudioEngine, AudioEvent, PlaybackState};
use crate::geo::GeoService;
use crate::ipc::{IpcRequest, StatusFeed, WakeFn};
use crate::library::StationLibrary;
use crate::media::MediaControls;
use crate::render::GlobeCamera;
use crate::services::{RadioBrowserClient, Station};
use crate::settings::{Language, MotionReactivity, SettingsStore, ThemeMode};
use crate::tray::{TrayHandle, TraySnapshot};
use crate::ui::single_line;
use crate::ui::station_filter::CountryFilterState;

use markers::GlobeMarker;
pub(crate) use notice::{Notice, NoticeKey};
pub(crate) use park::ParkOutcome;
pub(crate) use playback::StationStartError;
use search::SearchState;
pub(crate) use window::{UiIntent, WINDOW_TITLE, WindowMode, WindowState};
use world::WorldBaseline;

/// Вкладка панели библиотеки: какой список сейчас видимый.
///
/// Определяет очередь для `previous`/`next` (MPRIS, клавиатура, трей).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum LibraryTab {
    /// Результат текущего поиска или выбранной страны.
    #[default]
    Results,
    Favorites,
    History,
}

/// Разделяемое с фоновыми задачами пробуждение GUI-цикла.
///
/// Presentation-слой кладёт в слот замыкание, зовущее `Context::request_repaint`,
/// и убирает его при закрытии окна. Ядро работает с ним как с `WakeFn`, поэтому
/// про egui ничего не знает.
#[derive(Clone, Default)]
pub(crate) struct WakeHandle {
    notifier: Arc<Mutex<Option<Notifier>>>,
}

/// Замыкание пробуждения: тот же контракт, что у `WakeFn`, но без фоновых задач.
type Notifier = Arc<dyn Fn() + Send + Sync>;

impl WakeHandle {
    pub(crate) fn install(&self, notifier: impl Fn() + Send + Sync + 'static) {
        if let Ok(mut slot) = self.notifier.lock() {
            *slot = Some(Arc::new(notifier));
        }
    }

    /// Слот пуст без окна: пробуждать нечего, события читаются из очередей напрямую.
    pub(crate) fn clear(&self) {
        if let Ok(mut slot) = self.notifier.lock() {
            *slot = None;
        }
    }

    pub(crate) fn waker(&self) -> WakeFn {
        let notifier = Arc::clone(&self.notifier);
        Arc::new(move || {
            if let Ok(slot) = notifier.lock()
                && let Some(notifier) = slot.as_ref()
            {
                notifier();
            }
        })
    }
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
    /// Публикация полного кадра подписчикам `events` (ADR-0039).
    pub(crate) status: StatusFeed,
    pub(crate) runtime: tokio::runtime::Handle,
}

/// Ядро приложения: подсистемы + состояние, живущее между показами окна.
pub(crate) struct Session {
    pub(crate) audio: AudioEngine,
    pub(crate) audio_events: broadcast::Receiver<AudioEvent>,
    pub(crate) commands: mpsc::Receiver<IpcRequest>,
    pub(crate) geo: GeoService,
    pub(crate) radio: RadioBrowserClient,
    pub(crate) media: Option<MediaControls>,
    pub(crate) tray: Option<TrayHandle>,
    pub(crate) library: StationLibrary,
    pub(crate) settings: SettingsStore,
    pub(crate) runtime: tokio::runtime::Handle,
    pub(crate) wake: WakeHandle,

    pub(crate) globe_camera: GlobeCamera,
    /// Пользовательский переключатель cava-визуализатора и кольца.
    pub(crate) visualizer_enabled: bool,
    /// Идёт ли аудио-анализ сейчас: визуализатор или движение глобуса.
    pub(crate) spectrum_enabled: bool,
    /// Разрешённый язык интерфейса: `System` из настроек уже развёрнут (ADR-0030).
    pub(crate) language: Language,
    pub(crate) current_url: Option<String>,
    pub(crate) current_station: Option<Station>,
    /// UUID станции, выбранной в интерфейсе: строка списка и маркер глобуса
    /// показывают одно и то же выделение (ADR-0036).
    pub(crate) selected_station: Option<String>,
    /// Разовая просьба панели прокрутить строку в центр: клик по маркеру
    /// выбирает станцию, которой может не быть на экране.
    pub(crate) reveal_station: Option<String>,
    pub(crate) search: SearchState,
    pub(crate) markers: Vec<GlobeMarker>,
    /// Глобальный снимок Radio Browser: счётчики стран и мировой топ для
    /// тепловой карты (ADR-0035).
    pub(crate) world: WorldBaseline,
    pub(crate) library_tab: LibraryTab,
    pub(crate) country_filter: CountryFilterState,
    pub(crate) panel_open: bool,
    /// Открыто ли модальное окно настроек (ADR-0031).
    pub(crate) settings_open: bool,
    /// Громкость, изменённая перетаскиванием, но ещё не записанная на диск.
    pub(crate) pending_volume: Option<f64>,
    pub(crate) show_help: bool,
    pub(crate) window: WindowState,
    pub(crate) notices: VecDeque<Notice>,
    /// Последний опубликованный треем снимок: повторная публикация
    /// блокировала бы поток трея dbus-ответом без всякой надобности
    /// (ADR-0020, п. 4).
    pub(crate) tray_snapshot: Option<TraySnapshot>,
    /// Публикатор кадров `events` (ADR-0039).
    pub(crate) status: StatusFeed,
}

impl Session {
    pub(crate) fn new(
        commands: mpsc::Receiver<IpcRequest>,
        wake: WakeHandle,
        services: AppServices,
    ) -> Self {
        let AppServices {
            mut audio,
            geo,
            radio,
            media,
            tray,
            library,
            settings,
            status,
            runtime,
        } = services;
        let stored = settings.settings();
        if let Err(err) = runtime.block_on(audio.set_volume(stored.volume)) {
            tracing::warn!(error = %err, "не удалось восстановить громкость из настроек");
        }
        let audio_events = audio.subscribe();
        let visualizer_enabled = stored.visualizer_enabled;
        let spectrum_enabled = audio.spectrum_enabled();
        let language = stored.language.resolve();
        let show_help = stored.help_visible;
        let mut session = Self {
            audio,
            audio_events,
            commands,
            geo,
            radio,
            media,
            tray,
            library,
            settings,
            runtime,
            wake,
            globe_camera: GlobeCamera::default(),
            visualizer_enabled,
            spectrum_enabled,
            language,
            current_url: None,
            current_station: None,
            selected_station: None,
            reveal_station: None,
            search: SearchState::default(),
            markers: Vec::new(),
            world: WorldBaseline::default(),
            library_tab: LibraryTab::default(),
            country_filter: CountryFilterState::default(),
            panel_open: true,
            settings_open: false,
            pending_volume: None,
            show_help,
            window: WindowState::shown(),
            notices: VecDeque::new(),
            tray_snapshot: None,
            status,
        };
        // Снимок мира уходит в фон сразу: карта стран должна закраситься до
        // первого выбора в панели.
        session.start_world();
        // Первый кадр публикуется до появления окна: подписка `events`,
        // пришедшая на старте, получает snapshot, а не ждёт кадра UI (ADR-0039).
        session.publish_status();
        session
    }

    /// Один кадр фонового обслуживания: вызывается UI-циклом при открытом окне.
    pub(crate) fn tick(&mut self) {
        self.process_audio_events();
        self.process_commands();
        self.process_media_commands();
        self.process_tray_commands();
        self.poll_search();
        self.poll_roulette();
        self.poll_world();
        self.start_world();
        self.sync_media();
        self.sync_tray();
        self.spectrum_enabled = self.audio.spectrum_enabled();
        self.publish_status();
    }

    pub(crate) fn take_intent(&mut self) -> Option<UiIntent> {
        self.window.take_intent()
    }

    /// Забирает очередь сообщений: UI показывает их тостами.
    pub(crate) fn take_notices(&mut self) -> VecDeque<Notice> {
        std::mem::take(&mut self.notices)
    }

    pub(crate) fn notify_info(&mut self, key: NoticeKey) {
        self.notices.push_back(Notice::Info(key));
    }

    pub(crate) fn notify_error(&mut self, key: NoticeKey) {
        self.notices.push_back(Notice::Error { key, detail: None });
    }

    /// Ошибка с технической подробностью: `detail` не переводится (ADR-0030).
    pub(crate) fn notify_error_detail(&mut self, key: NoticeKey, detail: impl Into<String>) {
        self.notices.push_back(Notice::Error {
            key,
            detail: Some(detail.into()),
        });
    }

    /// Язык, на котором UI рисует строки. `language` уже разрешён при старте
    /// и при каждой смене настройки, поэтому `System` здесь не встречается.
    pub(crate) fn effective_language(&self) -> Language {
        self.language
    }

    /// Открывает окно настроек; в Listening его нет (ADR-0031).
    pub(crate) fn open_settings(&mut self) {
        if self.window.mode == WindowMode::Listening {
            self.set_window_mode(WindowMode::Overlay);
        }
        self.settings_open = true;
        if !self.window.visible {
            self.show();
        }
    }

    pub(crate) fn close_settings(&mut self) {
        self.settings_open = false;
    }

    pub(crate) fn toggle_settings(&mut self) {
        if self.settings_open {
            self.close_settings();
        } else {
            self.open_settings();
        }
    }

    /// Показ окна: `launch` вызывает перед каждым прогоном UI.
    pub(crate) fn begin_window(&mut self, size: [f32; 2]) {
        self.window.begin(size);
        self.publish_status();
    }

    /// Закрытие root-вьюпорта: окно скрыто, процесс и воспроизведение живы.
    pub(crate) fn end_window(&mut self) {
        self.window.end();
        self.wake.clear();
        self.flush_pending_volume();
        self.sync_media();
        self.sync_tray();
        self.publish_status();
    }

    pub(crate) fn set_window_size(&mut self, size: [f32; 2]) {
        self.window.remember_size(size);
    }

    /// Размер для `ViewportBuilder` нового окна (ADR-0021 п. 4).
    pub(crate) fn restore_size(&self) -> [f32; 2] {
        self.window.restore_size()
    }

    /// Минимум для `ViewportBuilder` нового окна (ADR-0021 п. 4).
    pub(crate) fn window_min_size(&self) -> [f32; 2] {
        self.window.mode.min_size()
    }

    pub(crate) fn window_mode(&self) -> WindowMode {
        self.window.mode
    }

    /// Эффективная реактивность движения: Reduce motion и режим окна (ADR-0028).
    pub(crate) fn motion_reactivity(&self) -> MotionReactivity {
        let settings = self.settings.settings();
        if settings.reduce_motion {
            return MotionReactivity::Off;
        }
        match self.window.mode {
            WindowMode::Focus => settings.motion,
            WindowMode::Overlay => settings.motion.capped(MotionReactivity::Low),
            WindowMode::Listening | WindowMode::Panel => MotionReactivity::Off,
        }
    }

    /// Нужен ли аудио-анализ: окно открыто и его потребляет визуализатор
    /// или движение глобуса. В простое без окна анализ не нужен никому.
    pub(crate) fn analysis_needed(&self) -> bool {
        let settings = self.settings.settings();
        analysis_required(
            self.window.visible,
            self.visualizer_enabled,
            settings.motion,
            settings.reduce_motion,
        )
    }

    /// Меняет режим и гарантирует, что окно будет видно: в `Focus`-раскладке
    /// капсула бессмысленна, поэтому `mode <режим>` показывает окно.
    pub(crate) fn set_window_mode(&mut self, mode: WindowMode) {
        self.window.set_mode(mode);
    }

    /// Переключатель-выпадающий (ADR-0033): открытая поверхность режима `mode`
    /// закрывается, любой другой режим переключается на него и показывается.
    /// Зовётся командой `panel` (бар-виджет) и командой `overlay` (CLI).
    pub(crate) fn toggle_dropdown(&mut self, mode: WindowMode) {
        self.window.toggle_dropdown(mode);
    }

    /// `true`, если смена режима требует нового прогона UI-цикла (ADR-0021 п. 3).
    pub(crate) fn take_reopen(&mut self) -> bool {
        self.window.take_reopen()
    }

    pub(crate) fn hide(&mut self) {
        self.window.hide();
    }

    pub(crate) fn show(&mut self) {
        self.window.show();
    }

    pub(crate) fn toggle(&mut self) {
        self.window.toggle();
    }

    pub(crate) fn quit(&mut self) {
        self.window.quit();
    }

    pub(crate) fn is_quitting(&self) -> bool {
        self.window.is_quitting()
    }

    /// Идёт ли пересоздание окна из-за смены режима: на macOS такой
    /// `close_requested` отменять нельзя (ADR-0026).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn is_reopening(&self) -> bool {
        self.window.is_reopening()
    }

    /// Видимо ли окно пользователю: на macOS скрытое окно продолжает жить,
    /// поэтому одного признака закрытого вьюпорта недостаточно (ADR-0026).
    pub(crate) fn window_visible(&self) -> bool {
        self.window.visible
    }

    /// Окно спрятано без закрытия root-вьюпорта (macOS): настройки закрываются,
    /// незаписанная громкость фиксируется, анализ и витрины обновляются сразу,
    /// не дожидаясь кадра (ADR-0026).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn mark_hidden(&mut self) {
        self.window.mark_hidden();
        self.close_settings();
        self.flush_pending_volume();
        self.settle_analysis();
        self.sync_media();
        self.sync_tray();
        self.publish_status();
    }

    /// Окно снова видимо: анализ возвращается, витрины отражают показ.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn mark_shown(&mut self) {
        self.window.mark_shown();
        self.settle_analysis();
        self.sync_media();
        self.sync_tray();
        self.publish_status();
    }

    /// Создаёт macOS-иконку в строке меню, когда run loop уже запущен
    /// (требование `tray-icon`, ADR-0026). На других платформах и без фичи
    /// `tray` — no-op.
    pub(crate) fn attach_tray(&mut self) {
        #[cfg(all(target_os = "macos", feature = "tray"))]
        {
            let Some(tray) = self.tray.as_mut() else {
                return;
            };
            if tray.attach() {
                return;
            }
            tracing::warn!("не удалось создать элемент строки меню, продолжаем без трея");
            self.tray = None;
            self.tray_snapshot = None;
        }
    }

    /// Станция, которая сейчас в эфире: `None` для остановки и ошибки.
    fn active_station(&self) -> Option<&Station> {
        match self.audio.state() {
            PlaybackState::Stopped | PlaybackState::Error(_) => None,
            _ => self.current_station.as_ref(),
        }
    }

    /// Строка ответа на IPC-команду `status`.
    pub(crate) fn status_text(&self) -> String {
        format_status(
            &self.audio.state(),
            self.audio.volume(),
            self.window.visible,
            self.window.mode,
            self.active_station(),
        )
    }

    /// Ответ на IPC-команду `status --json` (ADR-0037): одна строка JSON,
    /// схема 1. Поля только добавляются; удаление и переименование — schema 2.
    pub(crate) fn status_json(&self) -> String {
        let payload = build_status_payload(
            &self.audio.state(),
            self.audio.volume(),
            self.window.visible,
            self.window.mode,
            self.active_station(),
            self.settings.settings().theme,
            self.language,
            self.visible_queue_len() > 0,
        );
        serde_json::to_string(&payload).unwrap_or_else(|err| format!("error {err}"))
    }

    /// Публикует полный кадр подписчикам `events` (ADR-0039). Повтор кадра
    /// не будит клиентов, поэтому вызов безопасен на каждом тике UI.
    pub(crate) fn publish_status(&self) {
        self.status.publish(self.status_json());
    }
}

fn analysis_required(
    window_visible: bool,
    visualizer_enabled: bool,
    motion: MotionReactivity,
    reduce_motion: bool,
) -> bool {
    window_visible && (visualizer_enabled || (motion.is_enabled() && !reduce_motion))
}

fn format_status(
    state: &PlaybackState,
    volume: f64,
    window_visible: bool,
    mode: WindowMode,
    station: Option<&Station>,
) -> String {
    let window = if window_visible { "visible" } else { "hidden" };
    let mut status = format!("status {}", state.as_str());
    if let Some(station) = station {
        status.push_str("\nstationuuid ");
        status.push_str(&single_line(&station.stationuuid));
        status.push_str("\nname ");
        status.push_str(&single_line(&station.name));
    }
    status.push_str(&format!(
        "\nvolume {volume:.2}\nwindow {window}\nmode {}",
        mode.as_str()
    ));
    status
}

/// Версия схемы `status --json` (ADR-0037). Растёт только при удалении или
/// переименовании полей; добавление полей её не меняет.
pub(crate) const STATUS_SCHEMA: u32 = 1;

/// Машиночитаемый снимок состояния для плагинов оболочек и бар-виджетов.
#[derive(Debug, serde::Serialize)]
struct StatusPayload {
    schema: u32,
    app: &'static str,
    version: &'static str,
    state: &'static str,
    station: Option<StatusStation>,
    volume: f64,
    window: StatusWindow,
    theme: &'static str,
    language: &'static str,
    can: StatusCan,
    ui: StatusUi,
}

#[derive(Debug, serde::Serialize)]
struct StatusStation {
    uuid: String,
    name: String,
    country: String,
    countrycode: String,
    tags: String,
    favicon: Option<String>,
    homepage: Option<String>,
    bitrate: u32,
    codec: String,
}

impl StatusStation {
    fn new(station: &Station) -> Self {
        Self {
            uuid: station.stationuuid.clone(),
            name: station.name.clone(),
            country: station.country.clone(),
            countrycode: station.countrycode.clone(),
            tags: station.tags.clone(),
            favicon: station.favicon.clone(),
            homepage: station.homepage.clone(),
            bitrate: station.bitrate,
            codec: station.codec.clone(),
        }
    }
}

#[derive(Debug, serde::Serialize)]
struct StatusWindow {
    visible: bool,
    mode: &'static str,
}

#[derive(Debug, serde::Serialize)]
struct StatusCan {
    previous: bool,
    next: bool,
}

/// Локализованные подписи из `i18n::catalog`: QML/JS не дублируют каталог
/// на 11 языках (ADR-0037).
#[derive(Debug, serde::Serialize)]
struct StatusUi {
    play_pause: &'static str,
    stop: &'static str,
    previous: &'static str,
    next: &'static str,
    radio_roulette: &'static str,
    settings: &'static str,
    show_window: &'static str,
    hide_window: &'static str,
    nothing_playing: &'static str,
    volume_up: &'static str,
    volume_down: &'static str,
}

impl StatusUi {
    fn new(language: Language) -> Self {
        let tray = &crate::i18n::catalog(language).tray;
        Self {
            play_pause: tray.play_pause,
            stop: tray.stop,
            previous: tray.previous,
            next: tray.next,
            radio_roulette: tray.radio_roulette,
            settings: tray.settings,
            show_window: tray.show_window,
            hide_window: tray.hide_window,
            nothing_playing: tray.nothing_playing,
            volume_up: tray.volume_up,
            volume_down: tray.volume_down,
        }
    }
}

/// Округление громкости до тысячных: серия `volume-up` иначе даёт
/// `0.30000000000000004` в JSON и в подписях бар-виджетов.
fn round_volume(volume: f64) -> f64 {
    (volume * 1000.0).round() / 1000.0
}

#[allow(clippy::too_many_arguments)]
fn build_status_payload(
    state: &PlaybackState,
    volume: f64,
    window_visible: bool,
    mode: WindowMode,
    station: Option<&Station>,
    theme: ThemeMode,
    language: Language,
    can_skip: bool,
) -> StatusPayload {
    StatusPayload {
        schema: STATUS_SCHEMA,
        app: "orbitfm",
        version: env!("CARGO_PKG_VERSION"),
        state: state.as_str(),
        station: station.map(StatusStation::new),
        volume: round_volume(volume),
        window: StatusWindow {
            visible: window_visible,
            mode: mode.as_str(),
        },
        theme: theme.as_str(),
        language: language.code(),
        can: StatusCan {
            previous: can_skip,
            next: can_skip,
        },
        ui: StatusUi::new(language),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn analysis_is_suspended_without_a_window() {
        assert!(!analysis_required(
            false,
            true,
            MotionReactivity::Normal,
            false
        ));
        assert!(!analysis_required(
            false,
            false,
            MotionReactivity::Normal,
            false
        ));
    }

    #[test]
    fn analysis_follows_visualizer_and_motion_settings() {
        assert!(analysis_required(true, true, MotionReactivity::Off, false));
        assert!(analysis_required(
            true,
            false,
            MotionReactivity::Normal,
            false
        ));
        assert!(!analysis_required(
            true,
            false,
            MotionReactivity::Off,
            false
        ));
        assert!(!analysis_required(
            true,
            false,
            MotionReactivity::Normal,
            true
        ));
        assert!(analysis_required(
            true,
            true,
            MotionReactivity::Normal,
            true
        ));
    }

    #[test]
    fn wake_handle_is_quiet_without_a_window() {
        let wake = WakeHandle::default();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);
        wake.waker()();
        assert_eq!(hits.load(Ordering::SeqCst), 0, "без слота молчит");

        wake.install(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        wake.waker()();
        assert_eq!(hits.load(Ordering::SeqCst), 1);

        wake.clear();
        wake.waker()();
        assert_eq!(
            hits.load(Ordering::SeqCst),
            1,
            "после закрытия окна пробуждать нечего"
        );
    }

    #[test]
    fn status_reports_hidden_window() {
        assert_eq!(
            format_status(
                &PlaybackState::Reconnecting,
                0.8,
                false,
                WindowMode::Focus,
                None
            ),
            "status reconnecting\nvolume 0.80\nwindow hidden\nmode focus"
        );
    }

    #[test]
    fn status_reports_error_state_as_error() {
        assert_eq!(
            format_status(
                &PlaybackState::Error("нет сети".to_owned()),
                0.5,
                false,
                WindowMode::Overlay,
                None
            ),
            "status error\nvolume 0.50\nwindow hidden\nmode overlay"
        );
    }

    #[test]
    fn status_includes_station_metadata() {
        let station = markers::sample_station();
        assert_eq!(
            format_status(
                &PlaybackState::Playing,
                0.5,
                true,
                WindowMode::Focus,
                Some(&station)
            ),
            "status playing\nstationuuid c685c086-b716-4edd-b313-d4a7baf1084c\n\
             name Test Radio\nvolume 0.50\nwindow visible\nmode focus"
        );
    }

    #[test]
    fn status_sanitizes_multiline_station_name() {
        let mut station = markers::sample_station();
        station.name = "Radio\n  Nova\tLive".to_owned();
        let status = format_status(
            &PlaybackState::Playing,
            1.0,
            true,
            WindowMode::Listening,
            Some(&station),
        );
        assert!(status.contains("\nname Radio Nova Live\n"));
        assert!(status.ends_with("\nmode listening"));
    }

    fn status_value(payload: &StatusPayload) -> serde_json::Value {
        serde_json::to_value(payload).expect("снимок сериализуется")
    }

    #[test]
    fn status_json_is_compact_and_keeps_schema_one() {
        let payload = build_status_payload(
            &PlaybackState::Stopped,
            0.5,
            false,
            WindowMode::Panel,
            None,
            ThemeMode::Dark,
            Language::Russian,
            false,
        );
        let json = serde_json::to_string(&payload).expect("снимок сериализуется");
        assert!(!json.contains('\n'), "IPC отвечает одной строкой");
        let value = status_value(&payload);
        assert_eq!(value["schema"], 1);
        assert_eq!(value["app"], "orbitfm");
        assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(value["state"], "stopped");
        assert!(value["station"].is_null());
        assert_eq!(value["volume"], 0.5);
        assert_eq!(value["window"]["visible"], false);
        assert_eq!(value["window"]["mode"], "panel");
        assert_eq!(value["theme"], "dark");
        assert_eq!(value["language"], "ru");
        assert_eq!(value["can"]["previous"], false);
        assert_eq!(value["can"]["next"], false);
    }

    #[test]
    fn status_json_exposes_station_metadata_and_skip_flags() {
        let station = markers::sample_station();
        let payload = build_status_payload(
            &PlaybackState::Playing,
            0.3,
            true,
            WindowMode::Focus,
            Some(&station),
            ThemeMode::System,
            Language::English,
            true,
        );
        let value = status_value(&payload);
        assert_eq!(value["state"], "playing");
        assert_eq!(value["station"]["uuid"], station.stationuuid);
        assert_eq!(value["station"]["name"], station.name);
        assert_eq!(value["station"]["country"], station.country);
        assert_eq!(value["station"]["countrycode"], station.countrycode);
        assert_eq!(value["station"]["tags"], station.tags);
        assert_eq!(
            value["station"]["favicon"],
            serde_json::json!(station.favicon)
        );
        assert_eq!(
            value["station"]["homepage"],
            serde_json::json!(station.homepage)
        );
        assert_eq!(value["station"]["bitrate"], station.bitrate);
        assert_eq!(value["station"]["codec"], station.codec);
        assert_eq!(value["window"]["visible"], true);
        assert_eq!(value["window"]["mode"], "focus");
        assert_eq!(value["theme"], "system");
        assert_eq!(value["language"], "en");
        assert_eq!(value["can"]["previous"], true);
        assert_eq!(value["can"]["next"], true);
    }

    #[test]
    fn status_json_ui_labels_come_from_the_catalog() {
        for language in [Language::English, Language::Russian, Language::Japanese] {
            let payload = build_status_payload(
                &PlaybackState::Stopped,
                1.0,
                true,
                WindowMode::Focus,
                None,
                ThemeMode::Light,
                language,
                false,
            );
            let value = status_value(&payload);
            let tray = &crate::i18n::catalog(language).tray;
            assert_eq!(value["ui"]["play_pause"], tray.play_pause);
            assert_eq!(value["ui"]["stop"], tray.stop);
            assert_eq!(value["ui"]["previous"], tray.previous);
            assert_eq!(value["ui"]["next"], tray.next);
            assert_eq!(value["ui"]["radio_roulette"], tray.radio_roulette);
            assert_eq!(value["ui"]["settings"], tray.settings);
            assert_eq!(value["ui"]["show_window"], tray.show_window);
            assert_eq!(value["ui"]["hide_window"], tray.hide_window);
            assert_eq!(value["ui"]["nothing_playing"], tray.nothing_playing);
            assert_eq!(value["ui"]["volume_up"], tray.volume_up);
            assert_eq!(value["ui"]["volume_down"], tray.volume_down);
            assert_eq!(value["theme"], "light");
        }
    }

    #[test]
    fn status_json_rounds_fractional_volume() {
        let payload = build_status_payload(
            &PlaybackState::Playing,
            0.30000000000000004,
            true,
            WindowMode::Focus,
            None,
            ThemeMode::Dark,
            Language::English,
            false,
        );
        let value = status_value(&payload);
        assert_eq!(value["volume"], 0.3);
    }
}
