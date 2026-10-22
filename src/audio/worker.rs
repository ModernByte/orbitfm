use std::sync::Arc;
use std::time::{Duration, Instant};

use libmpv2::Mpv;
use libmpv2::events::{Event as MpvEvent, PropertyData};
use tokio::sync::{mpsc, oneshot};

use super::engine::{AudioCommand, CommandResult, Shared};
use super::{AudioError, AudioEvent, PlaybackState, SpectrumFrame};

const EVENT_POLL_INTERVAL_SECS: f64 = 0.05;
const SPECTRUM_POLL_INTERVAL: Duration = Duration::from_millis(50);
const SPECTRUM_FILTER: &str = "@spectrum:lavfi=[\
aspectralstats=measure=centroid+spread+flatness+flux+rolloff:win_size=2048,\
astats=metadata=1:reset=1]";
const SPECTRUM_CENTROID: &str = "af-metadata/spectrum/lavfi.aspectralstats.1.centroid";
const SPECTRUM_SPREAD: &str = "af-metadata/spectrum/lavfi.aspectralstats.1.spread";
const SPECTRUM_ROLLOFF: &str = "af-metadata/spectrum/lavfi.aspectralstats.1.rolloff";
const SPECTRUM_FLATNESS: &str = "af-metadata/spectrum/lavfi.aspectralstats.1.flatness";
const SPECTRUM_FLUX: &str = "af-metadata/spectrum/lavfi.aspectralstats.1.flux";
const SPECTRUM_RMS: &str = "af-metadata/spectrum/lavfi.astats.Overall.RMS_level";
const END_FILE_EOF: libmpv2::EndFileReason = libmpv2::mpv_end_file_reason::Eof;
const END_FILE_ERROR: libmpv2::EndFileReason = libmpv2::mpv_end_file_reason::Error;
const END_FILE_STOP: libmpv2::EndFileReason = libmpv2::mpv_end_file_reason::Stop;
const END_FILE_QUIT: libmpv2::EndFileReason = libmpv2::mpv_end_file_reason::Quit;
const END_FILE_REDIRECT: libmpv2::EndFileReason = libmpv2::mpv_end_file_reason::Redirect;
const DEFAULT_RECONNECT_ATTEMPTS: u32 = 5;
const INITIAL_RECONNECT_DELAY: Duration = Duration::from_secs(1);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(16);

pub(super) struct AudioWorker {
    mpv: Mpv,
    shared: Arc<Shared>,
    commands: mpsc::UnboundedReceiver<AudioCommand>,
    active_url: Option<String>,
    tracker: ReconnectTracker,
    last_spectrum_poll: Option<Instant>,
}

impl AudioWorker {
    pub(super) fn new(
        mpv: Mpv,
        shared: Arc<Shared>,
        commands: mpsc::UnboundedReceiver<AudioCommand>,
    ) -> Self {
        Self {
            mpv,
            shared,
            commands,
            active_url: None,
            tracker: ReconnectTracker::default(),
            last_spectrum_poll: None,
        }
    }

    pub(super) fn run(mut self) {
        loop {
            loop {
                match self.commands.try_recv() {
                    Ok(AudioCommand::Shutdown) => return,
                    Ok(command) => self.handle_command(command),
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => return,
                }
            }

            if self.tracker.take_due(Instant::now()).is_some() {
                self.retry();
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
            Some(Ok(_)) => None,
            Some(Err(err)) => Some(WorkerEvent::Failure(format!("поток прерван: {err}"))),
            None => None,
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
            AudioCommand::Shutdown => {}
        }
    }

    fn play_url(&mut self, url: &str) -> Result<(), AudioError> {
        self.active_url = Some(url.to_owned());
        self.tracker.reset();
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

    fn apply_spectrum(&mut self, enabled: bool) -> Result<(), AudioError> {
        let filters = if enabled { SPECTRUM_FILTER } else { "" };
        self.mpv
            .set_property("af", filters)
            .map_err(|err| AudioError::Spectrum(err.to_string()))?;
        self.shared.set_spectrum_enabled(enabled);
        if !enabled {
            self.shared.set_spectrum(None);
        }
        Ok(())
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

        if let Some(frame) = read_spectrum(&self.mpv) {
            self.shared.set_spectrum(Some(frame));
        }
    }

    fn retry(&mut self) {
        let Some(url) = self.active_url.clone() else {
            return;
        };

        tracing::debug!(%url, "повторное подключение к потоку");
        if let Err(err) = self.mpv.command("loadfile", &[&url, "replace"]) {
            self.on_failure(format!("не удалось переподключиться: {err}"));
        }
    }

    fn handle_event(&mut self, event: WorkerEvent) {
        match event {
            WorkerEvent::FileLoaded => self.on_file_loaded(),
            WorkerEvent::EndFile(reason) => self.on_end_file(reason),
            WorkerEvent::Buffering(buffering) => self.on_buffering(buffering),
            WorkerEvent::Shutdown => self.active_url = None,
            WorkerEvent::Failure(message) => self.on_failure(message),
        }
    }

    fn on_file_loaded(&mut self) {
        let Some(url) = self.active_url.clone() else {
            return;
        };

        self.tracker.reset();
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
                self.on_failure("поток завершился".to_owned());
            }
            END_FILE_ERROR => self.on_failure("поток прерван".to_owned()),
            END_FILE_STOP | END_FILE_QUIT | END_FILE_REDIRECT => {}
            _ => self.on_failure("поток завершился".to_owned()),
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

    fn on_failure(&mut self, reason: String) {
        if self.active_url.is_none() || self.tracker.is_pending() {
            return;
        }
        self.shared.set_spectrum(None);

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

fn read_spectrum(mpv: &Mpv) -> Option<SpectrumFrame> {
    spectrum_from_values(
        read_metric(mpv, SPECTRUM_CENTROID)?,
        read_metric(mpv, SPECTRUM_SPREAD)?,
        read_metric(mpv, SPECTRUM_ROLLOFF)?,
        read_metric(mpv, SPECTRUM_FLATNESS)?,
        read_metric(mpv, SPECTRUM_FLUX)?,
        read_metric(mpv, SPECTRUM_RMS)?,
    )
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
        let frame = spectrum_from_values(1741.0, 1679.0, 4008.0, 0.5, 0.001, -19.2)
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
            }
        );
    }

    #[test]
    fn rejects_non_finite_spectrum_values() {
        assert!(spectrum_from_values(f32::NAN, 1.0, 1.0, 0.5, 0.0, -20.0).is_none());
        assert!(spectrum_from_values(1.0, 1.0, 1.0, 0.5, 0.0, f32::INFINITY).is_none());
    }

    #[test]
    fn clamps_spectrum_ranges() {
        let frame = spectrum_from_values(-10.0, -1.0, -2.0, 2.0, -0.5, -3.0)
            .expect("кадр должен быть валидным");
        assert_eq!(frame.centroid_hz, 0.0);
        assert_eq!(frame.spread_hz, 0.0);
        assert_eq!(frame.rolloff_hz, 0.0);
        assert_eq!(frame.flatness, 1.0);
        assert_eq!(frame.flux, 0.0);
        assert_eq!(frame.rms_db, -3.0);
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
