//! Трей как третий источник команд и как витрина состояния (ADR-0020, п. 3, 7).
//!
//! Команды трея входят в тот же dispatch, что IPC и MPRIS, поэтому у модуля нет
//! ни своего протокола, ни своей бизнес-логики: [`TrayCommand`] превращается в
//! вызовы `Session`, а наружу публикуется [`TraySnapshot`].

use crate::audio::PlaybackState;
use crate::tray::{TrayCommand, TraySnapshot, VOLUME_STEP};
use crate::ui::single_line;

impl super::Session {
    /// Снимок состояния для SNI-хоста. Публикуется только при изменении:
    /// `update` блокирует поток трея до dbus-ответа, гонять его на каждый кадр
    /// нельзя (ADR-0020, п. 4).
    pub(crate) fn sync_tray(&mut self) {
        let snapshot = self.tray_snapshot();
        if self.tray_snapshot.as_ref() == Some(&snapshot) {
            return;
        }
        let Some(tray) = self.tray.as_mut() else {
            return;
        };
        if tray.publish(&snapshot) {
            self.tray_snapshot = Some(snapshot);
        } else {
            tracing::warn!("трей отключился, снимки больше не публикуются");
            self.tray = None;
            self.tray_snapshot = None;
        }
    }

    /// Очередь команд трея при открытом окне: `tick` вычитает её целиком.
    pub(crate) fn process_tray_commands(&mut self) {
        loop {
            let next = self
                .tray
                .as_mut()
                .and_then(|tray| tray.commands.try_recv().ok());
            let Some(command) = next else { break };
            self.execute_tray_command(command);
        }
    }

    /// Исполнение команды трея. Ответственности те же, что у IPC и MPRIS:
    /// окно — через `UiIntent`, воспроизведение — через аудио-ядро.
    pub(crate) fn execute_tray_command(&mut self, command: TrayCommand) {
        match command {
            TrayCommand::Toggle => self.toggle(),
            TrayCommand::Show => self.show(),
            TrayCommand::PlayPause => {
                if let Err(err) = self.toggle_playback() {
                    tracing::warn!(error = %err, "не удалось переключить воспроизведение из трея");
                }
            }
            TrayCommand::Stop => {
                if let Err(err) = self.stop_playback() {
                    tracing::warn!(error = %err, "не удалось остановить воспроизведение из трея");
                }
            }
            TrayCommand::Previous => self.play_relative(-1),
            TrayCommand::Next => self.play_relative(1),
            TrayCommand::Roulette => self.start_roulette(),
            TrayCommand::VolumeUp => self.nudge_volume(VOLUME_STEP),
            TrayCommand::VolumeDown => self.nudge_volume(-VOLUME_STEP),
            TrayCommand::SetTheme(theme) => {
                if let Err(err) = self.set_theme(theme) {
                    self.notify_error(format!("Theme: {err}"));
                }
            }
            TrayCommand::Quit => self.quit(),
        }
    }

    /// Громкость двигается относительно текущего значения и упирается в границы
    /// диапазона: `AudioEngine` вне `0.0..=1.0` отказывает, а трей не должен
    /// показывать ошибку на крайних значениях.
    fn nudge_volume(&mut self, delta: f64) {
        let target = (self.audio.volume() + delta).clamp(0.0, 1.0);
        if let Err(err) = self.set_volume(target) {
            tracing::warn!(error = %err, "не удалось изменить громкость из трея");
        }
    }

    fn tray_snapshot(&self) -> TraySnapshot {
        let state = self.audio.state();
        let playing = matches!(
            state,
            PlaybackState::Playing | PlaybackState::Buffering | PlaybackState::Reconnecting
        );
        let station = match state {
            PlaybackState::Stopped | PlaybackState::Error(_) => None,
            _ => self
                .current_station
                .as_ref()
                .map(|station| single_line(&station.name)),
        };
        TraySnapshot {
            window_visible: self.window.visible,
            playing,
            station,
            theme: self.settings.settings().theme,
        }
    }
}
