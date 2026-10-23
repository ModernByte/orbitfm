use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, warn};

use crate::audio::{AudioEngine, AudioEvent, PlaybackState};
use crate::geo::{Country, GeoService};
use crate::ipc::{IpcCommand, IpcRequest, WakeFn};
use crate::library::StationLibrary;
use crate::media::{MediaCommand, MediaControls, PlaybackInfo};
use crate::render::{GlobeCamera, GlobeRenderer, SpectrumOverlay, SpectrumTarget};
use crate::services::{RadioBrowserClient, RadioError, SearchParams, Station};
use crate::settings::{AccentColor, SettingsStore, ThemeMode};
use crate::ui::globe_view::{self, GlobeMarker, GlobeState};
use crate::ui::palette::{self, Palette};
use crate::ui::station_panel::{self, LibraryTab};
use crate::ui::toast::Toast;
use crate::ui::transport::{self, TransportState};
use crate::ui::{self, UiAction, hud};

const WINDOW_TITLE: &str = "OrbitFM";
const SEARCH_RESULT_LIMIT: u32 = 30;
const MARKER_RESULT_LIMIT: u32 = 60;
const STATION_LOOKUP_TIMEOUT: Duration = Duration::from_secs(8);
const HUD_HEIGHT: f32 = 58.0;
const TRANSPORT_HEIGHT: f32 = 48.0;
const PANEL_WIDTH: f32 = 310.0;
const NARROW_WIDTH: f32 = 900.0;
const LAYOUT_GAP: f32 = 8.0;
const HINT_SECONDS: f64 = 10.0;

struct SearchOutcome {
    stations: Vec<Station>,
    geo_stations: Vec<Station>,
}

/// Разделяемый с IPC-сервером слот для пробуждения GUI-цикла.
#[derive(Clone, Default)]
pub struct WakeHandle {
    context: Arc<Mutex<Option<egui::Context>>>,
}

impl WakeHandle {
    pub fn install(&self, ctx: egui::Context) {
        if let Ok(mut slot) = self.context.lock() {
            *slot = Some(ctx);
        }
    }

    pub fn waker(&self) -> WakeFn {
        let context = Arc::clone(&self.context);
        Arc::new(move || {
            if let Ok(slot) = context.lock()
                && let Some(ctx) = slot.as_ref()
            {
                ctx.request_repaint();
            }
        })
    }
}

/// Состояние Application Core.
#[derive(Debug)]
pub struct AppState {
    pub window_visible: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            window_visible: true,
        }
    }
}

/// Подсистемы, которые создаются в `main` и передаются в GUI.
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

impl OrbitApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        commands: mpsc::Receiver<IpcRequest>,
        wake: &WakeHandle,
        services: AppServices,
    ) -> Self {
        let AppServices {
            audio,
            geo,
            radio,
            media,
            library,
            settings,
            runtime,
        } = services;
        palette::configure_style(&cc.egui_ctx, settings.settings());
        wake.install(cc.egui_ctx.clone());
        let now = cc.egui_ctx.input(|input| input.time);
        let audio_events = audio.subscribe();
        let globe_ready = if let Some(render_state) = &cc.wgpu_render_state {
            let renderer = GlobeRenderer::new(
                &render_state.device,
                &render_state.queue,
                render_state.target_format,
            );
            render_state
                .renderer
                .write()
                .callback_resources
                .insert(renderer);
            true
        } else {
            warn!("wgpu render state недоступен: 3D-глобус отключён");
            false
        };
        Self {
            state: AppState::default(),
            commands,
            wake: wake.clone(),
            audio,
            audio_events,
            geo,
            radio,
            media,
            library,
            settings,
            globe_camera: GlobeCamera::default(),
            globe_ready,
            spectrum_overlay: SpectrumOverlay::default(),
            spectrum_enabled: false,
            current_url: None,
            current_station: None,
            search_query: String::new(),
            search_country: None,
            search_results: Vec::new(),
            markers: Vec::new(),
            library_tab: LibraryTab::default(),
            search_pending: false,
            search_empty: false,
            search_rx: None,
            search_error: None,
            panel_open: true,
            hint_until: now + HINT_SECONDS,
            toast: Toast::default(),
            runtime,
            quitting: false,
        }
    }

    fn process_commands(&mut self, ctx: &egui::Context) {
        while let Ok(request) = self.commands.try_recv() {
            let response = self.execute(ctx, request.command());
            request.try_respond(response);
        }
    }

    fn execute(&mut self, ctx: &egui::Context, command: &IpcCommand) -> String {
        match command {
            IpcCommand::Toggle => {
                let minimized = ctx.input(|input| input.viewport().minimized.unwrap_or(false));
                let visible = minimized || !self.state.window_visible;
                self.set_window_visible(ctx, visible);
                "ok".to_owned()
            }
            IpcCommand::Show => {
                self.set_window_visible(ctx, true);
                "ok".to_owned()
            }
            IpcCommand::Hide => {
                self.set_window_visible(ctx, false);
                "ok".to_owned()
            }
            IpcCommand::Status => self.status_text(),
            IpcCommand::Quit => {
                self.request_quit(ctx);
                "ok".to_owned()
            }
            IpcCommand::Play { station_uuid } => match self.resolve_station(station_uuid) {
                Ok(station) => match self.start_station(ctx, station) {
                    Ok(()) => "ok".to_owned(),
                    Err(err) => format!("error {err}"),
                },
                Err(err) => format!("error {err}"),
            },
            IpcCommand::Stop => match self.stop_playback() {
                Ok(()) => "ok".to_owned(),
                Err(err) => format!("error {err}"),
            },
            IpcCommand::Volume { level } => {
                match self.runtime.block_on(self.audio.set_volume(*level)) {
                    Ok(()) => "ok".to_owned(),
                    Err(err) => format!("error {err}"),
                }
            }
            IpcCommand::Raw(raw) => format!("error неизвестная команда: {raw}"),
        }
    }

    fn status_text(&self) -> String {
        let state = self.audio.state();
        let station = match state {
            PlaybackState::Stopped | PlaybackState::Error(_) => None,
            _ => self.current_station.as_ref(),
        };
        format_status(
            &state,
            self.audio.volume(),
            self.state.window_visible,
            station,
        )
    }

    fn resolve_station(&self, station_uuid: &str) -> Result<Station, RadioError> {
        if let Some(station) = self
            .search_results
            .iter()
            .find(|station| station.stationuuid == station_uuid)
        {
            return Ok(station.clone());
        }

        let result = self.runtime.block_on(async {
            tokio::time::timeout(STATION_LOOKUP_TIMEOUT, self.radio.get_station(station_uuid)).await
        });

        match result {
            Ok(result) => result,
            Err(_) => Err(RadioError::Network("таймаут каталога станций".to_owned())),
        }
    }

    fn start_station(&mut self, ctx: &egui::Context, station: Station) -> Result<(), String> {
        let url = station.url_resolved.trim().to_owned();
        if url.is_empty() {
            return Err("у станции нет url_resolved".to_owned());
        }

        self.runtime
            .block_on(self.audio.play(&url))
            .map_err(|err| err.to_string())?;

        self.current_url = Some(url);
        self.current_station = Some(station.clone());
        if let Err(err) = self.library.record_played(&station) {
            warn!(error = %err, "не удалось сохранить историю станций");
            self.toast
                .error(ctx, format!("История не сохранена: {err}"));
        }
        self.spawn_click(station.stationuuid);
        self.sync_media();
        Ok(())
    }

    fn toggle_favorite(&mut self, ctx: &egui::Context, station: &Station) {
        match self.library.toggle_favorite(station) {
            Ok(favorite) => debug!(favorite, "избранное обновлено"),
            Err(err) => {
                warn!(error = %err, "не удалось сохранить избранное");
                self.toast
                    .error(ctx, format!("Избранное не сохранено: {err}"));
            }
        }
    }

    fn clear_history(&mut self, ctx: &egui::Context) {
        match self.library.clear_history() {
            Ok(()) => self.toast.info(ctx, "История очищена"),
            Err(err) => {
                warn!(error = %err, "не удалось очистить историю");
                self.toast.error(ctx, format!("История не очищена: {err}"));
            }
        }
    }

    fn set_theme(&mut self, ctx: &egui::Context, theme: ThemeMode) {
        match self.settings.set_theme(theme) {
            Ok(()) => {}
            Err(err) => {
                warn!(error = %err, "не удалось сохранить тему");
                self.toast.error(ctx, format!("Тема не сохранена: {err}"));
            }
        }
        palette::configure_style(ctx, self.settings.settings());
    }

    fn set_accent(&mut self, ctx: &egui::Context, accent: AccentColor) {
        match self.settings.set_accent(accent) {
            Ok(()) => {}
            Err(err) => {
                warn!(error = %err, "не удалось сохранить акцент");
                self.toast.error(ctx, format!("Акцент не сохранён: {err}"));
            }
        }
        palette::configure_style(ctx, self.settings.settings());
    }

    fn spawn_click(&self, station_uuid: String) {
        let radio = self.radio.clone();
        self.runtime.spawn(async move {
            if let Err(err) = radio.send_click(&station_uuid).await {
                warn!(error = %err, "не удалось отправить click в Radio Browser");
            }
        });
    }

    fn start_search(&mut self) {
        let query = self.search_query.trim().to_owned();
        if query.is_empty() || self.search_pending {
            return;
        }

        self.search_country = None;
        self.begin_search(SearchParams {
            name: Some(query),
            limit: SEARCH_RESULT_LIMIT,
            hide_broken: true,
            ..SearchParams::default()
        });
    }

    fn start_country_search(&mut self, country: Country) {
        self.search_country = Some(country.clone());
        self.begin_search(SearchParams {
            country_code: Some(country.iso_a2),
            limit: SEARCH_RESULT_LIMIT,
            hide_broken: true,
            ..SearchParams::default()
        });
    }

    fn begin_search(&mut self, params: SearchParams) {
        let (sender, receiver) = mpsc::unbounded_channel();
        self.search_rx = Some(receiver);
        self.search_pending = true;
        self.search_empty = false;
        self.search_error = None;
        self.search_results.clear();
        self.markers.clear();
        self.library_tab = LibraryTab::Results;

        let mut geo_params = params.clone();
        geo_params.has_geo_info = true;
        geo_params.limit = MARKER_RESULT_LIMIT;

        let radio = self.radio.clone();
        let waker = self.wake.waker();
        self.runtime.spawn(async move {
            let (stations, geo_stations) =
                tokio::join!(radio.search(params), radio.search(geo_params));
            let result = match stations {
                Ok(stations) => {
                    let geo_stations = match geo_stations {
                        Ok(stations) => stations,
                        Err(err) => {
                            warn!(error = %err, "гео-поиск станций не удался");
                            Vec::new()
                        }
                    };
                    Ok(SearchOutcome {
                        stations,
                        geo_stations,
                    })
                }
                Err(err) => Err(err),
            };
            let _ = sender.send(result);
            waker();
        });
    }

    fn poll_search(&mut self) {
        let result = match self.search_rx.as_mut() {
            Some(receiver) => receiver.try_recv(),
            None => return,
        };

        match result {
            Ok(Ok(outcome)) => {
                self.search_empty = outcome.stations.is_empty();
                self.search_results = outcome.stations;
                self.markers = globe_view::build_markers(outcome.geo_stations);
                self.search_pending = false;
                self.search_rx = None;
            }
            Ok(Err(err)) => {
                self.search_error = Some(err.to_string());
                self.markers.clear();
                self.search_pending = false;
                self.search_rx = None;
            }
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                self.search_error = Some("задача поиска завершилась".to_owned());
                self.search_pending = false;
                self.search_rx = None;
            }
        }
    }

    fn process_audio_events(&mut self) {
        loop {
            match self.audio_events.try_recv() {
                Ok(AudioEvent::Started { url }) => self.current_url = Some(url),
                Ok(
                    AudioEvent::Stopped
                    | AudioEvent::Buffering
                    | AudioEvent::Reconnecting { .. }
                    | AudioEvent::Error { .. }
                    | AudioEvent::EndOfFile,
                ) => {}
                Err(
                    broadcast::error::TryRecvError::Empty | broadcast::error::TryRecvError::Closed,
                ) => break,
                Err(broadcast::error::TryRecvError::Lagged(skipped)) => {
                    warn!(skipped, "часть аудио-событий пропущена");
                }
            }
        }
    }

    fn process_media_commands(&mut self) {
        while let Some(command) = self
            .media
            .as_mut()
            .and_then(|media| media.events().try_recv().ok())
        {
            self.execute_media_command(command);
        }
    }

    fn execute_media_command(&mut self, command: MediaCommand) {
        match command {
            MediaCommand::Stop => {
                if let Err(err) = self.stop_playback() {
                    warn!(error = %err, "не удалось остановить воспроизведение из MPRIS");
                }
            }
            MediaCommand::SetVolume(volume) => {
                if let Err(err) = self.runtime.block_on(self.audio.set_volume(volume)) {
                    warn!(error = %err, "не удалось изменить громкость из MPRIS");
                }
            }
            MediaCommand::Play => {
                if let Err(err) = self.resume_playback() {
                    warn!(error = %err, "не удалось возобновить воспроизведение из MPRIS");
                }
            }
            MediaCommand::Pause => {
                if let Err(err) = self.set_audio_paused(true) {
                    warn!(error = %err, "не удалось поставить на паузу из MPRIS");
                }
            }
            MediaCommand::PlayPause => match self.audio.state() {
                PlaybackState::Paused => {
                    if let Err(err) = self.set_audio_paused(false) {
                        warn!(error = %err, "не удалось снять паузу из MPRIS");
                    }
                }
                PlaybackState::Playing | PlaybackState::Buffering => {
                    if let Err(err) = self.set_audio_paused(true) {
                        warn!(error = %err, "не удалось поставить на паузу из MPRIS");
                    }
                }
                PlaybackState::Stopped | PlaybackState::Error(_) => {
                    if let Err(err) = self.resume_playback() {
                        warn!(error = %err, "не удалось запустить воспроизведение из MPRIS");
                    }
                }
                PlaybackState::Reconnecting => {}
            },
            MediaCommand::Next | MediaCommand::Previous => {
                debug!("переключение станций из MPRIS пока не поддерживается");
            }
        }
    }

    fn set_audio_paused(&mut self, paused: bool) -> Result<(), String> {
        self.runtime
            .block_on(self.audio.set_paused(paused))
            .map_err(|err| err.to_string())
    }

    fn resume_playback(&mut self) -> Result<(), String> {
        match self.audio.state() {
            PlaybackState::Paused => self.set_audio_paused(false),
            PlaybackState::Playing | PlaybackState::Buffering | PlaybackState::Reconnecting => {
                Ok(())
            }
            PlaybackState::Stopped | PlaybackState::Error(_) => match self.current_url.clone() {
                Some(url) => self
                    .runtime
                    .block_on(self.audio.play(&url))
                    .map_err(|err| err.to_string()),
                None => Ok(()),
            },
        }
    }

    fn toggle_playback(&mut self) -> Result<(), String> {
        match self.audio.state() {
            PlaybackState::Paused => self.set_audio_paused(false),
            PlaybackState::Playing | PlaybackState::Buffering => self.set_audio_paused(true),
            PlaybackState::Reconnecting => Ok(()),
            PlaybackState::Stopped | PlaybackState::Error(_) => self.resume_playback(),
        }
    }

    fn stop_playback(&mut self) -> Result<(), String> {
        self.runtime
            .block_on(self.audio.stop())
            .map_err(|err| err.to_string())
    }

    fn sync_media(&mut self) {
        let state = self.audio.state();
        let has_track = !matches!(state, PlaybackState::Stopped | PlaybackState::Error(_));
        let playing = matches!(
            state,
            PlaybackState::Playing | PlaybackState::Buffering | PlaybackState::Reconnecting
        );
        let fallback_url = self.current_url.clone().unwrap_or_default();
        let (base_title, artist, art_url, url) = match &self.current_station {
            Some(station) => {
                let tags = station.tags.trim();
                (
                    ui::single_line(&station.name),
                    (!tags.is_empty()).then(|| tags.to_owned()),
                    station.favicon.clone(),
                    if station.url_resolved.trim().is_empty() {
                        fallback_url
                    } else {
                        station.url_resolved.clone()
                    },
                )
            }
            None => (fallback_url.clone(), None, None, fallback_url),
        };
        let title = if has_track {
            if base_title.is_empty() {
                WINDOW_TITLE.to_owned()
            } else {
                base_title
            }
        } else {
            String::new()
        };

        let info = PlaybackInfo {
            title,
            artist,
            art_url,
            url,
            playing,
            volume: self.audio.volume(),
        };

        if let Some(media) = self.media.as_mut()
            && let Err(err) = media.set_playback(&info)
        {
            warn!(error = %err, "не удалось обновить MPRIS");
        }
    }

    fn set_spectrum_enabled(&mut self, ctx: &egui::Context, enabled: bool) {
        match self
            .runtime
            .block_on(self.audio.set_spectrum_enabled(enabled))
        {
            Ok(()) => {
                self.spectrum_enabled = enabled;
            }
            Err(err) => {
                self.spectrum_enabled = self.audio.spectrum_enabled();
                self.toast.error(ctx, format!("FFT недоступен: {err}"));
            }
        }
    }

    fn set_window_visible(&mut self, ctx: &egui::Context, visible: bool) {
        self.state.window_visible = visible;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(visible));
        if visible {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        ctx.request_repaint();
    }

    fn request_quit(&mut self, ctx: &egui::Context) {
        self.quitting = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let settings = self.settings.settings();
        let palette = Palette::for_ui(ctx.theme(), settings.accent);
        let playback = self.audio.state();
        let volume = self.audio.volume();
        let now = ctx.input(|input| input.time);
        let show_hint = now < self.hint_until;
        let markers_total: usize = self.markers.iter().map(|marker| marker.count).sum();
        let current_favorite = self
            .current_station
            .as_ref()
            .is_some_and(|station| self.library.is_favorite(&station.stationuuid));
        let can_resume = self.current_url.is_some();
        let spectrum_target = self.audio.spectrum().map(|frame| SpectrumTarget {
            centroid_hz: frame.centroid_hz,
            spread_hz: frame.spread_hz,
            flux: frame.flux,
            flatness: frame.flatness,
            rms_db: frame.rms_db,
        });
        let narrow = ui.available_width() < NARROW_WIDTH;
        let mut actions = Vec::new();

        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(egui::Color32::TRANSPARENT)
                    .inner_margin(egui::Margin::same(8)),
            )
            .show(ui, |ui| {
                let viewport = ui.available_rect_before_wrap();
                let hud_height = HUD_HEIGHT.min(viewport.height() * 0.2).max(48.0);
                let transport_height = TRANSPORT_HEIGHT.min(viewport.height() * 0.15).max(40.0);
                let content_height =
                    (viewport.height() - hud_height - transport_height - LAYOUT_GAP * 2.0)
                        .max(120.0);

                let hud_rect = egui::Rect::from_min_size(
                    viewport.min,
                    egui::vec2(viewport.width(), hud_height),
                );
                let content_rect = egui::Rect::from_min_size(
                    egui::pos2(viewport.left(), hud_rect.bottom() + LAYOUT_GAP),
                    egui::vec2(viewport.width(), content_height),
                );
                let transport_rect = egui::Rect::from_min_size(
                    egui::pos2(viewport.left(), content_rect.bottom() + LAYOUT_GAP),
                    egui::vec2(viewport.width(), transport_height),
                );

                ui.scope_builder(egui::UiBuilder::new().max_rect(hud_rect), |ui| {
                    actions.extend(hud::show(
                        ui,
                        &palette,
                        hud::HudState {
                            query: &mut self.search_query,
                            now_playing: self.current_station.as_ref(),
                            playback: &playback,
                            volume,
                            settings,
                            panel_open: self.panel_open,
                        },
                    ));
                });

                let panel_width = if self.panel_open && !narrow {
                    PANEL_WIDTH.min(content_rect.width() * 0.42)
                } else {
                    0.0
                };
                let globe_width = (content_rect.width()
                    - if panel_width > 0.0 {
                        panel_width + LAYOUT_GAP
                    } else {
                        0.0
                    })
                .max(100.0);
                let globe_rect = egui::Rect::from_min_size(
                    content_rect.min,
                    egui::vec2(globe_width, content_rect.height()),
                );

                let globe_response = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(globe_rect), |ui| {
                        globe_view::show(
                            ui,
                            &palette,
                            GlobeState {
                                camera: &mut self.globe_camera,
                                ready: self.globe_ready,
                                markers: &self.markers,
                                current_uuid: self
                                    .current_station
                                    .as_ref()
                                    .map(|station| station.stationuuid.as_str()),
                                spectrum_overlay: &mut self.spectrum_overlay,
                                spectrum_target,
                                show_hint,
                            },
                        )
                    })
                    .inner;
                actions.extend(globe_response.actions);
                if let Some((x, y, z)) = globe_response.clicked_point
                    && let Some(country) = self.geo.country_at_sphere_point(x, y, z)
                {
                    actions.push(UiAction::SelectCountry(country.clone()));
                }
                if globe_response.interacted {
                    self.hint_until = 0.0;
                }

                if panel_width > 0.0 {
                    let panel_rect = egui::Rect::from_min_size(
                        egui::pos2(globe_rect.right() + LAYOUT_GAP, content_rect.top()),
                        egui::vec2(panel_width, content_rect.height()),
                    );
                    ui.scope_builder(egui::UiBuilder::new().max_rect(panel_rect), |ui| {
                        actions.extend(self.draw_station_panel(ui, &palette, markers_total));
                    });
                }

                ui.scope_builder(egui::UiBuilder::new().max_rect(transport_rect), |ui| {
                    actions.extend(transport::show(
                        ui,
                        &palette,
                        TransportState {
                            playback: &playback,
                            current: self.current_station.as_ref(),
                            favorite: current_favorite,
                            can_resume,
                            spectrum_enabled: self.spectrum_enabled,
                        },
                    ));
                });

                if self.panel_open && narrow {
                    let overlay_width =
                        PANEL_WIDTH.min((content_rect.width() - LAYOUT_GAP * 2.0).max(220.0));
                    let overlay_height = (content_rect.height() - LAYOUT_GAP * 2.0).max(160.0);
                    let position = egui::pos2(
                        content_rect.right() - overlay_width - LAYOUT_GAP,
                        content_rect.top() + LAYOUT_GAP,
                    );
                    egui::Area::new(egui::Id::unique("orbitfm_station_panel_overlay"))
                        .fixed_pos(position)
                        .order(egui::Order::Foreground)
                        .show(&ctx, |ui| {
                            ui.allocate_ui_with_layout(
                                egui::vec2(overlay_width, overlay_height),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    actions.extend(self.draw_station_panel(
                                        ui,
                                        &palette,
                                        markers_total,
                                    ));
                                },
                            );
                        });
                }

                self.toast.draw(&ctx, &palette);
                ui.allocate_rect(viewport, egui::Sense::hover());
            });

        self.apply_actions(&ctx, actions);

        if show_hint {
            let remaining = self.hint_until - now;
            ctx.request_repaint_after(Duration::from_secs_f64(remaining.max(0.0)));
        }
    }

    fn draw_station_panel(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        markers_total: usize,
    ) -> Vec<UiAction> {
        station_panel::show(
            ui,
            palette,
            station_panel::PanelState {
                tab: &mut self.library_tab,
                country: self.search_country.as_ref(),
                markers_total,
                results: &self.search_results,
                favorites: self.library.favorites(),
                history: self.library.history(),
                pending: self.search_pending,
                empty: self.search_empty,
                error: self.search_error.as_deref(),
            },
        )
    }

    fn apply_actions(&mut self, ctx: &egui::Context, actions: Vec<UiAction>) {
        for action in actions {
            self.apply_action(ctx, action);
        }
    }

    fn apply_action(&mut self, ctx: &egui::Context, action: UiAction) {
        match action {
            UiAction::Play(station) => {
                if let Err(err) = self.start_station(ctx, station) {
                    self.toast.error(ctx, err);
                }
            }
            UiAction::ToggleFavorite(station) => self.toggle_favorite(ctx, &station),
            UiAction::SelectCountry(country) => self.start_country_search(country),
            UiAction::RunSearch => self.start_search(),
            UiAction::ClearHistory => self.clear_history(ctx),
            UiAction::SetTheme(theme) => self.set_theme(ctx, theme),
            UiAction::SetAccent(accent) => self.set_accent(ctx, accent),
            UiAction::ToggleSpectrum(enabled) => self.set_spectrum_enabled(ctx, enabled),
            UiAction::SetVolume(volume) => {
                if let Err(err) = self.runtime.block_on(self.audio.set_volume(volume)) {
                    self.toast
                        .error(ctx, format!("Громкость не изменена: {err}"));
                }
            }
            UiAction::TogglePlayback => {
                if let Err(err) = self.toggle_playback() {
                    self.toast.error(ctx, err);
                }
            }
            UiAction::Stop => {
                if let Err(err) = self.stop_playback() {
                    self.toast.error(ctx, err);
                }
            }
            UiAction::Hide => self.set_window_visible(ctx, false),
            UiAction::Quit => self.request_quit(ctx),
            UiAction::TogglePanel => self.panel_open = !self.panel_open,
        }
        ctx.request_repaint();
    }
}

impl eframe::App for OrbitApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_audio_events();
        self.process_commands(ctx);
        self.process_media_commands();
        self.poll_search();
        self.sync_media();
        self.spectrum_enabled = self.audio.spectrum_enabled();

        if self.spectrum_enabled
            && matches!(
                self.audio.state(),
                PlaybackState::Playing | PlaybackState::Buffering
            )
        {
            ctx.request_repaint_after(Duration::from_millis(33));
        }

        if ctx.input(|input| input.viewport().close_requested()) {
            if self.quitting {
                return;
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.set_window_visible(ctx, false);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.state.window_visible {
            self.draw(ui);
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
}

fn format_status(
    state: &PlaybackState,
    volume: f64,
    window_visible: bool,
    station: Option<&Station>,
) -> String {
    let window = if window_visible { "visible" } else { "hidden" };
    let mut status = format!("status {}", state.as_str());
    if let Some(station) = station {
        status.push_str("\nstationuuid ");
        status.push_str(&ui::single_line(&station.stationuuid));
        status.push_str("\nname ");
        status.push_str(&ui::single_line(&station.name));
    }
    status.push_str(&format!("\nvolume {volume:.2}\nwindow {window}"));
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_station() -> Station {
        Station {
            stationuuid: "c685c086-b716-4edd-b313-d4a7baf1084c".to_owned(),
            name: "Test Radio".to_owned(),
            url: "http://example.com/live".to_owned(),
            url_resolved: "https://example.com/live".to_owned(),
            homepage: None,
            favicon: None,
            tags: "jazz".to_owned(),
            countrycode: "US".to_owned(),
            language: "english".to_owned(),
            bitrate: 128,
            codec: "MP3".to_owned(),
            geo_lat: None,
            geo_long: None,
        }
    }

    #[test]
    fn status_reports_playback_volume_and_window() {
        assert_eq!(
            format_status(&PlaybackState::Reconnecting, 0.8, true, None),
            "status reconnecting\nvolume 0.80\nwindow visible"
        );
    }

    #[test]
    fn status_includes_station_metadata() {
        let station = sample_station();
        assert_eq!(
            format_status(&PlaybackState::Playing, 0.5, false, Some(&station)),
            "status playing\nstationuuid c685c086-b716-4edd-b313-d4a7baf1084c\n\
             name Test Radio\nvolume 0.50\nwindow hidden"
        );
    }

    #[test]
    fn status_sanitizes_multiline_station_name() {
        let mut station = sample_station();
        station.name = "Radio\n  Nova\tLive".to_owned();
        let status = format_status(&PlaybackState::Playing, 1.0, true, Some(&station));
        assert!(status.contains("\nname Radio Nova Live\n"));
    }

    #[test]
    fn error_state_is_reported_as_error() {
        assert_eq!(
            format_status(
                &PlaybackState::Error("нет сети".to_owned()),
                0.5,
                false,
                None
            ),
            "status error\nvolume 0.50\nwindow hidden"
        );
    }
}
