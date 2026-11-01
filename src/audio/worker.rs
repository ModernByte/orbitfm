use std::sync::Arc;
use std::time::{Duration, Instant};

use libmpv2::Mpv;
use libmpv2::events::{Event as MpvEvent, PropertyData};
use tokio::sync::{mpsc, oneshot};

use super::engine::{AudioCommand, CommandResult, Shared};
use super::{AudioError, AudioEvent, BandLevels, PlaybackState, SpectrumFrame};

const EVENT_POLL_INTERVAL_SECS: f64 = 0.05;
const SPECTRUM_POLL_INTERVAL: Duration = Duration::from_millis(50);
const LAYOUT_RETRY_INTERVAL: Duration = Duration::from_millis(250);
const LAYOUT_RETRY_ATTEMPTS: u8 = 8;
/// Нижняя граница полос: тише неё полоса считается подавленной (ADR-0027).
const BAND_FLOOR_DB: f32 = -90.0;
/// Шесть агрегатов живут в `@level` и не зависят от раскладки каналов.
const LEVEL_FILTER: &str = "@level:lavfi=[\
aspectralstats=measure=centroid+spread+flatness+flux+rolloff:win_size=2048,\
astats=metadata=1:reset=1]";
/// Общая часть `@spectrum`: копия сигнала, три полосы и RMS по восьми каналам.
const SPECTRUM_ANALYSIS: &str = "\
asplit=2[main][analysis];\
[analysis]highpass=f=20,acrossover=split=150 2000:order=8th[lo][mi][hi];\
[lo][mi][hi]amerge=inputs=3[bands];\
[main][bands]amerge=inputs=2,\
astats=metadata=1:reset=1:measure_perchannel=RMS_level:measure_overall=none";
const LEVEL_CENTROID: &str = "af-metadata/level/lavfi.aspectralstats.1.centroid";
const LEVEL_SPREAD: &str = "af-metadata/level/lavfi.aspectralstats.1.spread";
const LEVEL_ROLLOFF: &str = "af-metadata/level/lavfi.aspectralstats.1.rolloff";
const LEVEL_FLATNESS: &str = "af-metadata/level/lavfi.aspectralstats.1.flatness";
const LEVEL_FLUX: &str = "af-metadata/level/lavfi.aspectralstats.1.flux";
const LEVEL_RMS: &str = "af-metadata/level/lavfi.astats.Overall.RMS_level";
const BAND_BASS_LEFT: &str = "af-metadata/spectrum/lavfi.astats.3.RMS_level";
const BAND_BASS_RIGHT: &str = "af-metadata/spectrum/lavfi.astats.4.RMS_level";
const BAND_MID_LEFT: &str = "af-metadata/spectrum/lavfi.astats.5.RMS_level";
const BAND_MID_RIGHT: &str = "af-metadata/spectrum/lavfi.astats.6.RMS_level";
const BAND_HIGH_LEFT: &str = "af-metadata/spectrum/lavfi.astats.7.RMS_level";
const BAND_HIGH_RIGHT: &str = "af-metadata/spectrum/lavfi.astats.8.RMS_level";
const END_FILE_EOF: libmpv2::EndFileReason = libmpv2::mpv_end_file_reason::Eof;
const END_FILE_ERROR: libmpv2::EndFileReason = libmpv2::mpv_end_file_reason::Error;
const END_FILE_STOP: libmpv2::EndFileReason = libmpv2::mpv_end_file_reason::Stop;
const END_FILE_QUIT: libmpv2::EndFileReason = libmpv2::mpv_end_file_reason::Quit;
const END_FILE_REDIRECT: libmpv2::EndFileReason = libmpv2::mpv_end_file_reason::Redirect;
const DEFAULT_RECONNECT_ATTEMPTS: u32 = 5;
const INITIAL_RECONNECT_DELAY: Duration = Duration::from_secs(1);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(16);
/// Экономный профиль буфера (ADR-0043 §3): больше упреждающего чтения, чтобы
/// короткие провалы не превращались в реконнект. Это заметно выше умолчаний
/// libmpv; URL и качество потока при этом не меняются.
const ECONOMY_DEMUXER_MAX_BYTES: i64 = 256 * 1024 * 1024;
const ECONOMY_CACHE_SECS: f64 = 60.0;

/// Раскладка, для которой определён граф полос (ADR-0027).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChannelLayout {
    Mono,
    Stereo,
    Unsupported,
}

impl ChannelLayout {
    fn from_count(count: i64) -> Self {
        match count {
            1 => Self::Mono,
            2 => Self::Stereo,
            _ => Self::Unsupported,
        }
    }
}

/// Отложенная попытка узнать раскладку: mpv сообщает `channel-count`
/// уже после `FileLoaded`, но до первого аудиокадра.
#[derive(Debug, Clone, Copy)]
struct LayoutRetry {
    at: Instant,
    attempt: u8,
}

/// Умолчания libmpv, снятые до первого изменения профиля: возврат в обычный
/// режим обязан восстановить именно их, а не зашитые в коде числа.
#[derive(Debug, Clone, Copy, Default)]
struct CacheDefaults {
    demuxer_max_bytes: Option<i64>,
    cache_secs: Option<f64>,
}

impl CacheDefaults {
    fn read(mpv: &Mpv) -> Self {
        Self {
            demuxer_max_bytes: mpv.get_property::<i64>("demuxer-max-bytes").ok(),
            cache_secs: mpv.get_property::<f64>("cache-secs").ok(),
        }
    }
}

pub(super) struct AudioWorker {
    mpv: Mpv,
    shared: Arc<Shared>,
    commands: mpsc::UnboundedReceiver<AudioCommand>,
    active_url: Option<String>,
    tracker: ReconnectTracker,
    last_spectrum_poll: Option<Instant>,
    layout_retry: Option<LayoutRetry>,
    /// Граф полос ещё можно устанавливать; `false` после отказа фильтра.
    bands_allowed: bool,
    /// `@spectrum` реально стоит в текущей цепочке.
    bands_active: bool,
    applied_af: Option<String>,
    cache_defaults: CacheDefaults,
    /// Включён ли экономный профиль буфера (ADR-0043 §3).
    economy_cache: bool,
}

impl AudioWorker {
    pub(super) fn new(
        mpv: Mpv,
        shared: Arc<Shared>,
        commands: mpsc::UnboundedReceiver<AudioCommand>,
    ) -> Self {
        Self {
            cache_defaults: CacheDefaults::read(&mpv),
            mpv,
            shared,
            commands,
            active_url: None,
            tracker: ReconnectTracker::default(),
            last_spectrum_poll: None,
            layout_retry: None,
            bands_allowed: true,
            bands_active: false,
            applied_af: None,
            economy_cache: false,
        }
    }

    pub(super) fn run(mut self) {
        if self.shared.spectrum_enabled()
            && let Err(err) = self.apply_spectrum(true)
        {
            tracing::warn!(error = %err, "не удалось включить спектр-фильтр при старте");
        }

        loop {
            loop {
                match self.commands.try_recv() {
                    Ok(AudioCommand::Shutdown) | Err(mpsc::error::TryRecvError::Disconnected) => {
                        return;
                    }
                    Ok(command) => self.handle_command(command),
                    Err(mpsc::error::TryRecvError::Empty) => break,
                }
            }

            if self.tracker.take_due(Instant::now()).is_some() {
                self.retry();
            }

            if self.layout_retry_due(Instant::now()) {
                self.retry_layout();
            }

            if let Some(event) = self.next_event() {
                self.handle_event(event);
            }

            if self.shared.spectrum_enabled() && self.spectrum_poll_due() {
                self.poll_spectrum();
            }
        }
    }

    fn next_event(&self) -> Option<WorkerEvent> {
        match self.mpv.wait_event(EVENT_POLL_INTERVAL_SECS) {
            Some(Ok(MpvEvent::FileLoaded)) => Some(WorkerEvent::FileLoaded),
            Some(Ok(MpvEvent::EndFile(reason))) => Some(WorkerEvent::EndFile(reason)),
            Some(Ok(MpvEvent::PropertyChange {
                name: "paused-for-cache",
                change: PropertyData::Flag(buffering),
                ..
            })) => Some(WorkerEvent::Buffering(buffering)),
            Some(Ok(MpvEvent::Shutdown)) => Some(WorkerEvent::Shutdown),
            Some(Err(err)) => Some(WorkerEvent::Failure(format!("поток прерван: {err}"))),
            // Прочие события mpv и таймаут `wait_event` — это «ничего не пришло».
            Some(Ok(_)) | None => None,
        }
    }

    fn handle_command(&mut self, command: AudioCommand) {
        match command {
            AudioCommand::Play { url, reply } => {
                let result = self.play_url(&url);
                respond(reply, result);
            }
            AudioCommand::Stop { reply } => {
                let result = self.stop_playback();
                respond(reply, result);
            }
            AudioCommand::SetPaused { paused, reply } => {
                let result = self.apply_paused(paused);
                respond(reply, result);
            }
            AudioCommand::SetVolume { volume, reply } => {
                let result = self.apply_volume(volume);
                respond(reply, result);
            }
            AudioCommand::SetSpectrum { enabled, reply } => {
                let result = self.apply_spectrum(enabled);
                respond(reply, result);
            }
            AudioCommand::SetCacheProfile { economy } => self.apply_cache_profile(economy),
            AudioCommand::Shutdown => {}
        }
    }

    fn play_url(&mut self, url: &str) -> Result<(), AudioError> {
        self.active_url = Some(url.to_owned());
        self.tracker.reset();
        self.layout_retry = None;
        self.bands_active = false;
        // Новая станция может иметь другую раскладку: до FileLoaded держим
        // только безопасный @level, полосы вернутся после определения каналов.
        if self.shared.spectrum_enabled()
            && let Err(err) = self.apply_af_chain(LEVEL_FILTER)
        {
            tracing::warn!(error = %err, "не удалось сбросить граф полос перед новым потоком");
        }
        self.shared.set_spectrum(None);
        self.shared.set_state(PlaybackState::Buffering);
        self.shared.emit(AudioEvent::Buffering);

        if let Err(err) = self.mpv.command("loadfile", &[url, "replace"]) {
            let message = format!("не удалось начать воспроизведение: {err}");
            self.active_url = None;
            self.shared.set_state(PlaybackState::Error(message.clone()));
            self.shared.emit(AudioEvent::Error {
                message: message.clone(),
            });
            return Err(AudioError::Playback(message));
        }

        Ok(())
    }

    fn stop_playback(&mut self) -> Result<(), AudioError> {
        self.mpv
            .command("stop", &[])
            .map_err(|err| AudioError::Playback(err.to_string()))?;

        self.active_url = None;
        self.tracker.reset();
        self.layout_retry = None;
        self.bands_active = false;
        self.shared.set_spectrum(None);
        self.shared.set_state(PlaybackState::Stopped);
        self.shared.emit(AudioEvent::Stopped);
        Ok(())
    }

    fn apply_paused(&mut self, paused: bool) -> Result<(), AudioError> {
        if self.active_url.is_none()
            || matches!(
                self.shared.state(),
                PlaybackState::Stopped | PlaybackState::Reconnecting | PlaybackState::Error(_)
            )
        {
            return Err(AudioError::Playback(
                "нет активного воспроизведения".to_owned(),
            ));
        }

        self.mpv
            .set_property("pause", paused)
            .map_err(|err| AudioError::Playback(err.to_string()))?;

        if paused {
            self.shared.set_state(PlaybackState::Paused);
        } else {
            self.shared.set_state(PlaybackState::Playing);
            if let Some(url) = self.active_url.clone() {
                self.shared.emit(AudioEvent::Started { url });
            }
        }
        Ok(())
    }

    fn apply_volume(&mut self, volume: f64) -> Result<(), AudioError> {
        self.mpv
            .set_property("volume", volume * 100.0)
            .map_err(|err| AudioError::Playback(err.to_string()))?;
        self.shared.set_volume(volume);
        Ok(())
    }

    /// Переключает буфер между экономным профилем и умолчаниями libmpv.
    /// Ошибка свойства только логируется: воспроизведение важнее профиля.
    fn apply_cache_profile(&mut self, economy: bool) {
        if self.economy_cache == economy {
            return;
        }
        self.economy_cache = economy;
        if economy {
            self.set_cache_property("demuxer-max-bytes", ECONOMY_DEMUXER_MAX_BYTES);
            self.set_cache_property("cache-secs", ECONOMY_CACHE_SECS);
            tracing::debug!("включён экономный профиль буфера");
            return;
        }

        if let Some(value) = self.cache_defaults.demuxer_max_bytes {
            self.set_cache_property("demuxer-max-bytes", value);
        } else {
            tracing::debug!("умолчание demuxer-max-bytes неизвестно");
        }
        if let Some(value) = self.cache_defaults.cache_secs {
            self.set_cache_property("cache-secs", value);
        } else {
            tracing::debug!("умолчание cache-secs неизвестно");
        }
        tracing::debug!("восстановлен обычный профиль буфера");
    }

    fn set_cache_property<T: libmpv2::SetData>(&self, name: &str, value: T) {
        if let Err(err) = self.mpv.set_property(name, value) {
            tracing::warn!(property = name, error = %err, "не удалось применить профиль буфера");
        }
    }

    fn apply_spectrum(&mut self, enabled: bool) -> Result<(), AudioError> {
        if !enabled {
            self.set_af("")?;
            self.shared.set_spectrum_enabled(false);
            self.shared.set_spectrum(None);
            self.bands_active = false;
            self.layout_retry = None;
            return Ok(());
        }

        self.shared.set_spectrum_enabled(true);
        self.bands_allowed = true;
        self.layout_retry = None;
        if let Err(err) = self.install_analysis_filters() {
            self.shared.set_spectrum_enabled(false);
            return Err(err);
        }
        Ok(())
    }

    fn install_analysis_filters(&mut self) -> Result<(), AudioError> {
        let layout = self.channel_layout();
        if layout.is_none() && self.active_url.is_some() {
            self.schedule_layout_retry();
        }
        let desired = analysis_chain(layout, self.bands_allowed);
        self.apply_af_chain(&desired)
    }

    fn apply_af_chain(&mut self, desired: &str) -> Result<(), AudioError> {
        if self.applied_af.as_deref() == Some(desired) {
            self.bands_active = desired.contains("@spectrum");
            return Ok(());
        }

        match self.set_af(desired) {
            Ok(()) => {
                self.bands_active = desired.contains("@spectrum");
                self.shared.set_spectrum(None);
                tracing::debug!(bands = self.bands_active, "граф аудио-анализа обновлён");
                Ok(())
            }
            Err(err) if desired.contains("@spectrum") => {
                tracing::warn!(
                    error = %err,
                    "граф полос отклонён mpv; остаётся только @level"
                );
                self.bands_allowed = false;
                self.bands_active = false;
                self.layout_retry = None;
                self.set_af(LEVEL_FILTER)?;
                self.shared.set_spectrum(None);
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    fn set_af(&mut self, chain: &str) -> Result<(), AudioError> {
        self.mpv
            .set_property("af", chain)
            .map_err(|err| AudioError::Spectrum(err.to_string()))?;
        self.applied_af = Some(chain.to_owned());
        Ok(())
    }

    fn channel_layout(&self) -> Option<ChannelLayout> {
        let count = self
            .mpv
            .get_property::<i64>("audio-params/channel-count")
            .ok()?;
        Some(ChannelLayout::from_count(count))
    }

    fn schedule_layout_retry(&mut self) {
        if !self.bands_allowed
            || !self.shared.spectrum_enabled()
            || self.active_url.is_none()
            || self.layout_retry.is_some()
        {
            return;
        }
        self.layout_retry = Some(LayoutRetry {
            at: Instant::now() + LAYOUT_RETRY_INTERVAL,
            attempt: 1,
        });
    }

    fn layout_retry_due(&self, now: Instant) -> bool {
        self.layout_retry.is_some_and(|retry| retry.at <= now)
    }

    fn retry_layout(&mut self) {
        let Some(retry) = self.layout_retry.take() else {
            return;
        };
        if !self.shared.spectrum_enabled() || self.active_url.is_none() {
            return;
        }

        match self.channel_layout() {
            Some(_) => {
                if let Err(err) = self.install_analysis_filters() {
                    tracing::warn!(
                        error = %err,
                        "не удалось обновить граф полос после загрузки"
                    );
                }
            }
            None if retry.attempt < LAYOUT_RETRY_ATTEMPTS => {
                self.layout_retry = Some(LayoutRetry {
                    at: Instant::now() + LAYOUT_RETRY_INTERVAL,
                    attempt: retry.attempt + 1,
                });
            }
            None => {
                tracing::debug!("раскладка каналов не определилась; полосы недоступны");
            }
        }
    }

    fn spectrum_poll_due(&self) -> bool {
        self.last_spectrum_poll
            .is_none_or(|last| last.elapsed() >= SPECTRUM_POLL_INTERVAL)
    }

    fn poll_spectrum(&mut self) {
        self.last_spectrum_poll = Some(Instant::now());

        if !matches!(
            self.shared.state(),
            PlaybackState::Playing | PlaybackState::Buffering
        ) {
            self.shared.set_spectrum(None);
            return;
        }

        if let Some(frame) = read_spectrum(&self.mpv, self.bands_active) {
            self.shared.set_spectrum(Some(frame));
        }
    }

    fn retry(&mut self) {
        let Some(url) = self.active_url.clone() else {
            return;
        };

        tracing::debug!(%url, "повторное подключение к потоку");
        if let Err(err) = self.mpv.command("loadfile", &[&url, "replace"]) {
            self.on_failure(&format!("не удалось переподключиться: {err}"));
        }
    }

    fn handle_event(&mut self, event: WorkerEvent) {
        match event {
            WorkerEvent::FileLoaded => self.on_file_loaded(),
            WorkerEvent::EndFile(reason) => self.on_end_file(reason),
            WorkerEvent::Buffering(buffering) => self.on_buffering(buffering),
            WorkerEvent::Shutdown => self.active_url = None,
            WorkerEvent::Failure(message) => self.on_failure(&message),
        }
    }

    fn on_file_loaded(&mut self) {
        let Some(url) = self.active_url.clone() else {
            return;
        };

        self.tracker.reset();
        if self.shared.spectrum_enabled() {
            self.layout_retry = None;
            if let Err(err) = self.install_analysis_filters() {
                tracing::warn!(
                    error = %err,
                    "не удалось применить граф анализа после загрузки"
                );
            }
        }
        let state = if self.is_paused() {
            PlaybackState::Paused
        } else {
            PlaybackState::Playing
        };
        self.shared.set_state(state);
        self.shared.emit(AudioEvent::Started { url });
    }

    fn is_paused(&self) -> bool {
        match self.mpv.get_property::<bool>("pause") {
            Ok(paused) => paused,
            Err(err) => {
                tracing::trace!(error = %err, "не удалось прочитать свойство pause");
                false
            }
        }
    }

    fn on_end_file(&mut self, reason: libmpv2::EndFileReason) {
        match reason {
            END_FILE_EOF => {
                self.shared.emit(AudioEvent::EndOfFile);
                self.on_failure("поток завершился");
            }
            END_FILE_ERROR => self.on_failure("поток прерван"),
            END_FILE_STOP | END_FILE_QUIT | END_FILE_REDIRECT => {}
            _ => self.on_failure("поток завершился"),
        }
    }

    fn on_buffering(&mut self, buffering: bool) {
        if self.active_url.is_none()
            || matches!(
                self.shared.state(),
                PlaybackState::Stopped
                    | PlaybackState::Paused
                    | PlaybackState::Reconnecting
                    | PlaybackState::Error(_)
            )
        {
            return;
        }

        if buffering {
            if self.shared.state() != PlaybackState::Buffering {
                self.shared.set_state(PlaybackState::Buffering);
                self.shared.emit(AudioEvent::Buffering);
            }
        } else if self.shared.state() == PlaybackState::Buffering
            && let Some(url) = self.active_url.clone()
        {
            self.shared.set_state(PlaybackState::Playing);
            self.shared.emit(AudioEvent::Started { url });
        }
    }

    fn on_failure(&mut self, reason: &str) {
        if self.active_url.is_none() || self.tracker.is_pending() {
            return;
        }
        self.shared.set_spectrum(None);
        self.layout_retry = None;
        self.bands_active = false;

        match self.tracker.on_failure(Instant::now()) {
            RetryOutcome::Scheduled { attempt, delay } => {
                tracing::warn!(
                    attempt,
                    ?delay,
                    %reason,
                    "поток недоступен, планируется переподключение"
                );
                self.shared.set_state(PlaybackState::Reconnecting);
                self.shared.emit(AudioEvent::Reconnecting { attempt });
            }
            RetryOutcome::Exhausted => {
                let message = format!(
                    "не удалось восстановить поток после {} попыток: {reason}",
                    self.tracker.policy.max_attempts
                );
                tracing::error!(%message);
                self.active_url = None;
                self.shared.set_state(PlaybackState::Error(message.clone()));
                self.shared.emit(AudioEvent::Error { message });
            }
        }
    }
}

/// Собирает цепочку `af`: шесть агрегатов всегда, полосы — когда раскладка
/// известна и mpv ещё не отверг граф полос (ADR-0027).
fn analysis_chain(layout: Option<ChannelLayout>, with_bands: bool) -> String {
    let spectrum = layout.filter(|_| with_bands).and_then(spectrum_filter);
    match spectrum {
        Some(filter) => format!("{LEVEL_FILTER},{filter}"),
        None => LEVEL_FILTER.to_owned(),
    }
}

/// Граф трёх полос: копия сигнала уходит в `acrossover`, оригинал возвращается
/// в выход без изменений; `pan` нормализует моно к стерео и обратно.
fn spectrum_filter(layout: ChannelLayout) -> Option<String> {
    match layout {
        ChannelLayout::Stereo => Some(format!(
            "@spectrum:lavfi=[{SPECTRUM_ANALYSIS},pan=stereo|c0=c0|c1=c1]"
        )),
        ChannelLayout::Mono => Some(format!(
            "@spectrum:lavfi=[pan=stereo|c0=c0|c1=c0,{SPECTRUM_ANALYSIS},\
pan=mono|c0=0.5*c0+0.5*c1]"
        )),
        ChannelLayout::Unsupported => None,
    }
}

fn read_spectrum(mpv: &Mpv, bands_active: bool) -> Option<SpectrumFrame> {
    let bands = if bands_active {
        read_band_levels(mpv)
    } else {
        None
    };
    spectrum_from_values(
        read_metric(mpv, LEVEL_CENTROID)?,
        read_metric(mpv, LEVEL_SPREAD)?,
        read_metric(mpv, LEVEL_ROLLOFF)?,
        read_metric(mpv, LEVEL_FLATNESS)?,
        read_metric(mpv, LEVEL_FLUX)?,
        read_metric(mpv, LEVEL_RMS)?,
        bands,
    )
}

fn read_band_levels(mpv: &Mpv) -> Option<BandLevels> {
    let bass = band_db(
        read_metric(mpv, BAND_BASS_LEFT)?,
        read_metric(mpv, BAND_BASS_RIGHT)?,
    )?;
    let mid = band_db(
        read_metric(mpv, BAND_MID_LEFT)?,
        read_metric(mpv, BAND_MID_RIGHT)?,
    )?;
    let high = band_db(
        read_metric(mpv, BAND_HIGH_LEFT)?,
        read_metric(mpv, BAND_HIGH_RIGHT)?,
    )?;
    Some(BandLevels {
        bass_db: bass,
        mid_db: mid,
        high_db: high,
    })
}

/// Объединяет L/R по мощности; `-inf` опускается до пола, `NaN`/`+inf`
/// инвалидируют всю тройку полос.
fn band_db(left_db: f32, right_db: f32) -> Option<f32> {
    let invalid = |value: f32| value.is_nan() || value == f32::INFINITY;
    if invalid(left_db) || invalid(right_db) {
        return None;
    }
    let power = 0.5 * (10f32.powf(left_db / 10.0) + 10f32.powf(right_db / 10.0));
    let db = 10.0 * power.log10();
    Some(if db.is_finite() {
        db.max(BAND_FLOOR_DB)
    } else {
        BAND_FLOOR_DB
    })
}

fn read_metric(mpv: &Mpv, property: &str) -> Option<f32> {
    let raw: String = mpv.get_property(property).ok()?;
    raw.trim().parse::<f32>().ok()
}

fn spectrum_from_values(
    centroid_hz: f32,
    spread_hz: f32,
    rolloff_hz: f32,
    flatness: f32,
    flux: f32,
    rms_db: f32,
    bands: Option<BandLevels>,
) -> Option<SpectrumFrame> {
    if ![centroid_hz, spread_hz, rolloff_hz, flatness, flux, rms_db]
        .into_iter()
        .all(f32::is_finite)
    {
        return None;
    }

    Some(SpectrumFrame {
        centroid_hz: centroid_hz.max(0.0),
        spread_hz: spread_hz.max(0.0),
        rolloff_hz: rolloff_hz.max(0.0),
        flatness: flatness.clamp(0.0, 1.0),
        flux: flux.max(0.0),
        rms_db,
        bands,
    })
}

enum WorkerEvent {
    FileLoaded,
    EndFile(libmpv2::EndFileReason),
    Buffering(bool),
    Shutdown,
    Failure(String),
}

fn respond(reply: oneshot::Sender<CommandResult>, result: CommandResult) {
    if reply.send(result).is_err() {
        tracing::trace!("получатель ответа аудио-команды отключился");
    }
}

#[derive(Debug, Clone, Copy)]
struct ReconnectPolicy {
    max_attempts: u32,
    initial_delay: Duration,
    max_delay: Duration,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            max_attempts: DEFAULT_RECONNECT_ATTEMPTS,
            initial_delay: INITIAL_RECONNECT_DELAY,
            max_delay: MAX_RECONNECT_DELAY,
        }
    }
}

impl ReconnectPolicy {
    fn delay_for(&self, attempt: u32) -> Duration {
        let exponent = attempt.saturating_sub(1).min(31);
        let factor = 1_u32 << exponent;
        self.initial_delay
            .saturating_mul(factor)
            .min(self.max_delay)
    }
}

#[derive(Debug, Default)]
struct ReconnectTracker {
    policy: ReconnectPolicy,
    attempts: u32,
    retry_at: Option<Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetryOutcome {
    Scheduled { attempt: u32, delay: Duration },
    Exhausted,
}

impl ReconnectTracker {
    fn reset(&mut self) {
        self.attempts = 0;
        self.retry_at = None;
    }

    fn on_failure(&mut self, now: Instant) -> RetryOutcome {
        if self.attempts >= self.policy.max_attempts {
            self.retry_at = None;
            return RetryOutcome::Exhausted;
        }

        self.attempts += 1;
        let delay = self.policy.delay_for(self.attempts);
        self.retry_at = Some(now + delay);
        RetryOutcome::Scheduled {
            attempt: self.attempts,
            delay,
        }
    }

    fn is_pending(&self) -> bool {
        self.retry_at.is_some()
    }

    fn take_due(&mut self, now: Instant) -> Option<u32> {
        if self.retry_at.is_some_and(|at| at <= now) {
            self.retry_at = None;
            Some(self.attempts)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(max_attempts: u32) -> ReconnectPolicy {
        ReconnectPolicy {
            max_attempts,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(300),
        }
    }

    #[test]
    fn backoff_grows_exponentially_and_caps() {
        let policy = policy(10);
        assert_eq!(policy.delay_for(1), Duration::from_millis(100));
        assert_eq!(policy.delay_for(2), Duration::from_millis(200));
        assert_eq!(policy.delay_for(3), Duration::from_millis(300));
        assert_eq!(policy.delay_for(100), Duration::from_millis(300));
    }

    #[test]
    fn tracker_schedules_retries_with_backoff() {
        let mut tracker = ReconnectTracker {
            policy: policy(3),
            ..ReconnectTracker::default()
        };
        let now = Instant::now();

        assert_eq!(
            tracker.on_failure(now),
            RetryOutcome::Scheduled {
                attempt: 1,
                delay: Duration::from_millis(100),
            }
        );
        assert!(tracker.is_pending());
        assert_eq!(tracker.take_due(now), None);
        assert_eq!(tracker.take_due(now + Duration::from_millis(100)), Some(1));
        assert!(!tracker.is_pending());
        assert_eq!(
            tracker.on_failure(now),
            RetryOutcome::Scheduled {
                attempt: 2,
                delay: Duration::from_millis(200),
            }
        );
    }

    #[test]
    fn tracker_exhausts_after_max_attempts() {
        let mut tracker = ReconnectTracker {
            policy: policy(2),
            ..ReconnectTracker::default()
        };
        let now = Instant::now();
        assert!(matches!(
            tracker.on_failure(now),
            RetryOutcome::Scheduled { attempt: 1, .. }
        ));
        let _ = tracker.take_due(now + Duration::from_secs(1));
        assert!(matches!(
            tracker.on_failure(now),
            RetryOutcome::Scheduled { attempt: 2, .. }
        ));
        let _ = tracker.take_due(now + Duration::from_secs(1));
        assert_eq!(tracker.on_failure(now), RetryOutcome::Exhausted);
        assert_eq!(tracker.on_failure(now), RetryOutcome::Exhausted);
    }

    #[test]
    fn accepts_valid_spectrum_values() {
        let frame = spectrum_from_values(1741.0, 1679.0, 4008.0, 0.5, 0.001, -19.2, None)
            .expect("кадр должен быть валидным");
        assert_eq!(
            frame,
            SpectrumFrame {
                centroid_hz: 1741.0,
                spread_hz: 1679.0,
                rolloff_hz: 4008.0,
                flatness: 0.5,
                flux: 0.001,
                rms_db: -19.2,
                bands: None,
            }
        );
    }

    #[test]
    fn keeps_band_levels_in_frame() {
        let bands = BandLevels {
            bass_db: -24.0,
            mid_db: -21.0,
            high_db: -30.0,
        };
        let frame = spectrum_from_values(1741.0, 1679.0, 4008.0, 0.5, 0.001, -19.2, Some(bands))
            .expect("кадр должен быть валидным");
        assert_eq!(frame.bands, Some(bands));
    }

    #[test]
    fn rejects_non_finite_spectrum_values() {
        assert!(spectrum_from_values(f32::NAN, 1.0, 1.0, 0.5, 0.0, -20.0, None).is_none());
        assert!(spectrum_from_values(1.0, 1.0, 1.0, 0.5, 0.0, f32::INFINITY, None).is_none());
    }

    #[test]
    fn clamps_spectrum_ranges() {
        let frame = spectrum_from_values(-10.0, -1.0, -2.0, 2.0, -0.5, -3.0, None)
            .expect("кадр должен быть валидным");
        assert_eq!(frame.centroid_hz, 0.0);
        assert_eq!(frame.spread_hz, 0.0);
        assert_eq!(frame.rolloff_hz, 0.0);
        assert_eq!(frame.flatness, 1.0);
        assert_eq!(frame.flux, 0.0);
        assert_eq!(frame.rms_db, -3.0);
    }

    #[test]
    fn combines_band_channels_by_power() {
        assert_eq!(band_db(-20.0, -20.0), Some(-20.0));
        let mixed = band_db(-10.0, -20.0).expect("полоса должна быть валидной");
        assert!((mixed - (-12.596_374)).abs() < 1e-4, "mixed: {mixed}");
    }

    #[test]
    fn floors_silent_band_and_rejects_garbage() {
        let one_silent = band_db(f32::NEG_INFINITY, -30.0).expect("полоса должна быть валидной");
        assert!(
            (one_silent - (-33.010_3)).abs() < 1e-3,
            "one silent: {one_silent}"
        );
        assert_eq!(
            band_db(f32::NEG_INFINITY, f32::NEG_INFINITY),
            Some(BAND_FLOOR_DB)
        );
        assert_eq!(band_db(f32::NAN, -30.0), None);
        assert_eq!(band_db(-30.0, f32::INFINITY), None);
    }

    #[test]
    fn maps_channel_counts_to_layouts() {
        assert_eq!(ChannelLayout::from_count(1), ChannelLayout::Mono);
        assert_eq!(ChannelLayout::from_count(2), ChannelLayout::Stereo);
        assert_eq!(ChannelLayout::from_count(6), ChannelLayout::Unsupported);
    }

    #[test]
    fn analysis_chain_requires_known_layout_and_permission() {
        assert_eq!(analysis_chain(None, true), LEVEL_FILTER);
        assert_eq!(
            analysis_chain(Some(ChannelLayout::Unsupported), true),
            LEVEL_FILTER
        );
        assert_eq!(
            analysis_chain(Some(ChannelLayout::Stereo), false),
            LEVEL_FILTER
        );
    }

    #[test]
    fn analysis_chain_keeps_level_and_spectrum_labels() {
        let stereo = analysis_chain(Some(ChannelLayout::Stereo), true);
        assert!(stereo.starts_with(LEVEL_FILTER));
        assert!(stereo.contains("@spectrum:lavfi=[asplit=2[main][analysis];"));
        assert!(stereo.ends_with("astats=metadata=1:reset=1:measure_perchannel=RMS_level:measure_overall=none,pan=stereo|c0=c0|c1=c1]"));

        let mono = analysis_chain(Some(ChannelLayout::Mono), true);
        assert!(mono.contains("pan=stereo|c0=c0|c1=c0,asplit=2[main][analysis];"));
        assert!(mono.ends_with("pan=mono|c0=0.5*c0+0.5*c1]"));
    }

    #[test]
    fn reset_clears_attempts_and_timer() {
        let mut tracker = ReconnectTracker {
            policy: policy(3),
            ..ReconnectTracker::default()
        };
        let now = Instant::now();
        let _ = tracker.on_failure(now);
        tracker.reset();
        assert!(!tracker.is_pending());
        assert_eq!(
            tracker.on_failure(now),
            RetryOutcome::Scheduled {
                attempt: 1,
                delay: Duration::from_millis(100),
            }
        );
    }
}
