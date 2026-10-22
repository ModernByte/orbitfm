use std::sync::{Arc, Mutex};

use eframe::egui;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, warn};

use crate::audio::{AudioEngine, AudioEvent, PlaybackState};
use crate::ipc::{IpcCommand, IpcRequest, WakeFn};
use crate::media::{MediaCommand, MediaControls, PlaybackInfo};

const WINDOW_TITLE: &str = "OrbitFM";
const CARD_ALPHA: u8 = 236;
const MUTED_TEXT: egui::Color32 = egui::Color32::from_rgb(150, 158, 176);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(108, 176, 255);

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

/// Состояние Application Core для Этапа 1.
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

pub struct OrbitApp {
    state: AppState,
    commands: mpsc::Receiver<IpcRequest>,
    audio: AudioEngine,
    audio_events: broadcast::Receiver<AudioEvent>,
    media: Option<MediaControls>,
    current_url: Option<String>,
    runtime: tokio::runtime::Handle,
    quitting: bool,
}

impl OrbitApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        commands: mpsc::Receiver<IpcRequest>,
        wake: &WakeHandle,
        audio: AudioEngine,
        media: Option<MediaControls>,
        runtime: tokio::runtime::Handle,
    ) -> Self {
        configure_style(&cc.egui_ctx);
        wake.install(cc.egui_ctx.clone());
        let audio_events = audio.subscribe();
        let mut app = Self {
            state: AppState::default(),
            commands,
            audio,
            audio_events,
            media,
            current_url: None,
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
            IpcCommand::Play { .. } => "error каталог станций ещё не подключён (Этап 3)".to_owned(),
            IpcCommand::Stop => match self.runtime.block_on(self.audio.stop()) {
                Ok(()) => {
                    self.current_url = None;
                    "ok".to_owned()
                }
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
        format_status(
            &self.audio.state(),
            self.audio.volume(),
            self.state.window_visible,
        )
    }

    fn process_audio_events(&mut self) {
        loop {
            match self.audio_events.try_recv() {
                Ok(AudioEvent::Started { url }) => self.current_url = Some(url),
                Ok(AudioEvent::Stopped | AudioEvent::Error { .. }) => self.current_url = None,
                Ok(AudioEvent::Buffering | AudioEvent::Reconnecting { .. }) => {}
                Ok(AudioEvent::EndOfFile) => {}
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
                } else {
                    self.current_url = None;
                }
            }
            MediaCommand::SetVolume(volume) => {
                if let Err(err) = self.runtime.block_on(self.audio.set_volume(volume)) {
                    warn!(error = %err, "не удалось изменить громкость из MPRIS");
                }
            }
            MediaCommand::Play | MediaCommand::PlayPause => {
                if let Some(url) = self.current_url.clone()
                    && matches!(
                        self.audio.state(),
                        PlaybackState::Stopped | PlaybackState::Error(_)
                    )
                    && let Err(err) = self.runtime.block_on(self.audio.play(&url))
                {
                    warn!(error = %err, "не удалось возобновить воспроизведение из MPRIS");
                }
            }
            MediaCommand::Pause => {
                debug!("пауза не поддерживается для живого радио");
            }
            MediaCommand::Next | MediaCommand::Previous => {
                debug!("переключение станций из MPRIS появится вместе с Radio Browser");
            }
        }
    }

    fn sync_media(&mut self) {
        let state = self.audio.state();
        let playing = matches!(
            state,
            PlaybackState::Playing | PlaybackState::Buffering | PlaybackState::Reconnecting
        );
        let title = if playing {
            self.current_url
                .clone()
                .unwrap_or_else(|| WINDOW_TITLE.to_owned())
        } else {
            String::new()
        };
        let info = PlaybackInfo {
            title,
            artist: None,
            art_url: None,
            url: self.current_url.clone().unwrap_or_default(),
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

                let height = (ui.available_height() - 56.0).max(180.0);
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

fn format_status(state: &PlaybackState, volume: f64, window_visible: bool) -> String {
    let window = if window_visible { "visible" } else { "hidden" };
    format!(
        "status {}\nvolume {volume:.2}\nwindow {window}",
        state.as_str()
    )
}

fn playback_label(state: &PlaybackState) -> String {
    match state {
        PlaybackState::Stopped => "остановлено".to_owned(),
        PlaybackState::Playing => "играет".to_owned(),
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

    #[test]
    fn status_reports_playback_volume_and_window() {
        assert_eq!(
            format_status(&PlaybackState::Reconnecting, 0.8, true),
            "status reconnecting\nvolume 0.80\nwindow visible"
        );
    }

    #[test]
    fn error_state_is_reported_as_error() {
        assert_eq!(
            format_status(&PlaybackState::Error("нет сети".to_owned()), 0.5, false),
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
}
