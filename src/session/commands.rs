//! Обработка IPC-команд и настроек оформления.
//!
//! Тот же `execute` обслуживает окно и простой без него: команда не знает,
//! есть ли `egui::Context`, а всё, что касается окна, выражается через
//! [`UiIntent`](super::UiIntent).

use crate::ipc::{IpcCommand, ThemeArg, WindowModeArg};
use crate::settings::{AccentColor, Language, MotionReactivity, MotionStyle, ThemeMode};
use crate::tray::VOLUME_STEP;

use super::WindowMode;
use super::notice::NoticeKey;

impl From<WindowModeArg> for WindowMode {
    fn from(arg: WindowModeArg) -> Self {
        match arg {
            WindowModeArg::Focus => Self::Focus,
            WindowModeArg::Overlay => Self::Overlay,
            WindowModeArg::Listening => Self::Listening,
            WindowModeArg::Panel => Self::Panel,
        }
    }
}

impl From<ThemeArg> for ThemeMode {
    fn from(arg: ThemeArg) -> Self {
        match arg {
            ThemeArg::System => Self::System,
            ThemeArg::Dark => Self::Dark,
            ThemeArg::Light => Self::Light,
        }
    }
}

impl super::Session {
    pub(crate) fn process_commands(&mut self) {
        loop {
            let next = self.commands.try_recv();
            let Ok(request) = next else { break };
            let response = self.execute(request.command());
            request.try_respond(response);
        }
    }

    /// Выполняет команду и возвращает строку ответа клиенту IPC.
    pub(crate) fn execute(&mut self, command: &IpcCommand) -> String {
        match command {
            IpcCommand::Toggle => {
                self.toggle();
                "ok".to_owned()
            }
            IpcCommand::Show => {
                self.show();
                "ok".to_owned()
            }
            IpcCommand::Hide => {
                self.hide();
                "ok".to_owned()
            }
            IpcCommand::Status => self.status_text(),
            IpcCommand::StatusJson => self.status_json(),
            // Подписку обслуживает IPC-сервер: в очередь команд она не попадает.
            IpcCommand::Events => "error events обслуживается IPC-сервером".to_owned(),
            IpcCommand::ModeQuery => format!("mode {}", self.window_mode().as_str()),
            IpcCommand::SetMode { mode } => {
                self.set_window_mode((*mode).into());
                "ok".to_owned()
            }
            IpcCommand::Panel => {
                self.toggle_dropdown(WindowMode::Panel);
                "ok".to_owned()
            }
            IpcCommand::Overlay => {
                self.toggle_dropdown(WindowMode::Overlay);
                "ok".to_owned()
            }
            IpcCommand::PlayPause => match self.toggle_playback() {
                Ok(()) => "ok".to_owned(),
                Err(err) => format!("error {err}"),
            },
            IpcCommand::Next => {
                self.play_relative(1);
                "ok".to_owned()
            }
            IpcCommand::Previous => {
                self.play_relative(-1);
                "ok".to_owned()
            }
            IpcCommand::Roulette => {
                self.start_roulette();
                "ok".to_owned()
            }
            IpcCommand::Theme { mode } => match self.set_theme(ThemeMode::from(*mode)) {
                Ok(()) => "ok".to_owned(),
                Err(err) => format!("error {err}"),
            },
            IpcCommand::Settings => {
                self.open_settings();
                "ok".to_owned()
            }
            IpcCommand::VolumeUp => {
                self.nudge_volume(VOLUME_STEP);
                "ok".to_owned()
            }
            IpcCommand::VolumeDown => {
                self.nudge_volume(-VOLUME_STEP);
                "ok".to_owned()
            }
            IpcCommand::Quit => {
                self.quit();
                "ok".to_owned()
            }
            IpcCommand::Play { station_uuid } => match self.resolve_station(station_uuid) {
                Ok(station) => match self.start_station(station) {
                    Ok(()) => "ok".to_owned(),
                    Err(err) => format!("error {err}"),
                },
                Err(err) => format!("error {err}"),
            },
            IpcCommand::Stop => match self.stop_playback() {
                Ok(()) => "ok".to_owned(),
                Err(err) => format!("error {err}"),
            },
            IpcCommand::Volume { level } => match self.set_volume(*level) {
                Ok(()) => "ok".to_owned(),
                Err(err) => format!("error {err}"),
            },
            IpcCommand::Raw(raw) => format!("error неизвестная команда: {raw}"),
        }
    }

    /// Тема сохраняется в Core, стиль перекрашивает UI: `configure_style` знает про egui.
    pub(crate) fn set_theme(&mut self, theme: ThemeMode) -> Result<(), String> {
        self.settings
            .set_theme(theme)
            .map_err(|err| err.to_string())
    }

    /// Язык сохраняется и разрешается сразу: UI и трей читают `self.language`,
    /// перезапуск не нужен (ADR-0030).
    pub(crate) fn set_language(&mut self, language: Language) -> Result<(), String> {
        self.settings
            .set_language(language)
            .map_err(|err| err.to_string())?;
        self.language = language.resolve();
        self.sync_tray();
        Ok(())
    }

    pub(crate) fn set_accent(&mut self, accent: AccentColor) -> Result<(), String> {
        self.settings
            .set_accent(accent)
            .map_err(|err| err.to_string())
    }

    /// Число столбцов визуализатора (ADR-0019): Core хранит настройку, модель строит UI.
    pub(crate) fn set_viz_bars(&mut self, bars: u8) -> Result<(), String> {
        self.settings
            .set_viz_bars(bars)
            .map_err(|err| err.to_string())
    }

    /// Визуализатор — настройка, которая живёт в Core: переключает и FFT-анализ,
    /// и сохранение (ADR-0031).
    pub(crate) fn set_visualizer_enabled(&mut self, enabled: bool) -> Result<(), String> {
        if let Err(err) = self.settings.set_visualizer_enabled(enabled) {
            tracing::warn!(error = %err, "не удалось сохранить видимость визуализатора");
            self.notify_error_detail(NoticeKey::VisualizerNotSaved, err.to_string());
        }
        self.visualizer_enabled = enabled;
        self.sync_analysis()
    }

    pub(crate) fn set_motion(&mut self, motion: MotionReactivity) -> Result<(), String> {
        self.settings
            .set_motion(motion)
            .map_err(|err| err.to_string())?;
        self.sync_analysis()
    }

    pub(crate) fn set_reduce_motion(&mut self, reduce: bool) -> Result<(), String> {
        self.settings
            .set_reduce_motion(reduce)
            .map_err(|err| err.to_string())?;
        self.sync_analysis()
    }

    pub(crate) fn set_motion_style(&mut self, style: MotionStyle) -> Result<(), String> {
        self.settings
            .set_motion_style(style)
            .map_err(|err| err.to_string())
    }

    pub(crate) fn set_allow_flips(&mut self, allow: bool) -> Result<(), String> {
        self.settings
            .set_allow_flips(allow)
            .map_err(|err| err.to_string())
    }

    /// Переключает анализ под текущую видимость окна. Зовётся на границах
    /// прогона UI: в простое без окна эфирные фильтры не тратят CPU.
    pub(crate) fn settle_analysis(&mut self) {
        if let Err(err) = self.sync_analysis() {
            tracing::warn!(error = %err, "не удалось переключить аудио-анализ");
        }
    }

    /// Анализ включён, если он нужен визуализатору или движению глобуса.
    fn sync_analysis(&mut self) -> Result<(), String> {
        let needed = self.analysis_needed();
        if self.audio.spectrum_enabled() == needed {
            self.spectrum_enabled = needed;
            return Ok(());
        }

        let runtime = self.runtime.clone();
        match runtime.block_on(self.audio.set_spectrum_enabled(needed)) {
            Ok(()) => {
                self.spectrum_enabled = needed;
                Ok(())
            }
            Err(err) => {
                self.spectrum_enabled = self.audio.spectrum_enabled();
                Err(err.to_string())
            }
        }
    }

    /// Видимость карточки управления глобусом: выбор пользователя переживает
    /// перезапуск, а по умолчанию карточка включена (ADR-0036).
    pub(crate) fn set_help_visible(&mut self, visible: bool) -> Result<(), String> {
        self.show_help = visible;
        self.settings
            .set_help_visible(visible)
            .map_err(|err| err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ответ неизвестной команды обязан начинаться с `error`: по этому префиксу
    /// CLI считает команду неудачной.
    #[test]
    fn unknown_command_reply_is_marked_as_error() {
        let raw = IpcCommand::Raw("включите бегемота".to_owned());
        assert!(raw.to_wire().contains("бегемота"));
    }

    #[test]
    fn wire_round_trip_keeps_command_identity() {
        for command in [
            IpcCommand::Toggle,
            IpcCommand::Show,
            IpcCommand::Hide,
            IpcCommand::Status,
            IpcCommand::StatusJson,
            IpcCommand::Quit,
            IpcCommand::Stop,
            IpcCommand::ModeQuery,
            IpcCommand::Panel,
            IpcCommand::Overlay,
            IpcCommand::PlayPause,
            IpcCommand::Next,
            IpcCommand::Previous,
            IpcCommand::Roulette,
            IpcCommand::Settings,
            IpcCommand::VolumeUp,
            IpcCommand::VolumeDown,
            IpcCommand::Theme {
                mode: ThemeArg::Light,
            },
            IpcCommand::SetMode {
                mode: WindowModeArg::Overlay,
            },
        ] {
            let wire = command.to_wire();
            assert_eq!(IpcCommand::parse(&wire).ok(), Some(command), "{wire}");
        }
    }

    /// Wire-аргумент превращается в доменный режим, а не наоборот.
    #[test]
    fn mode_argument_maps_to_domain_mode() {
        assert_eq!(WindowMode::from(WindowModeArg::Focus), WindowMode::Focus);
        assert_eq!(
            WindowMode::from(WindowModeArg::Listening),
            WindowMode::Listening
        );
        assert_eq!(WindowMode::from(WindowModeArg::Panel), WindowMode::Panel);
    }

    #[test]
    fn theme_argument_maps_to_domain_theme() {
        assert_eq!(ThemeMode::from(ThemeArg::System), ThemeMode::System);
        assert_eq!(ThemeMode::from(ThemeArg::Dark), ThemeMode::Dark);
        assert_eq!(ThemeMode::from(ThemeArg::Light), ThemeMode::Light);
    }
}
