//! Трей как третий источник команд и как витрина состояния (ADR-0020, п. 3, 7).
//!
//! Команды трея входят в тот же dispatch, что IPC и MPRIS, поэтому у модуля нет
//! ни своего протокола, ни своей бизнес-логики: [`TrayCommand`] превращается в
//! вызовы `Session`, а наружу публикуется [`TraySnapshot`].

use crate::audio::PlaybackState;
use crate::tray::{TrayCommand, TraySnapshot, TrayStation, VOLUME_STEP};
use crate::ui::single_line;

use super::notice::NoticeKey;

impl super::Session {
    /// Снимок состояния для трея (SNI на Linux, строка меню на macOS).
    /// Публикуется только при изменении: на Linux `update` блокирует поток трея
    /// до dbus-ответа, гонять его на каждый кадр нельзя (ADR-0020, п. 4).
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
                    self.notify_error_detail(NoticeKey::ThemeNotSaved, err);
                }
            }
            TrayCommand::ToggleFavorite => {
                if let Some(station) = self.active_station().cloned() {
                    let _ = self.toggle_favorite(&station);
                }
            }
            TrayCommand::OpenHomepage => self.open_station_homepage(),
            TrayCommand::Settings => self.open_settings(),
            TrayCommand::Quit => self.quit(),
        }
    }

    /// Домашняя страница активной станции — во внешнем браузере. Ссылки других
    /// схем не запускаются: в меню попадают только `http(s)` (ADR-0020,
    /// ревизия 0.16). Процесс запускается без shell, URL — единственный аргумент.
    fn open_station_homepage(&mut self) {
        let Some(homepage) = self
            .active_station()
            .and_then(|station| station.homepage.clone())
        else {
            return;
        };
        if !is_web_url(&homepage) {
            tracing::warn!(
                homepage,
                "домашняя страница станции не является http(s)-ссылкой"
            );
            return;
        }
        let program = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        match std::process::Command::new(program).arg(&homepage).spawn() {
            Ok(mut child) => {
                // xdg-open/open завершаются быстро; без wait() остаётся зомби.
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
            }
            Err(err) => {
                tracing::warn!(error = %err, "не удалось открыть домашнюю страницу станции");
            }
        }
    }

    /// Громкость двигается относительно текущего значения и упирается в границы
    /// диапазона: `AudioEngine` вне `0.0..=1.0` отказывает, а трей не должен
    /// показывать ошибку на крайних значениях. Тот же шаг используют IPC-команды
    /// `volume-up`/`volume-down` (ADR-0037).
    pub(crate) fn nudge_volume(&mut self, delta: f64) {
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
        let active = self.active_station();
        let station = active.map(|station| TrayStation {
            name: single_line(&station.name),
            country: single_line(&station.country),
            tags: single_line(&station.tags),
            bitrate: station.bitrate,
            homepage: station
                .homepage
                .as_deref()
                .map(single_line)
                .filter(|url| is_web_url(url)),
        });
        let favorite = active.is_some_and(|station| self.library.is_favorite(&station.stationuuid));
        TraySnapshot {
            window_visible: self.window.visible,
            playing,
            station,
            favorite,
            volume_percent: volume_percent(self.audio.volume()),
            theme: self.settings.settings().theme,
            language: self.language,
        }
    }
}

fn volume_percent(volume: f64) -> u8 {
    (volume.clamp(0.0, 1.0) * 100.0).round() as u8
}

/// Разрешена только веб-ссылка с непустым хостом: меню не должно уметь
/// запускать `file:`, `javascript:` или URL с userinfo.
fn is_web_url(url: &str) -> bool {
    let trimmed = url.trim();
    if trimmed.len() > 2048
        || trimmed != url
        || trimmed
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace())
    {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"));
    let Some(rest) = rest else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    !authority.is_empty() && !authority.contains('@')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn homepage_accepts_only_web_urls() {
        for url in [
            "http://wfmu.org",
            "https://wfmu.org/shows?day=1#top",
            "HTTPS://example.com/stream",
        ] {
            assert!(is_web_url(url), "{url}");
        }
        for url in [
            "",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "ftp://example.com",
            "https://",
            "http:///path",
            " https://example.com",
            "https://example.com/ bad",
            "https://user@example.com",
            "https://example.com/\nnext",
        ] {
            assert!(!is_web_url(url), "{url}");
        }
    }

    #[test]
    fn volume_percent_rounds_and_clamps() {
        assert_eq!(volume_percent(0.0), 0);
        assert_eq!(volume_percent(0.704), 70);
        assert_eq!(volume_percent(0.705), 71);
        assert_eq!(volume_percent(1.0), 100);
        assert_eq!(volume_percent(1.5), 100);
        assert_eq!(volume_percent(-0.2), 0);
    }
}
