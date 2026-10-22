use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, warn};

use crate::audio::{AudioEngine, AudioEvent, PlaybackState};
use crate::ipc::{IpcCommand, IpcRequest, WakeFn};
use crate::media::{MediaCommand, MediaControls, PlaybackInfo};
use crate::services::{RadioBrowserClient, RadioError, SearchParams, Station};

const WINDOW_TITLE: &str = "OrbitFM";
const CARD_ALPHA: u8 = 236;
const MUTED_TEXT: egui::Color32 = egui::Color32::from_rgb(150, 158, 176);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(108, 176, 255);
const ERROR_TEXT: egui::Color32 = egui::Color32::from_rgb(255, 128, 128);
const SEARCH_RESULT_LIMIT: u32 = 30;
const STATION_LOOKUP_TIMEOUT: Duration = Duration::from_secs(8);

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
    radio: RadioBrowserClient,
    media: Option<MediaControls>,
    current_url: Option<String>,
    current_station: Option<Station>,
    search_query: String,
    search_results: Vec<Station>,
    search_pending: bool,
    search_rx: Option<mpsc::UnboundedReceiver<Result<Vec<Station>, RadioError>>>,
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
        configure_style(&cc.egui_ctx);
        wake.install(cc.egui_ctx.clone());
        let AppServices {
            audio,
            radio,
            media,
            runtime,
        } = services;
        let audio_events = audio.subscribe();
        let mut app = Self {
            state: AppState::default(),
            commands,
            wake: wake.clone(),
            audio,
            audio_events,
            radio,
            media,
            current_url: None,
            current_station: None,
            search_query: String::new(),
            search_results: Vec::new(),
            search_pending: false,
            search_rx: None,
            search_error: None,
            runtime,
            quitting: false,
        };
        app.sync_media();
        app
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
        self.spawn_click(station.stationuuid);
        self.sync_media();
        Ok(())
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

        let (sender, receiver) = mpsc::unbounded_channel();
        self.search_rx = Some(receiver);
        self.search_pending = true;
        self.search_error = None;

        let radio = self.radio.clone();
        let waker = self.wake.waker();
        let params = SearchParams {
            name: Some(query),
            limit: SEARCH_RESULT_LIMIT,
            hide_broken: true,
            ..SearchParams::default()
        };
        self.runtime.spawn(async move {
            let result = radio.search(params).await;
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
            Ok(Ok(stations)) => {
                self.search_results = stations;
                self.search_pending = false;
                self.search_rx = None;
            }
            Ok(Err(err)) => {
                self.search_error = Some(err.to_string());
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
        let playback = playback_label(&self.audio.state());
        let volume = self.audio.volume() * 100.0;
        let mut hide_requested = false;
        let mut quit_requested = false;
        let mut search_requested = false;
        let mut selected_station: Option<Station> = None;

        let card = egui::Frame::default()
            .fill(egui::Color32::from_rgba_unmultiplied(
                12, 15, 23, CARD_ALPHA,
            ))
            .stroke(egui::Stroke::new(
                1.0,
                egui::Color32::from_rgba_unmultiplied(255, 255, 255, 20),
            ))
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
                                .color(MUTED_TEXT),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Выход").clicked() {
                            quit_requested = true;
                        }
                        if ui.button("Скрыть").clicked() {
                            hide_requested = true;
                        }
                    });
                });

                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);

                egui::Frame::default()
                    .fill(egui::Color32::from_rgba_unmultiplied(255, 255, 255, 8))
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
                        ui.label(
                            egui::RichText::new(format!("Громкость: {volume:.0}%"))
                                .color(MUTED_TEXT),
                        );
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
                if self.search_pending {
                    ui.label(egui::RichText::new("Поиск…").color(MUTED_TEXT));
                }
                if let Some(error) = &self.search_error {
                    ui.label(egui::RichText::new(error).color(ERROR_TEXT));
                }
                if !self.search_results.is_empty() {
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical()
                        .max_height(132.0)
                        .show(ui, |ui| {
                            for station in &self.search_results {
                                if ui.selectable_label(false, station_label(station)).clicked() {
                                    selected_station = Some(station.clone());
                                }
                            }
                        });
                }

                ui.add_space(10.0);

                let height = (ui.available_height() - 56.0).max(140.0);
                let (globe_rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), height),
                    egui::Sense::hover(),
                );
                let painter = ui.painter();
                painter.rect_filled(
                    globe_rect,
                    egui::CornerRadius::same(14),
                    egui::Color32::from_rgba_unmultiplied(255, 255, 255, 7),
                );
                let radius = (globe_rect.width().min(globe_rect.height()) * 0.24).max(48.0);
                painter.circle_stroke(
                    globe_rect.center(),
                    radius,
                    egui::Stroke::new(1.5, ACCENT.gamma_multiply(0.7)),
                );
                painter.text(
                    globe_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "3D-глобус\nЭтап 4",
                    egui::FontId::proportional(16.0),
                    MUTED_TEXT,
                );

                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(
                        "Закрытие окна скрывает его — процесс продолжает работу. \
                         Управление: orbitfm toggle | status | quit",
                    )
                    .size(11.0)
                    .color(MUTED_TEXT),
                );
            });
        });

        if search_requested {
            self.start_search();
            ctx.request_repaint();
        }
        if let Some(station) = selected_station {
            match self.start_station(station) {
                Ok(()) => self.search_error = None,
                Err(err) => self.search_error = Some(err),
            }
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

fn configure_style(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);

    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = egui::Color32::TRANSPARENT;
    visuals.window_fill = egui::Color32::TRANSPARENT;
    ctx.set_visuals_of(egui::Theme::Dark, visuals);
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
}
