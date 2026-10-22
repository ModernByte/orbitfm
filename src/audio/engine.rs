use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use libmpv2::Mpv;
use tokio::sync::{broadcast, mpsc, oneshot};
use url::Url;

use super::{AudioError, AudioEvent, PlaybackState};
use crate::audio::worker::AudioWorker;

pub(super) const DEFAULT_VOLUME: f64 = 0.7;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(3);
const EVENT_CHANNEL_CAPACITY: usize = 64;
const PROPERTY_PAUSED_FOR_CACHE: u64 = 1;

pub(super) type CommandResult = Result<(), AudioError>;

pub(super) enum AudioCommand {
    Play {
        url: String,
        reply: oneshot::Sender<CommandResult>,
    },
    Stop {
        reply: oneshot::Sender<CommandResult>,
    },
    SetVolume {
        volume: f64,
        reply: oneshot::Sender<CommandResult>,
    },
    Shutdown,
}

pub(super) struct Shared {
    state: Mutex<PlaybackState>,
    volume: Mutex<f64>,
    events: broadcast::Sender<AudioEvent>,
}

impl Shared {
    fn new() -> Self {
        let (events, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self {
            state: Mutex::new(PlaybackState::Stopped),
            volume: Mutex::new(DEFAULT_VOLUME),
            events,
        }
    }

    pub(super) fn state(&self) -> PlaybackState {
        lock(&self.state).clone()
    }

    pub(super) fn set_state(&self, state: PlaybackState) {
        *lock(&self.state) = state;
    }

    pub(super) fn volume(&self) -> f64 {
        *lock(&self.volume)
    }

    pub(super) fn set_volume(&self, volume: f64) {
        *lock(&self.volume) = volume;
    }

    pub(super) fn emit(&self, event: AudioEvent) {
        if let Err(err) = self.events.send(event) {
            tracing::trace!(error = %err, "нет подписчиков аудио-событий");
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Аудио-движок на базе libmpv.
///
/// Все обращения к libmpv выполняет выделенный поток; `AudioEngine` общается с ним
/// через канал команд и публикует события в broadcast-канал.
pub struct AudioEngine {
    commands: mpsc::UnboundedSender<AudioCommand>,
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl AudioEngine {
    /// Создаёт libmpv-контекст и запускает аудио-поток.
    pub fn new() -> Result<Self, AudioError> {
        ensure_c_numeric_locale()?;

        let mpv = create_mpv()?;
        let shared = Arc::new(Shared::new());
        let (commands, command_rx) = mpsc::unbounded_channel();
        let worker_shared = Arc::clone(&shared);

        let worker = thread::Builder::new()
            .name("orbitfm-audio".to_owned())
            .spawn(move || AudioWorker::new(mpv, worker_shared, command_rx).run())
            .map_err(|err| AudioError::Init(format!("не удалось запустить аудио-поток: {err}")))?;

        Ok(Self {
            commands,
            shared,
            worker: Some(worker),
        })
    }

    /// Начать воспроизведение URL (предпочтительно `url_resolved`).
    pub async fn play(&mut self, url: &str) -> Result<(), AudioError> {
        validate_url(url)?;
        let (reply, response) = oneshot::channel();
        self.commands
            .send(AudioCommand::Play {
                url: url.to_owned(),
                reply,
            })
            .map_err(|_| AudioError::Internal("аудио-поток остановлен".to_owned()))?;
        wait_for_reply(response).await
    }

    /// Остановить воспроизведение.
    pub async fn stop(&mut self) -> Result<(), AudioError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(AudioCommand::Stop { reply })
            .map_err(|_| AudioError::Internal("аудио-поток остановлен".to_owned()))?;
        wait_for_reply(response).await
    }

    /// Установить громкость в диапазоне `0.0..=1.0`.
    pub async fn set_volume(&mut self, volume: f64) -> Result<(), AudioError> {
        validate_volume(volume)?;
        let (reply, response) = oneshot::channel();
        self.commands
            .send(AudioCommand::SetVolume { volume, reply })
            .map_err(|_| AudioError::Internal("аудио-поток остановлен".to_owned()))?;
        wait_for_reply(response).await
    }

    /// Текущее состояние воспроизведения.
    pub fn state(&self) -> PlaybackState {
        self.shared.state()
    }

    /// Текущая громкость в диапазоне `0.0..=1.0`.
    pub fn volume(&self) -> f64 {
        self.shared.volume()
    }

    /// Подписка на события аудио-ядра.
    pub fn subscribe(&self) -> broadcast::Receiver<AudioEvent> {
        self.shared.events.subscribe()
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            if self.commands.send(AudioCommand::Shutdown).is_err() {
                tracing::debug!("аудио-поток завершился до shutdown");
            }
            if worker.join().is_err() {
                tracing::warn!("аудио-поток завершился с паникой");
            }
        }
    }
}

fn create_mpv() -> Result<Mpv, AudioError> {
    let mpv = Mpv::with_initializer(|init| {
        init.set_option("config", "no")?;
        init.set_option("terminal", "no")?;
        init.set_option("idle", "yes")?;
        init.set_option("vid", "no")?;
        init.set_option("network-timeout", "10")?;
        Ok(())
    })
    .map_err(|err| AudioError::Init(err.to_string()))?;

    mpv.set_property("volume", DEFAULT_VOLUME * 100.0)
        .map_err(|err| AudioError::Init(format!("не удалось задать громкость: {err}")))?;
    mpv.observe_property(
        "paused-for-cache",
        libmpv2::Format::Flag,
        PROPERTY_PAUSED_FOR_CACHE,
    )
    .map_err(|err| AudioError::Init(format!("не удалось отслеживать буферизацию: {err}")))?;

    for event in [
        libmpv2::events::mpv_event_id::EndFile,
        libmpv2::events::mpv_event_id::FileLoaded,
        libmpv2::events::mpv_event_id::StartFile,
        libmpv2::events::mpv_event_id::Shutdown,
    ] {
        mpv.enable_event(event)
            .map_err(|err| AudioError::Init(format!("не удалось включить событие mpv: {err}")))?;
    }

    Ok(mpv)
}

fn ensure_c_numeric_locale() -> Result<(), AudioError> {
    // SAFETY: строка `c"C"` статична и NUL-терминирована, а setlocale не сохраняет
    // переданный указатель. Вызов выполняется до запуска аудио-потока.
    let result = unsafe { libc::setlocale(libc::LC_NUMERIC, c"C".as_ptr()) };
    if result.is_null() {
        return Err(AudioError::Init(
            "не удалось установить LC_NUMERIC=C".to_owned(),
        ));
    }
    Ok(())
}

async fn wait_for_reply(response: oneshot::Receiver<CommandResult>) -> Result<(), AudioError> {
    match tokio::time::timeout(COMMAND_TIMEOUT, response).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(AudioError::Internal(
            "аудио-поток не прислал ответ".to_owned(),
        )),
        Err(_) => Err(AudioError::Internal(
            "аудио-поток не ответил вовремя".to_owned(),
        )),
    }
}

fn validate_url(url: &str) -> Result<(), AudioError> {
    let parsed = Url::parse(url).map_err(|_| AudioError::InvalidUrl)?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(AudioError::InvalidUrl);
    }
    Ok(())
}

fn validate_volume(volume: f64) -> Result<(), AudioError> {
    if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
        return Err(AudioError::Playback(
            "громкость должна быть в диапазоне 0.0..=1.0".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_http_and_https_urls() {
        assert!(validate_url("https://example.com/stream.mp3").is_ok());
        assert!(validate_url("http://radio.example.org:8000/live").is_ok());
    }

    #[test]
    fn rejects_non_http_and_malformed_urls() {
        for url in [
            "",
            "not a url",
            "file:///tmp/stream.mp3",
            "ftp://example.com/stream",
            "https://",
        ] {
            assert!(matches!(validate_url(url), Err(AudioError::InvalidUrl)));
        }
    }

    #[test]
    fn validates_volume_range() {
        assert!(validate_volume(0.0).is_ok());
        assert!(validate_volume(1.0).is_ok());
        assert!(validate_volume(f64::NAN).is_err());
        assert!(validate_volume(-0.1).is_err());
        assert!(validate_volume(1.1).is_err());
    }
}
