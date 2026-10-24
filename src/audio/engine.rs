use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use libmpv2::Mpv;
use tokio::sync::{broadcast, mpsc, oneshot};
use url::Url;

use super::{AudioError, AudioEvent, PlaybackState, SpectrumFrame};
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
    SetPaused {
        paused: bool,
        reply: oneshot::Sender<CommandResult>,
    },
    SetVolume {
        volume: f64,
        reply: oneshot::Sender<CommandResult>,
    },
    SetSpectrum {
        enabled: bool,
        reply: oneshot::Sender<CommandResult>,
    },
    Shutdown,
}

pub(super) struct Shared {
    state: Mutex<PlaybackState>,
    volume: Mutex<f64>,
    events: broadcast::Sender<AudioEvent>,
    spectrum_enabled: AtomicBool,
    spectrum: Mutex<Option<SpectrumFrame>>,
}

impl Shared {
    fn new() -> Self {
        let (events, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self {
            state: Mutex::new(PlaybackState::Stopped),
            volume: Mutex::new(DEFAULT_VOLUME),
            events,
            spectrum_enabled: AtomicBool::new(true),
            spectrum: Mutex::new(None),
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

    pub(super) fn spectrum_enabled(&self) -> bool {
        self.spectrum_enabled.load(Ordering::Relaxed)
    }

    pub(super) fn set_spectrum_enabled(&self, enabled: bool) {
        self.spectrum_enabled.store(enabled, Ordering::Relaxed);
    }

    pub(super) fn spectrum(&self) -> Option<SpectrumFrame> {
        *lock(&self.spectrum)
    }

    pub(super) fn set_spectrum(&self, frame: Option<SpectrumFrame>) {
        *lock(&self.spectrum) = frame;
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

    /// Приостановить или возобновить воспроизведение.
    pub async fn set_paused(&mut self, paused: bool) -> Result<(), AudioError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(AudioCommand::SetPaused { paused, reply })
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

    /// Включить или выключить экспериментальный FFT-визуализатор.
    ///
    /// Фильтры подключаются к работающему mpv и не меняют аудиотракт.
    pub async fn set_spectrum_enabled(&mut self, enabled: bool) -> Result<(), AudioError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(AudioCommand::SetSpectrum { enabled, reply })
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

    /// Включён ли FFT-визуализатор.
    pub fn spectrum_enabled(&self) -> bool {
        self.shared.spectrum_enabled()
    }

    /// Последний кадр FFT-метрик, пока визуализатор получает данные.
    pub fn spectrum(&self) -> Option<SpectrumFrame> {
        self.shared.spectrum()
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

    #[test]
    fn spectrum_is_enabled_by_default() {
        assert!(Shared::new().spectrum_enabled());
    }

    #[tokio::test]
    #[ignore = "требует сеть и аудиоустройство; задайте ORBITFM_TEST_STREAM_URL"]
    async fn plays_and_stops_live_stream() {
        let url =
            std::env::var("ORBITFM_TEST_STREAM_URL").expect("задайте ORBITFM_TEST_STREAM_URL");
        let mut engine = AudioEngine::new().expect("не удалось создать аудио-ядро");
        let mut events = engine.subscribe();

        engine.play(&url).await.expect("не удалось запустить поток");

        let started = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                match events.recv().await {
                    Ok(AudioEvent::Started { .. }) => return,
                    Ok(AudioEvent::Error { message }) => {
                        panic!("ошибка потока: {message}");
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => {
                        panic!("канал событий закрыт");
                    }
                }
            }
        })
        .await;
        assert!(started.is_ok(), "поток не загрузился за 20 секунд");

        let state = engine.state();
        assert!(
            matches!(state, PlaybackState::Playing | PlaybackState::Buffering),
            "неожиданное состояние: {state:?}"
        );

        engine
            .set_paused(true)
            .await
            .expect("не удалось поставить на паузу");
        assert_eq!(engine.state(), PlaybackState::Paused);
        engine
            .set_paused(false)
            .await
            .expect("не удалось возобновить воспроизведение");
        assert_eq!(engine.state(), PlaybackState::Playing);

        engine.stop().await.expect("не удалось остановить поток");
        assert_eq!(engine.state(), PlaybackState::Stopped);
    }

    #[tokio::test]
    #[ignore = "требует сеть и аудиоустройство; задайте ORBITFM_TEST_STREAM_URL"]
    async fn produces_fft_metrics_for_live_stream() {
        let url =
            std::env::var("ORBITFM_TEST_STREAM_URL").expect("задайте ORBITFM_TEST_STREAM_URL");
        let mut engine = AudioEngine::new().expect("не удалось создать аудио-ядро");
        engine
            .set_volume(0.0)
            .await
            .expect("не удалось выключить звук");

        engine.play(&url).await.expect("не удалось запустить поток");
        engine
            .set_spectrum_enabled(true)
            .await
            .expect("не удалось включить визуализатор");

        let frame = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if let Some(frame) = engine.spectrum() {
                    break frame;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("FFT-метрики не появились за 20 секунд");

        assert!(frame.centroid_hz > 0.0, "центроид: {frame:?}");
        assert!(frame.rms_db.is_finite(), "RMS: {frame:?}");

        let bands = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if let Some(bands) = engine.spectrum().and_then(|frame| frame.bands) {
                    break bands;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("полосы Bass/Mid/High не появились за 20 секунд");
        for db in [bands.bass_db, bands.mid_db, bands.high_db] {
            assert!(db.is_finite() && db >= -90.0, "полоса: {bands:?}");
        }

        engine
            .set_spectrum_enabled(false)
            .await
            .expect("не удалось выключить визуализатор");
        assert!(!engine.spectrum_enabled());
        assert!(engine.spectrum().is_none());
        engine.stop().await.expect("не удалось остановить поток");
    }
}
