//! Обработка IPC-команд и настроек оформления.
//!
//! Тот же `execute` обслуживает окно и простой без него: команда не знает,
//! есть ли `egui::Context`, а всё, что касается окна, выражается через
//! [`UiIntent`](super::UiIntent).

use crate::ipc::{IpcCommand, NetworkArg, SleepActionArg, ThemeArg, WindowModeArg};
use crate::settings::{
    AccentColor, Language, MotionReactivity, MotionStyle, NetworkProfile, SleepAction, ThemeMode,
};
use crate::tray::VOLUME_STEP;

use super::notice::NoticeKey;
use super::{SettingsOrigin, WindowMode};

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

impl From<NetworkArg> for NetworkProfile {
    fn from(arg: NetworkArg) -> Self {
        match arg {
            NetworkArg::Unlimited => Self::Unlimited,
            NetworkArg::Metered => Self::Metered,
            NetworkArg::OfflineFirst => Self::OfflineFirst,
        }
    }
}

impl From<SleepActionArg> for SleepAction {
    fn from(arg: SleepActionArg) -> Self {
        match arg {
            SleepActionArg::Nothing => Self::Nothing,
            SleepActionArg::Suspend => Self::Suspend,
            SleepActionArg::PowerOff => Self::PowerOff,
        }
    }
}

impl super::Session {
    pub(crate) fn process_commands(&mut self) {
        loop {
            let next = self.commands.try_recv();
            let Ok(request) = next else { break };
            // Импорт потока отвечает после резолва: немедленный ответ закрыл бы
            // соединение раньше, чем станет известен результат (ADR-0040).
            if matches!(request.command(), IpcCommand::PlayUrl { .. }) {
                self.begin_play_url(request);
                continue;
            }
            // Незнакомый `play <uuid>` тоже отвечает после поиска в каталоге,
            // но кадр UI при этом не ждёт сеть (ADR-0063).
            if matches!(request.command(), IpcCommand::Play { .. }) {
                self.begin_play(request);
                continue;
            }
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
            IpcCommand::Network { profile } => {
                match self.set_network_profile(NetworkProfile::from(*profile)) {
                    Ok(()) => format!("ok network {}", profile.as_str()),
                    Err(err) => format!("error {err}"),
                }
            }
            IpcCommand::Settings => {
                // Если вьюпорт есть — мы в UI-пассе ( Settings станут
                // immediate-потомком ), иначе — припаркованы (откроется
                // отдельное окно настроек).
                if self.window.viewport {
                    self.open_settings(SettingsOrigin::Desk);
                } else {
                    self.open_settings(SettingsOrigin::Parked);
                }
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
            // `play` обслуживается отложенно: ответ приходит после поиска
            // станции в каталоге (ADR-0063).
            IpcCommand::Play { .. } => "error play обслуживается отложенно".to_owned(),
            IpcCommand::Stop => match self.stop_playback() {
                Ok(()) => "ok".to_owned(),
                Err(err) => format!("error {err}"),
            },
            IpcCommand::Volume { level } => match self.set_volume(*level) {
                Ok(()) => "ok".to_owned(),
                Err(err) => format!("error {err}"),
            },
            IpcCommand::Search { query } => {
                let generation = self.start_companion_search(query);
                format!("search {generation}")
            }
            IpcCommand::Library => self.library_json(),
            IpcCommand::Favorite { station_uuid } => {
                match self.favorite_station(station_uuid.as_deref()) {
                    Ok(favorite) => format!("favorite {favorite}"),
                    Err(err) => format!("error {err}"),
                }
            }
            IpcCommand::PlayResult { station_uuid } => {
                match self.play_companion_result(station_uuid) {
                    Ok(()) => "ok".to_owned(),
                    Err(err) => format!("error {err}"),
                }
            }
            IpcCommand::PlayLibrary { station_uuid } => {
                match self.play_library_station(station_uuid) {
                    Ok(()) => "ok".to_owned(),
                    Err(err) => format!("error {err}"),
                }
            }
            // `play-url` перехватывается в `process_commands` и парковке:
            // ответ приходит после сетевого резолва.
            IpcCommand::PlayUrl { .. } => "error play-url обслуживается отложенно".to_owned(),
            IpcCommand::SetSleep { minutes } => match self.set_sleep(*minutes) {
                Ok(()) => format!("ok sleep {minutes}"),
                Err(err) => format!("error {err}"),
            },
            IpcCommand::ClearSleep => {
                self.clear_sleep();
                "ok".to_owned()
            }
            // Ритуал сна (ADR-0054…0058): подкоманды пишут настройку и сразу
            // применяются к активному таймеру.
            IpcCommand::SetSleepAction { action } => {
                let action = SleepAction::from(*action);
                match self.set_sleep_action(action) {
                    Ok(()) => format!("ok sleep action {}", action.as_str()),
                    Err(err) => format!("error {err}"),
                }
            }
            IpcCommand::SetSleepFade { seconds } => match self.set_sleep_fade_seconds(*seconds) {
                Ok(applied) => format!("ok sleep fade {applied}"),
                Err(err) => format!("error {err}"),
            },
            IpcCommand::SetSleepInhibit { enabled } => {
                match self.set_sleep_prevent_sleep(*enabled) {
                    Ok(()) => format!("ok sleep inhibit {}", if *enabled { "on" } else { "off" }),
                    Err(err) => format!("error {err}"),
                }
            }
            IpcCommand::SetSleepWarning { enabled } => match self.set_sleep_warning(*enabled) {
                Ok(()) => format!("ok sleep warning {}", if *enabled { "on" } else { "off" }),
                Err(err) => format!("error {err}"),
            },
            IpcCommand::LibraryExport { path } => {
                match self.library.export_to(std::path::Path::new(path)) {
                    Ok(favorites) => format!("ok exported {favorites}"),
                    Err(err) => format!("error {err}"),
                }
            }
            IpcCommand::LibraryImport { path } => {
                match self.library.import_from(std::path::Path::new(path)) {
                    Ok(summary) => format!(
                        "ok imported {} favorites, {} history, {} skipped",
                        summary.favorites_added, summary.history_added, summary.skipped
                    ),
                    Err(err) => format!("error {err}"),
                }
            }
            IpcCommand::Stats => {
                let enabled = self.settings.settings().stats_enabled;
                super::stats::format_stats_text(&self.stats_snapshot(), enabled)
            }
            IpcCommand::StatsJson => {
                let enabled = self.settings.settings().stats_enabled;
                super::stats::build_stats_payload(&self.stats_snapshot(), enabled, self.language)
            }
            IpcCommand::ClearStats => match self.clear_stats() {
                Ok(()) => "ok stats cleared".to_owned(),
                Err(err) => format!("error {err}"),
            },
            // Алиасы URL (ADR-0050 § 7): полный адрес отвечает только CLI,
            // в UI и статусе показывается хост-подпись.
            IpcCommand::AliasQuery { station_uuid } => match self.library.alias_url(station_uuid) {
                Some(url) => format!("alias {station_uuid} {url}"),
                None => "alias none".to_owned(),
            },
            IpcCommand::SetAlias { station_uuid, url } => {
                match self.library.set_alias(station_uuid, url) {
                    Ok(label) => format!("ok alias {label}"),
                    Err(err) => format!("error {err}"),
                }
            }
            IpcCommand::RemoveAlias { station_uuid } => {
                match self.library.remove_alias(station_uuid) {
                    Ok(()) => "ok alias removed".to_owned(),
                    Err(err) => format!("error {err}"),
                }
            }
            IpcCommand::AliasList => {
                let aliases = self.library.aliases();
                if aliases.is_empty() {
                    "alias none".to_owned()
                } else {
                    aliases
                        .iter()
                        .map(|alias| format!("alias {} {}", alias.stationuuid, alias.url))
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            }
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

    /// Opt-in HTTP-обложек (ADR-0025, п. 11): Core только сохраняет настройку,
    /// политику фетчера переключает presentation-слой.
    pub(crate) fn set_artwork_allow_http(&mut self, allow: bool) -> Result<(), String> {
        self.settings
            .set_artwork_allow_http(allow)
            .map_err(|err| err.to_string())
    }

    /// Кольцевой переход очереди после последней станции (ADR-0046).
    pub(crate) fn set_queue_wrap(&mut self, wrap: bool) -> Result<(), String> {
        self.settings
            .set_queue_wrap(wrap)
            .map_err(|err| err.to_string())
    }

    /// Профиль сети (ADR-0048): сохранение и отмена фонового мира при
    /// переходе в offline-first. Favicon-кэш переключает presentation-слой.
    pub(crate) fn set_network_profile(&mut self, profile: NetworkProfile) -> Result<(), String> {
        self.settings
            .set_network_profile(profile)
            .map_err(|err| err.to_string())?;
        if !profile.allows_background_network() {
            self.cancel_world_fetch();
        }
        // Потолок битрейта входит в слой карты: перезапускаем его с новым капом
        // (ADR-0048, ADR-0065).
        if self.search.context.is_active() {
            self.start_map_layer();
        }
        Ok(())
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
            IpcCommand::Network {
                profile: NetworkArg::Metered,
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

    #[test]
    fn network_argument_maps_to_domain_profile() {
        assert_eq!(
            NetworkProfile::from(NetworkArg::Unlimited),
            NetworkProfile::Unlimited
        );
        assert_eq!(
            NetworkProfile::from(NetworkArg::Metered),
            NetworkProfile::Metered
        );
        assert_eq!(
            NetworkProfile::from(NetworkArg::OfflineFirst),
            NetworkProfile::OfflineFirst
        );
    }
}
