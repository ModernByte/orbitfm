//! Системные медиа-контролы на базе playwire (MPRIS v2 на Linux).

#[cfg(windows)]
mod host_window;

use std::sync::Arc;
use std::time::Duration;

use playwire::{
    Capabilities, Event as PlatformEvent, MediaControls as PlatformControls, PlaybackState,
    PlayerConfig, Repeat, Track,
};
use tokio::sync::mpsc;

const PLAYER_IDENTITY: &str = "OrbitFM";
const DESKTOP_ENTRY: &str = "org.modernbyte.OrbitFM";
const TRACK_ID_PREFIX: &str = "/org/modernbyte/OrbitFM/track";

/// Пробуждение GUI-цикла после команды от системного медиа-виджета.
pub type WakeFn = Arc<dyn Fn() + Send + Sync>;

/// Информация, отображаемая системой.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackInfo {
    pub title: String,
    pub artist: Option<String>,
    pub art_url: Option<String>,
    pub url: String,
    pub playing: bool,
    pub volume: f64,
    pub can_go_next: bool,
    pub can_go_previous: bool,
}

/// Команды, приходящие от системы.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MediaCommand {
    Play,
    Pause,
    PlayPause,
    Stop,
    Next,
    Previous,
    SetVolume(f64),
}

/// Ошибки медиа-контролов.
#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("не удалось инициализировать медиа-контролы: {0}")]
    Init(String),
    #[error("не удалось обновить метаданные: {0}")]
    Publish(String),
    #[error("платформа не поддерживается")]
    UnsupportedPlatform,
}

/// Обёртка над системными медиа-контролами.
pub struct MediaControls {
    controls: PlatformControls,
    // Поле объявлено после `controls`, чтобы SMTC отвязывался от окна раньше,
    // чем уничтожается сам HWND (ADR-0024).
    #[cfg(windows)]
    _smtc_window: host_window::HostWindow,
    events: mpsc::UnboundedReceiver<MediaCommand>,
    last_published: Option<PlaybackInfo>,
}

impl MediaControls {
    /// Регистрирует MPRIS-имя и начинает принимать команды системы.
    pub fn new(wake: WakeFn) -> Result<Self, MediaError> {
        let (event_tx, events) = mpsc::unbounded_channel();
        #[cfg(windows)]
        let smtc_window = host_window::HostWindow::new().map_err(MediaError::Init)?;
        let config = PlayerConfig::new(PLAYER_IDENTITY)
            .desktop_entry(DESKTOP_ENTRY)
            .track_id_prefix(TRACK_ID_PREFIX);
        #[cfg(windows)]
        let config = config.hwnd(smtc_window.hwnd());

        let controls = PlatformControls::new(config, move |event| {
            if let Some(command) = map_event(event) {
                if event_tx.send(command).is_err() {
                    tracing::trace!("приёмник медиа-команд отключён");
                }
                wake();
            }
        })
        .map_err(init_error)?;

        Ok(Self {
            controls,
            #[cfg(windows)]
            _smtc_window: smtc_window,
            events,
            last_published: None,
        })
    }

    /// Обновляет состояние, отображаемое системой.
    pub fn set_playback(&mut self, info: &PlaybackInfo) -> Result<(), MediaError> {
        if self.last_published.as_ref() == Some(info) {
            return Ok(());
        }

        let track = if info.title.is_empty() {
            None
        } else {
            Some(Track {
                id: if info.url.is_empty() {
                    info.title.clone()
                } else {
                    info.url.clone()
                },
                title: info.title.clone(),
                artists: info.artist.iter().cloned().collect(),
                album: String::new(),
                artwork_url: info.art_url.clone().unwrap_or_default(),
                url: info.url.clone(),
            })
        };

        let state = PlaybackState {
            track,
            playing: info.playing,
            position: Duration::ZERO,
            duration: None,
            volume: info.volume,
            repeat: Repeat::Off,
            shuffle: false,
            capabilities: capabilities(info),
        };

        self.controls.set_state(&state).map_err(|err| match err {
            playwire::Error::Unsupported => MediaError::UnsupportedPlatform,
            other => MediaError::Publish(other.to_string()),
        })?;
        self.last_published = Some(info.clone());
        Ok(())
    }

    /// Очередь команд от системы.
    pub fn events(&mut self) -> &mut mpsc::UnboundedReceiver<MediaCommand> {
        &mut self.events
    }
}

fn init_error(err: playwire::Error) -> MediaError {
    match err {
        playwire::Error::Unsupported => MediaError::UnsupportedPlatform,
        other => MediaError::Init(other.to_string()),
    }
}

fn capabilities(info: &PlaybackInfo) -> Capabilities {
    Capabilities {
        can_go_next: info.can_go_next,
        can_go_previous: info.can_go_previous,
        can_seek: false,
    }
}

// `PlatformEvent` — чужой enum без `Copy`: match по значению здесь читаемее,
// чем ссылки на каждом из тестовых вызовов.
#[allow(clippy::needless_pass_by_value)]
fn map_event(event: PlatformEvent) -> Option<MediaCommand> {
    match event {
        PlatformEvent::Play => Some(MediaCommand::Play),
        PlatformEvent::Pause => Some(MediaCommand::Pause),
        PlatformEvent::PlayPause => Some(MediaCommand::PlayPause),
        PlatformEvent::Stop => Some(MediaCommand::Stop),
        PlatformEvent::Next => Some(MediaCommand::Next),
        PlatformEvent::Previous => Some(MediaCommand::Previous),
        PlatformEvent::SetVolume(volume) if volume.is_finite() => {
            Some(MediaCommand::SetVolume(volume.clamp(0.0, 1.0)))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_supported_platform_events() {
        assert_eq!(map_event(PlatformEvent::Play), Some(MediaCommand::Play));
        assert_eq!(map_event(PlatformEvent::Pause), Some(MediaCommand::Pause));
        assert_eq!(
            map_event(PlatformEvent::PlayPause),
            Some(MediaCommand::PlayPause)
        );
        assert_eq!(map_event(PlatformEvent::Stop), Some(MediaCommand::Stop));
        assert_eq!(
            map_event(PlatformEvent::SetVolume(0.42)),
            Some(MediaCommand::SetVolume(0.42))
        );
        assert_eq!(
            map_event(PlatformEvent::SetVolume(1.5)),
            Some(MediaCommand::SetVolume(1.0))
        );
        assert_eq!(map_event(PlatformEvent::SetVolume(f64::NAN)), None);
    }

    #[test]
    fn advertises_skip_actions_from_playback_info() {
        let mut info = PlaybackInfo {
            title: "Station".to_owned(),
            artist: None,
            art_url: None,
            url: "https://example.com/stream".to_owned(),
            playing: true,
            volume: 0.8,
            can_go_next: false,
            can_go_previous: false,
        };
        let caps = capabilities(&info);
        assert!(!caps.can_go_next);
        assert!(!caps.can_go_previous);
        assert!(!caps.can_seek, "перемотка живого радио не поддерживается");

        info.can_go_next = true;
        info.can_go_previous = true;
        let caps = capabilities(&info);
        assert!(caps.can_go_next);
        assert!(caps.can_go_previous);
    }

    #[test]
    fn ignores_events_outside_stage_2_scope() {
        assert_eq!(map_event(PlatformEvent::Next), Some(MediaCommand::Next));
        assert_eq!(
            map_event(PlatformEvent::Previous),
            Some(MediaCommand::Previous)
        );
        assert_eq!(
            map_event(PlatformEvent::SeekBy(5.0)),
            None,
            "перемотка не поддерживается для живого радио"
        );
    }
}
