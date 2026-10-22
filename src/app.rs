use std::collections::{HashMap, HashSet};
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
use crate::render::{
    GlobeCamera, GlobeInput, GlobeRenderer, MarkerColors, MarkerVisual, SpectrumOverlay,
    SpectrumTarget, paint_globe, paint_markers, sphere_point,
};
use crate::services::{RadioBrowserClient, RadioError, SearchParams, Station};
use crate::settings::{AccentColor, Settings, SettingsStore, ThemeMode};

const WINDOW_TITLE: &str = "OrbitFM";
const CARD_ALPHA: u8 = 236;
const SEARCH_RESULT_LIMIT: u32 = 30;
const MARKER_RESULT_LIMIT: u32 = 60;
const MARKER_CLICK_RADIUS: f32 = 10.0;
const MARKER_GROUP_SCALE: f64 = 100.0;
const STATION_LOOKUP_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone)]
struct StationMarker {
    station: Station,
    lat: f32,
    lon: f32,
    count: usize,
}

struct SearchOutcome {
    stations: Vec<Station>,
    geo_stations: Vec<Station>,
}

/// Активная вкладка списка станций под поиском.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum LibraryTab {
    #[default]
    Results,
    Favorites,
    History,
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
    settings_error: Option<String>,
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
    markers: Vec<StationMarker>,
    library_tab: LibraryTab,
    library_error: Option<String>,
    search_pending: bool,
    search_empty: bool,
    search_rx: Option<mpsc::UnboundedReceiver<Result<SearchOutcome, RadioError>>>,
    search_error: Option<String>,
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
        configure_style(&cc.egui_ctx, settings.settings());
        wake.install(cc.egui_ctx.clone());
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
            settings_error: None,
            globe_camera: GlobeCamera::default(),
            globe_ready,
            spectrum_overlay: SpectrumOverlay::default(),
            spectrum_enabled: false,
            spectrum_error: None,
            current_url: None,
            current_station: None,
            search_query: String::new(),
            search_country: None,
            search_results: Vec::new(),
            markers: Vec::new(),
            library_tab: LibraryTab::default(),
            library_error: None,
            search_pending: false,
            search_empty: false,
            search_rx: None,
            search_error: None,
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
                Ok(station) => match self.start_station(station) {
                    Ok(()) => "ok".to_owned(),
                    Err(err) => format!("error {err}"),
                },
                Err(err) => format!("error {err}"),
            },
            IpcCommand::Stop => match self.runtime.block_on(self.audio.stop()) {
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

    fn start_station(&mut self, station: Station) -> Result<(), String> {
        let url = station.url_resolved.trim().to_owned();
        if url.is_empty() {
            return Err("у станции нет url_resolved".to_owned());
        }

        self.runtime
            .block_on(self.audio.play(&url))
            .map_err(|err| err.to_string())?;

        self.current_url = Some(url);
        self.current_station = Some(station.clone());
        match self.library.record_played(&station) {
            Ok(()) => self.library_error = None,
            Err(err) => {
                warn!(error = %err, "не удалось сохранить историю станций");
                self.library_error = Some(err.to_string());
            }
        }
        self.spawn_click(station.stationuuid);
        self.sync_media();
        Ok(())
    }

    fn toggle_favorite(&mut self, station: &Station) {
        match self.library.toggle_favorite(station) {
            Ok(favorite) => {
                debug!(favorite, "избранное обновлено");
                self.library_error = None;
            }
            Err(err) => {
                warn!(error = %err, "не удалось сохранить избранное");
                self.library_error = Some(err.to_string());
            }
        }
    }

    fn clear_history(&mut self) {
        match self.library.clear_history() {
            Ok(()) => self.library_error = None,
            Err(err) => {
                warn!(error = %err, "не удалось очистить историю");
                self.library_error = Some(err.to_string());
            }
        }
    }

    fn set_theme(&mut self, ctx: &egui::Context, theme: ThemeMode) {
        match self.settings.set_theme(theme) {
            Ok(()) => self.settings_error = None,
            Err(err) => {
                warn!(error = %err, "не удалось сохранить тему");
                self.settings_error = Some(err.to_string());
            }
        }
        configure_style(ctx, self.settings.settings());
    }

    fn set_accent(&mut self, ctx: &egui::Context, accent: AccentColor) {
        match self.settings.set_accent(accent) {
            Ok(()) => self.settings_error = None,
            Err(err) => {
                warn!(error = %err, "не удалось сохранить акцент");
                self.settings_error = Some(err.to_string());
            }
        }
        configure_style(ctx, self.settings.settings());
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
                self.markers = build_markers(outcome.geo_stations);
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

    fn marker_screens(&self, viewport: egui::Rect) -> Vec<(usize, egui::Pos2)> {
        self.markers
            .iter()
            .enumerate()
            .filter_map(|(index, marker)| {
                self.globe_camera
                    .project_on_sphere(sphere_point(marker.lat, marker.lon), viewport)
                    .map(|position| (index, position))
            })
            .collect()
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
                if let Err(err) = self.runtime.block_on(self.audio.stop()) {
                    warn!(error = %err, "не удалось остановить воспроизведение из MPRIS");
                }
            }
            MediaCommand::SetVolume(volume) => {
                if let Err(err) = self.runtime.block_on(self.audio.set_volume(volume)) {
                    warn!(error = %err, "не удалось изменить громкость из MPRIS");
                }
            }
            MediaCommand::Play => self.resume_playback(),
            MediaCommand::Pause => self.set_audio_paused(true),
            MediaCommand::PlayPause => match self.audio.state() {
                PlaybackState::Paused => self.set_audio_paused(false),
                PlaybackState::Playing | PlaybackState::Buffering => {
                    self.set_audio_paused(true);
                }
                PlaybackState::Stopped | PlaybackState::Error(_) => self.resume_playback(),
                PlaybackState::Reconnecting => {}
            },
            MediaCommand::Next | MediaCommand::Previous => {
                debug!("переключение станций из MPRIS пока не поддерживается");
            }
        }
    }

    fn set_audio_paused(&mut self, paused: bool) {
        if let Err(err) = self.runtime.block_on(self.audio.set_paused(paused)) {
            warn!(error = %err, "не удалось изменить паузу из MPRIS");
        }
    }

    fn resume_playback(&mut self) {
        match self.audio.state() {
            PlaybackState::Paused => self.set_audio_paused(false),
            PlaybackState::Playing | PlaybackState::Buffering | PlaybackState::Reconnecting => {}
            PlaybackState::Stopped | PlaybackState::Error(_) => {
                if let Some(url) = self.current_url.clone()
                    && let Err(err) = self.runtime.block_on(self.audio.play(&url))
                {
                    warn!(error = %err, "не удалось возобновить воспроизведение из MPRIS");
                }
            }
        }
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
                    single_line(&station.name),
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

    fn set_spectrum_enabled(&mut self, enabled: bool) {
        match self
            .runtime
            .block_on(self.audio.set_spectrum_enabled(enabled))
        {
            Ok(()) => {
                self.spectrum_enabled = enabled;
                self.spectrum_error = None;
            }
            Err(err) => {
                self.spectrum_error = Some(err.to_string());
                self.spectrum_enabled = self.audio.spectrum_enabled();
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
        let playback = playback_label(&self.audio.state());
        let volume = self.audio.volume() * 100.0;
        let mut hide_requested = false;
        let mut quit_requested = false;
        let mut search_requested = false;
        let mut spectrum_requested: Option<bool> = None;
        let mut theme_change: Option<ThemeMode> = None;
        let mut accent_change: Option<AccentColor> = None;
        let mut selected_station: Option<Station> = None;
        let mut selected_country: Option<Country> = None;
        let mut favorite_toggle: Option<Station> = None;
        let mut clear_history_requested = false;

        let card = egui::Frame::default()
            .fill(palette.card_fill)
            .stroke(egui::Stroke::new(1.0, palette.card_stroke))
            .corner_radius(egui::CornerRadius::same(18))
            .inner_margin(egui::Margin::same(22));

        let outer = egui::Frame::default()
            .fill(egui::Color32::TRANSPARENT)
            .inner_margin(egui::Margin::same(8));

        egui::CentralPanel::default().frame(outer).show(ui, |ui| {
            card.show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.set_min_height(ui.available_height());

                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(WINDOW_TITLE).size(26.0).strong());
                        ui.label(
                            egui::RichText::new("3D Interactive Global Radio Player")
                                .size(12.0)
                                .color(palette.muted),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Выход").clicked() {
                            quit_requested = true;
                        }
                        if ui.button("Скрыть").clicked() {
                            hide_requested = true;
                        }
                        let mut spectrum = self.spectrum_enabled;
                        if ui
                            .checkbox(&mut spectrum, "FFT")
                            .on_hover_text(
                                "Экспериментальный FFT-визуализатор: агрегированные \
                                 метрики mpv (без PCM)",
                            )
                            .changed()
                        {
                            spectrum_requested = Some(spectrum);
                        }
                    });
                });

                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);

                egui::Frame::default()
                    .fill(palette.subtle_fill)
                    .corner_radius(egui::CornerRadius::same(12))
                    .inner_margin(egui::Margin::symmetric(16, 10))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(egui::RichText::new("Состояние").strong());
                        ui.label(format!(
                            "Окно: {}",
                            if self.state.window_visible {
                                "видимо"
                            } else {
                                "скрыто"
                            }
                        ));
                        ui.label(format!("Воспроизведение: {playback}"));
                        if let Some(station) = &self.current_station {
                            ui.horizontal(|ui| {
                                let favorite = self.library.is_favorite(&station.stationuuid);
                                let (star, hint) = if favorite {
                                    ("★", "Убрать из избранного")
                                } else {
                                    ("☆", "Добавить в избранное")
                                };
                                if ui.small_button(star).on_hover_text(hint).clicked() {
                                    favorite_toggle = Some(station.clone());
                                }
                                ui.label(format!("Станция: {}", single_line(&station.name)));
                            });
                        }
                        ui.label(
                            egui::RichText::new(format!("Громкость: {volume:.0}%"))
                                .color(palette.muted),
                        );
                        if let Some(error) = &self.spectrum_error {
                            ui.label(egui::RichText::new(error).size(11.0).color(palette.error));
                        }
                        if let Some(error) = &self.library_error {
                            ui.label(egui::RichText::new(error).size(11.0).color(palette.error));
                        }
                        if let Some(error) = &self.settings_error {
                            ui.label(egui::RichText::new(error).size(11.0).color(palette.error));
                        }
                    });

                ui.add_space(6.0);
                egui::CollapsingHeader::new("Оформление")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Тема:");
                            for mode in ThemeMode::ALL {
                                if ui
                                    .selectable_label(settings.theme == mode, theme_label(mode))
                                    .clicked()
                                    && settings.theme != mode
                                {
                                    theme_change = Some(mode);
                                }
                            }
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Акцент:");
                            for accent in AccentColor::ALL {
                                let selected = settings.accent == accent;
                                let stroke = if selected {
                                    egui::Stroke::new(2.0, ui.visuals().strong_text_color())
                                } else {
                                    egui::Stroke::new(1.0, palette.card_stroke)
                                };
                                let response = ui
                                    .add(
                                        egui::Button::new("")
                                            .fill(accent_color(accent))
                                            .stroke(stroke)
                                            .min_size(egui::vec2(24.0, 18.0)),
                                    )
                                    .on_hover_text(accent_label(accent));
                                if response.clicked() && !selected {
                                    accent_change = Some(accent);
                                }
                            }
                        });
                    });

                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.search_query)
                            .hint_text("Поиск станции")
                            .desired_width((ui.available_width() - 92.0).max(80.0)),
                    );
                    let enter = response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter));
                    if ui.button("Найти").clicked() || enter {
                        search_requested = true;
                    }
                });
                if let Some(country) = &self.search_country {
                    ui.label(
                        egui::RichText::new(format!("Страна: {}", country_label(country)))
                            .color(palette.accent)
                            .strong(),
                    );
                }
                if !self.markers.is_empty() {
                    let total: usize = self.markers.iter().map(|marker| marker.count).sum();
                    ui.label(
                        egui::RichText::new(format!("Маркеры: {total} станций с координатами"))
                            .size(11.0)
                            .color(palette.muted),
                    );
                }
                if self.search_pending {
                    ui.label(egui::RichText::new("Поиск…").color(palette.muted));
                }
                if let Some(error) = &self.search_error {
                    ui.label(egui::RichText::new(error).color(palette.error));
                }
                if self.search_empty && !self.search_pending && self.search_error.is_none() {
                    ui.label(egui::RichText::new("Станции не найдены").color(palette.muted));
                }
                let has_library =
                    !self.library.favorites().is_empty() || !self.library.history().is_empty();
                if !self.search_results.is_empty() || has_library {
                    ui.add_space(4.0);
                    let favorites_count = self.library.favorites().len();
                    let history_count = self.library.history().len();
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut self.library_tab,
                            LibraryTab::Results,
                            format!("Результаты ({})", self.search_results.len()),
                        );
                        ui.selectable_value(
                            &mut self.library_tab,
                            LibraryTab::Favorites,
                            format!("Избранное ({favorites_count})"),
                        );
                        ui.selectable_value(
                            &mut self.library_tab,
                            LibraryTab::History,
                            format!("История ({history_count})"),
                        );
                    });
                    egui::ScrollArea::vertical()
                        .id_salt("station_library")
                        .max_height(132.0)
                        .show(ui, |ui| match self.library_tab {
                            LibraryTab::Results => {
                                if self.search_results.is_empty() {
                                    ui.label(
                                        egui::RichText::new("Станции не найдены")
                                            .color(palette.muted),
                                    );
                                } else {
                                    for station in &self.search_results {
                                        station_row(
                                            ui,
                                            station,
                                            self.library.is_favorite(&station.stationuuid),
                                            &mut selected_station,
                                            &mut favorite_toggle,
                                        );
                                    }
                                }
                            }
                            LibraryTab::Favorites => {
                                if self.library.favorites().is_empty() {
                                    ui.label(
                                        egui::RichText::new("В избранном пока пусто")
                                            .color(palette.muted),
                                    );
                                } else {
                                    for station in self.library.favorites() {
                                        station_row(
                                            ui,
                                            station,
                                            true,
                                            &mut selected_station,
                                            &mut favorite_toggle,
                                        );
                                    }
                                }
                            }
                            LibraryTab::History => {
                                if self.library.history().is_empty() {
                                    ui.label(
                                        egui::RichText::new("История пока пуста")
                                            .color(palette.muted),
                                    );
                                } else {
                                    for station in self.library.history() {
                                        station_row(
                                            ui,
                                            station,
                                            self.library.is_favorite(&station.stationuuid),
                                            &mut selected_station,
                                            &mut favorite_toggle,
                                        );
                                    }
                                    if ui.button("Очистить историю").clicked() {
                                        clear_history_requested = true;
                                    }
                                }
                            }
                        });
                }

                ui.add_space(10.0);

                let height = (ui.available_height() - 72.0).max(120.0);
                let (globe_rect, response) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), height),
                    egui::Sense::click_and_drag(),
                );
                ui.painter().rect_filled(
                    globe_rect,
                    egui::CornerRadius::same(14),
                    palette.globe_fill,
                );

                if self.globe_ready {
                    let mut globe_input = GlobeInput::default();
                    if response.dragged_by(egui::PointerButton::Secondary) {
                        globe_input.drag_delta = ui.input(|input| input.pointer.delta());
                    }
                    if response.hovered() {
                        globe_input.scroll_delta = ui.input(|input| input.smooth_scroll_delta.y);
                    }
                    if response.clicked_by(egui::PointerButton::Primary) {
                        globe_input.primary_clicked = response.interact_pointer_pos();
                    }
                    let dt = ui.input(|input| input.stable_dt);
                    self.globe_camera.update(dt, &globe_input);

                    let marker_screens = self.marker_screens(globe_rect);

                    if let Some(position) = globe_input.primary_clicked {
                        if let Some(index) = nearest_marker(&marker_screens, position) {
                            selected_station = Some(self.markers[index].station.clone());
                        } else if let Some(point) =
                            self.globe_camera.screen_to_sphere(position, globe_rect)
                            && let Some(country) = self.geo.country_at_sphere_point(
                                f64::from(point.x),
                                f64::from(point.y),
                                f64::from(point.z),
                            )
                        {
                            selected_country = Some(country.clone());
                        }
                    }

                    paint_globe(ui.painter(), globe_rect, &self.globe_camera);

                    let hovered = response
                        .hover_pos()
                        .and_then(|position| nearest_marker(&marker_screens, position));
                    if hovered.is_some() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if !marker_screens.is_empty() {
                        let current_station = self
                            .current_station
                            .as_ref()
                            .map(|station| station.stationuuid.as_str());
                        let visuals: Vec<MarkerVisual> = marker_screens
                            .iter()
                            .map(|(index, position)| {
                                let marker = &self.markers[*index];
                                MarkerVisual {
                                    pos: *position,
                                    highlighted: current_station
                                        == Some(marker.station.stationuuid.as_str()),
                                    count: marker.count,
                                }
                            })
                            .collect();
                        let hovered_label = hovered.map(|index| marker_label(&self.markers[index]));
                        paint_markers(
                            ui.painter(),
                            globe_rect,
                            &visuals,
                            hovered,
                            hovered_label.as_deref(),
                            MarkerColors {
                                marker: palette.marker,
                                ..MarkerColors::default()
                            },
                        );
                    }
                } else {
                    ui.painter().text(
                        globe_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "3D-глобус недоступен",
                        egui::FontId::proportional(16.0),
                        palette.muted,
                    );
                }

                let spectrum_target = self.audio.spectrum().map(|frame| SpectrumTarget {
                    centroid_hz: frame.centroid_hz,
                    spread_hz: frame.spread_hz,
                    flux: frame.flux,
                    flatness: frame.flatness,
                    rms_db: frame.rms_db,
                });
                self.spectrum_overlay
                    .update(spectrum_target, ui.input(|input| input.stable_dt));
                if self.spectrum_overlay.is_visible() {
                    let strip = egui::Rect::from_min_max(
                        egui::pos2(globe_rect.left() + 16.0, globe_rect.bottom() - 76.0),
                        egui::pos2(globe_rect.right() - 16.0, globe_rect.bottom() - 14.0),
                    );
                    self.spectrum_overlay
                        .paint(ui.painter(), strip, ui.input(|input| input.time));
                    ui.ctx().request_repaint_after(Duration::from_millis(33));
                }

                if self.globe_camera.is_animating() {
                    ui.ctx().request_repaint();
                }

                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(
                        "ЛКМ — выбрать страну или станцию · ПКМ + перетаскивание — \
                         вращение · колесо — масштаб",
                    )
                    .size(11.0)
                    .color(palette.muted),
                );

                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "Закрытие окна скрывает его — процесс продолжает работу. \
                         Управление: orbitfm toggle | status | quit",
                    )
                    .size(11.0)
                    .color(palette.muted),
                );
            });
        });

        if search_requested {
            self.start_search();
            ctx.request_repaint();
        }
        if let Some(theme) = theme_change {
            self.set_theme(&ctx, theme);
            ctx.request_repaint();
        }
        if let Some(accent) = accent_change {
            self.set_accent(&ctx, accent);
            ctx.request_repaint();
        }
        if let Some(enabled) = spectrum_requested {
            self.set_spectrum_enabled(enabled);
            ctx.request_repaint();
        }
        if let Some(country) = selected_country {
            self.start_country_search(country);
            ctx.request_repaint();
        }
        if let Some(station) = selected_station {
            match self.start_station(station) {
                Ok(()) => self.search_error = None,
                Err(err) => self.search_error = Some(err),
            }
        }
        if let Some(station) = favorite_toggle {
            self.toggle_favorite(&station);
            ctx.request_repaint();
        }
        if clear_history_requested {
            self.clear_history();
            ctx.request_repaint();
        }
        if hide_requested {
            self.set_window_visible(&ctx, false);
        }
        if quit_requested {
            self.request_quit(&ctx);
        }
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
        status.push_str(&single_line(&station.stationuuid));
        status.push_str("\nname ");
        status.push_str(&single_line(&station.name));
    }
    status.push_str(&format!("\nvolume {volume:.2}\nwindow {window}"));
    status
}

fn build_markers(stations: Vec<Station>) -> Vec<StationMarker> {
    let mut markers: Vec<StationMarker> = Vec::new();
    let mut groups: HashMap<(i64, i64), usize> = HashMap::new();
    let mut seen = HashSet::new();

    for station in stations {
        if !seen.insert(station.stationuuid.clone()) {
            continue;
        }
        let Some((lat, lon)) = station_geo(&station) else {
            continue;
        };
        let key = (
            (lat * MARKER_GROUP_SCALE).round() as i64,
            (lon * MARKER_GROUP_SCALE).round() as i64,
        );
        if let Some(&index) = groups.get(&key) {
            markers[index].count += 1;
        } else {
            groups.insert(key, markers.len());
            markers.push(StationMarker {
                station,
                lat: lat as f32,
                lon: lon as f32,
                count: 1,
            });
        }
    }

    markers
}

fn station_geo(station: &Station) -> Option<(f64, f64)> {
    let lat = station.geo_lat?;
    let lon = station.geo_long?;
    if !lat.is_finite()
        || !lon.is_finite()
        || !(-90.0..=90.0).contains(&lat)
        || !(-180.0..=180.0).contains(&lon)
    {
        return None;
    }
    if lat == 0.0 && lon == 0.0 {
        return None;
    }
    Some((lat, lon))
}

fn nearest_marker(screens: &[(usize, egui::Pos2)], position: egui::Pos2) -> Option<usize> {
    screens
        .iter()
        .filter_map(|(index, marker)| {
            let distance = (*marker - position).length();
            (distance <= MARKER_CLICK_RADIUS).then_some((distance, *index))
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, index)| index)
}

fn marker_label(marker: &StationMarker) -> String {
    let name = single_line(&marker.station.name);
    let name = if name.is_empty() {
        "Без названия".to_owned()
    } else {
        truncate(&name, 40)
    };

    if marker.count > 1 {
        format!("{name} · +{}", marker.count - 1)
    } else {
        name
    }
}

fn station_row(
    ui: &mut egui::Ui,
    station: &Station,
    favorite: bool,
    selected_station: &mut Option<Station>,
    favorite_toggle: &mut Option<Station>,
) {
    ui.horizontal(|ui| {
        let (star, hint) = if favorite {
            ("★", "Убрать из избранного")
        } else {
            ("☆", "Добавить в избранное")
        };
        if ui.small_button(star).on_hover_text(hint).clicked() {
            *favorite_toggle = Some(station.clone());
        }
        if ui.selectable_label(false, station_label(station)).clicked() {
            *selected_station = Some(station.clone());
        }
    });
}

fn single_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }

    let mut truncated: String = value.chars().take(max_chars.saturating_sub(1)).collect();
    truncated.push('…');
    truncated
}

fn country_label(country: &Country) -> String {
    let name = single_line(&country.name);
    let name = if name.is_empty() {
        country.iso_a2.clone()
    } else {
        name
    };

    match country.iso_a3.as_deref() {
        Some(iso_a3) => format!("{name} ({}, {iso_a3})", country.iso_a2),
        None => format!("{name} ({})", country.iso_a2),
    }
}

fn station_label(station: &Station) -> String {
    let name = single_line(&station.name);
    let name = if name.is_empty() {
        "Без названия".to_owned()
    } else {
        truncate(&name, 48)
    };

    let mut details = Vec::new();
    let country = station.countrycode.trim();
    if !country.is_empty() {
        details.push(country.to_owned());
    }
    let codec = station.codec.trim();
    if !codec.is_empty() {
        details.push(codec.to_owned());
    }
    if station.bitrate > 0 {
        details.push(format!("{} kbps", station.bitrate));
    }

    if details.is_empty() {
        name
    } else {
        format!("{name} — {}", details.join(" · "))
    }
}

fn playback_label(state: &PlaybackState) -> String {
    match state {
        PlaybackState::Stopped => "остановлено".to_owned(),
        PlaybackState::Playing => "играет".to_owned(),
        PlaybackState::Paused => "пауза".to_owned(),
        PlaybackState::Buffering => "буферизация".to_owned(),
        PlaybackState::Reconnecting => "переподключение".to_owned(),
        PlaybackState::Error(message) => format!("ошибка: {message}"),
    }
}

fn configure_style(ctx: &egui::Context, settings: Settings) {
    ctx.set_theme(theme_preference(settings.theme));

    let accent = accent_color(settings.accent);
    let selection_text = contrast_text(accent);
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        let mut visuals = match theme {
            egui::Theme::Dark => egui::Visuals::dark(),
            egui::Theme::Light => egui::Visuals::light(),
        };
        visuals.panel_fill = egui::Color32::TRANSPARENT;
        visuals.window_fill = egui::Color32::TRANSPARENT;
        visuals.selection.bg_fill = accent;
        visuals.selection.stroke = egui::Stroke::new(1.0, selection_text);
        visuals.hyperlink_color = match theme {
            egui::Theme::Dark => accent,
            egui::Theme::Light => shade(accent, 0.6),
        };
        ctx.set_visuals_of(theme, visuals);
    }
}

fn theme_preference(mode: ThemeMode) -> egui::ThemePreference {
    match mode {
        ThemeMode::System => egui::ThemePreference::System,
        ThemeMode::Dark => egui::ThemePreference::Dark,
        ThemeMode::Light => egui::ThemePreference::Light,
    }
}

fn theme_label(mode: ThemeMode) -> &'static str {
    match mode {
        ThemeMode::System => "Системная",
        ThemeMode::Dark => "Тёмная",
        ThemeMode::Light => "Светлая",
    }
}

fn accent_label(accent: AccentColor) -> &'static str {
    match accent {
        AccentColor::Blue => "Орбита",
        AccentColor::Green => "Изумруд",
        AccentColor::Amber => "Янтарь",
        AccentColor::Violet => "Фиалка",
        AccentColor::Rose => "Роза",
        AccentColor::Cyan => "Океан",
    }
}

fn accent_color(accent: AccentColor) -> egui::Color32 {
    let [red, green, blue] = accent.rgb();
    egui::Color32::from_rgb(red, green, blue)
}

fn contrast_text(color: egui::Color32) -> egui::Color32 {
    let luma =
        0.299 * f32::from(color.r()) + 0.587 * f32::from(color.g()) + 0.114 * f32::from(color.b());
    if luma > 150.0 {
        egui::Color32::BLACK
    } else {
        egui::Color32::WHITE
    }
}

fn shade(color: egui::Color32, factor: f32) -> egui::Color32 {
    let scale = |channel: u8| (f32::from(channel) * factor).round() as u8;
    egui::Color32::from_rgb(scale(color.r()), scale(color.g()), scale(color.b()))
}

struct Palette {
    card_fill: egui::Color32,
    card_stroke: egui::Color32,
    muted: egui::Color32,
    error: egui::Color32,
    subtle_fill: egui::Color32,
    globe_fill: egui::Color32,
    accent: egui::Color32,
    marker: egui::Color32,
}

impl Palette {
    fn for_ui(theme: egui::Theme, accent: AccentColor) -> Self {
        let marker = accent_color(accent);
        match theme {
            egui::Theme::Dark => Self {
                card_fill: egui::Color32::from_rgba_unmultiplied(12, 15, 23, CARD_ALPHA),
                card_stroke: egui::Color32::from_rgba_unmultiplied(255, 255, 255, 20),
                muted: egui::Color32::from_rgb(150, 158, 176),
                error: egui::Color32::from_rgb(255, 128, 128),
                subtle_fill: egui::Color32::from_rgba_unmultiplied(255, 255, 255, 8),
                globe_fill: egui::Color32::from_rgba_unmultiplied(255, 255, 255, 7),
                accent: marker,
                marker,
            },
            egui::Theme::Light => Self {
                card_fill: egui::Color32::from_rgba_unmultiplied(246, 248, 252, CARD_ALPHA),
                card_stroke: egui::Color32::from_rgba_unmultiplied(15, 20, 30, 40),
                muted: egui::Color32::from_rgb(88, 97, 116),
                error: egui::Color32::from_rgb(176, 40, 40),
                subtle_fill: egui::Color32::from_rgba_unmultiplied(15, 20, 30, 10),
                globe_fill: egui::Color32::from_rgba_unmultiplied(15, 20, 30, 8),
                accent: shade(marker, 0.6),
                marker,
            },
        }
    }
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

    #[test]
    fn playback_labels_are_human_readable() {
        assert_eq!(playback_label(&PlaybackState::Stopped), "остановлено");
        assert_eq!(playback_label(&PlaybackState::Playing), "играет");
        assert_eq!(
            playback_label(&PlaybackState::Error("таймаут".to_owned())),
            "ошибка: таймаут"
        );
    }

    #[test]
    fn truncates_by_characters() {
        assert_eq!(truncate("коротко", 10), "коротко");
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("абвгде", 4), "абв…");
    }

    #[test]
    fn country_label_includes_iso_codes() {
        let country = Country {
            name: "Germany".to_owned(),
            iso_a2: "DE".to_owned(),
            iso_a3: Some("DEU".to_owned()),
        };
        assert_eq!(country_label(&country), "Germany (DE, DEU)");
    }

    #[test]
    fn country_label_falls_back_to_iso_codes() {
        let country = Country {
            name: "  ".to_owned(),
            iso_a2: "XK".to_owned(),
            iso_a3: None,
        };
        assert_eq!(country_label(&country), "XK (XK)");
    }

    #[test]
    fn station_label_includes_country_codec_and_bitrate() {
        assert_eq!(
            station_label(&sample_station()),
            "Test Radio — US · MP3 · 128 kbps"
        );
    }

    #[test]
    fn station_label_falls_back_for_empty_name() {
        let mut station = sample_station();
        station.name = "   ".to_owned();
        assert_eq!(
            station_label(&station),
            "Без названия — US · MP3 · 128 kbps"
        );
    }

    fn geo_station(uuid: &str, lat: f64, lon: f64) -> Station {
        let mut station = sample_station();
        station.stationuuid = uuid.to_owned();
        station.geo_lat = Some(lat);
        station.geo_long = Some(lon);
        station
    }

    #[test]
    fn marker_groups_stations_with_shared_coordinates() {
        let markers = build_markers(vec![
            geo_station("a", 52.5200, 13.4050),
            geo_station("b", 52.5201, 13.4051),
            geo_station("c", 48.8566, 2.3522),
        ]);

        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].count, 2);
        assert_eq!(markers[1].count, 1);
        assert_eq!(markers[0].station.stationuuid, "a");
    }

    #[test]
    fn markers_skip_missing_invalid_and_zero_coordinates() {
        let mut missing = sample_station();
        missing.stationuuid = "missing".to_owned();

        let mut zero = geo_station("zero", 0.0, 0.0);
        zero.stationuuid = "zero".to_owned();

        let markers = build_markers(vec![
            missing,
            zero,
            geo_station("nan", f64::NAN, 13.0),
            geo_station("range", 91.0, 13.0),
            geo_station("valid", 52.52, 13.405),
        ]);

        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].station.stationuuid, "valid");
    }

    #[test]
    fn markers_deduplicate_by_uuid() {
        let markers = build_markers(vec![
            geo_station("same", 52.52, 13.405),
            geo_station("same", 48.85, 2.35),
        ]);

        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].station.stationuuid, "same");
    }

    #[test]
    fn nearest_marker_uses_click_radius() {
        let screens = vec![(0, egui::pos2(10.0, 10.0)), (1, egui::pos2(100.0, 100.0))];

        assert_eq!(nearest_marker(&screens, egui::pos2(14.0, 12.0)), Some(0));
        assert_eq!(nearest_marker(&screens, egui::pos2(50.0, 50.0)), None);
    }

    #[test]
    fn marker_label_reports_group_size() {
        let mut marker = StationMarker {
            station: geo_station("a", 52.52, 13.405),
            lat: 52.52,
            lon: 13.405,
            count: 3,
        };
        marker.station.name = "Groove\nSalad".to_owned();

        assert_eq!(marker_label(&marker), "Groove Salad · +2");
    }

    #[test]
    fn theme_preference_maps_all_modes() {
        assert_eq!(
            theme_preference(ThemeMode::System),
            egui::ThemePreference::System
        );
        assert_eq!(
            theme_preference(ThemeMode::Dark),
            egui::ThemePreference::Dark
        );
        assert_eq!(
            theme_preference(ThemeMode::Light),
            egui::ThemePreference::Light
        );
    }

    #[test]
    fn contrast_text_picks_readable_foreground() {
        assert_eq!(
            contrast_text(accent_color(AccentColor::Amber)),
            egui::Color32::BLACK
        );
        assert_eq!(
            contrast_text(egui::Color32::from_rgb(20, 30, 60)),
            egui::Color32::WHITE
        );
    }

    #[test]
    fn light_palette_darkens_accent_but_keeps_marker_bright() {
        let dark = Palette::for_ui(egui::Theme::Dark, AccentColor::Blue);
        let light = Palette::for_ui(egui::Theme::Light, AccentColor::Blue);
        assert_eq!(dark.accent, dark.marker);
        assert_ne!(light.accent, light.marker);
        assert!(light.accent.r() < light.marker.r());
    }
}
