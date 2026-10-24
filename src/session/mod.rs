//! Application Core: подсистемы и состояние, которые переживают закрытие окна.
//!
//! `Session` владеет аудио, гео, Radio Browser, MPRIS, треем, библиотекой и
//! настройками, а также всем долгоживущим состоянием. Он не знает про egui:
//! наружу session отдаёт [`Notice`] и [`UiIntent`], а решение открыть или закрыть
//! окно принимает presentation-слой (`src/app.rs`). Именно так скрытие остаётся
//! честным закрытием root-вьюпорта (ADR-0018), а воспроизведение, IPC, MPRIS и
//! трей продолжают работать.

pub(crate) mod commands;
pub(crate) mod markers;
pub(crate) mod park;
pub(crate) mod playback;
pub(crate) mod search;
pub(crate) mod tray;
pub(crate) mod window;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use tokio::sync::{broadcast, mpsc};

use crate::audio::{AudioEngine, AudioEvent, PlaybackState};
use crate::geo::GeoService;
use crate::ipc::{IpcRequest, WakeFn};
use crate::library::StationLibrary;
use crate::media::MediaControls;
use crate::render::GlobeCamera;
use crate::services::{RadioBrowserClient, Station};
use crate::settings::{MotionReactivity, SettingsStore};
use crate::tray::{TrayHandle, TraySnapshot};
use crate::ui::single_line;
use crate::ui::station_filter::CountryFilterState;

use markers::GlobeMarker;
pub(crate) use park::ParkOutcome;
use search::SearchState;
pub(crate) use window::{UiIntent, WINDOW_TITLE, WindowMode, WindowState};

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

/// Сообщение для пользователя. Как его показать, решает UI (обычно тостом).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Notice {
    Info(String),
    Error(String),
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
    pub(crate) current_url: Option<String>,
    pub(crate) current_station: Option<Station>,
    pub(crate) search: SearchState,
    pub(crate) markers: Vec<GlobeMarker>,
    pub(crate) library_tab: LibraryTab,
    pub(crate) country_filter: CountryFilterState,
    pub(crate) panel_open: bool,
    pub(crate) show_help: bool,
    pub(crate) window: WindowState,
    pub(crate) notices: VecDeque<Notice>,
    /// Последний опубликованный треем снимок: повторная публикация
    /// блокировала бы поток трея dbus-ответом без всякой надобности
    /// (ADR-0020, п. 4).
    pub(crate) tray_snapshot: Option<TraySnapshot>,
}

impl Session {
    pub(crate) fn new(
        commands: mpsc::Receiver<IpcRequest>,
        wake: WakeHandle,
        services: AppServices,
    ) -> Self {
        let AppServices {
            audio,
            geo,
            radio,
            media,
            tray,
            library,
            settings,
            runtime,
        } = services;
        let audio_events = audio.subscribe();
        let visualizer_enabled = audio.spectrum_enabled();
        let spectrum_enabled = visualizer_enabled;
        let show_help = !settings.settings().help_seen;
        Self {
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
            current_url: None,
            current_station: None,
            search: SearchState::default(),
            markers: Vec::new(),
            library_tab: LibraryTab::default(),
            country_filter: CountryFilterState::default(),
            panel_open: true,
            show_help,
            window: WindowState::shown(),
            notices: VecDeque::new(),
            tray_snapshot: None,
        }
    }

    /// Один кадр фонового обслуживания: вызывается UI-циклом при открытом окне.
    pub(crate) fn tick(&mut self) {
        self.process_audio_events();
        self.process_commands();
        self.process_media_commands();
        self.process_tray_commands();
        self.poll_search();
        self.poll_roulette();
        self.sync_media();
        self.sync_tray();
        self.spectrum_enabled = self.audio.spectrum_enabled();
    }

    pub(crate) fn take_intent(&mut self) -> Option<UiIntent> {
        self.window.take_intent()
    }

    /// Забирает очередь сообщений: UI показывает их тостами.
    pub(crate) fn take_notices(&mut self) -> VecDeque<Notice> {
        std::mem::take(&mut self.notices)
    }

    pub(crate) fn notify_info(&mut self, message: impl Into<String>) {
        self.notices.push_back(Notice::Info(message.into()));
    }

    pub(crate) fn notify_error(&mut self, message: impl Into<String>) {
        self.notices.push_back(Notice::Error(message.into()));
    }

    /// Показ окна: `launch` вызывает перед каждым прогоном UI.
    pub(crate) fn begin_window(&mut self, size: [f32; 2]) {
        self.window.begin(size);
    }

    /// Закрытие root-вьюпорта: окно скрыто, процесс и воспроизведение живы.
    pub(crate) fn end_window(&mut self) {
        self.window.end();
        self.wake.clear();
        self.sync_media();
        self.sync_tray();
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
            WindowMode::Listening => MotionReactivity::Off,
        }
    }

    /// Нужен ли аудио-анализ: визуализатору или движению глобуса.
    pub(crate) fn analysis_needed(&self) -> bool {
        let settings = self.settings.settings();
        self.visualizer_enabled || (settings.motion.is_enabled() && !settings.reduce_motion)
    }

    /// Меняет режим и гарантирует, что окно будет видно: в `Focus`-раскладке
    /// капсула бессмысленна, поэтому `mode <режим>` показывает окно.
    pub(crate) fn set_window_mode(&mut self, mode: WindowMode) {
        self.window.set_mode(mode);
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

    /// Строка ответа на IPC-команду `status`.
    pub(crate) fn status_text(&self) -> String {
        let state = self.audio.state();
        let station = match state {
            PlaybackState::Stopped | PlaybackState::Error(_) => None,
            _ => self.current_station.as_ref(),
        };
        format_status(
            &state,
            self.audio.volume(),
            self.window.visible,
            self.window.mode,
            station,
        )
    }
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

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
}
