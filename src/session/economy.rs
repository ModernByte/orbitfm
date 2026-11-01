//! Экономный режим аудио при плохой сети (ADR-0043 §3–4).
//!
//! Решение принимает Session: аудио-ядро только применяет профиль буфера, а
//! лёгкая альтернатива запускается обычным `start_station`, поэтому SSRF,
//! история, click-tracking и MPRIS остаются прежними.

use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tracing::warn;

use crate::audio::{AudioEvent, PlaybackState};
use crate::services::{SearchParams, Station, StationSource, publishable};

use super::notice::NoticeKey;

/// Сколько станция должна буферизоваться, чтобы включить экономный буфер.
const BUFFERING_TRIGGER: Duration = Duration::from_secs(3);
/// Номер попытки реконнекта, с которой включается экономный режим.
const RECONNECT_TRIGGER_ATTEMPT: u32 = 2;
/// Сколько совпадений по имени просить у каталога.
const ALTERNATIVE_LIMIT: u32 = 20;

/// Состояние адаптации к плохой сети: профиль буфера, таймер буферизации,
/// одна попытка подбора лёгкого потока на станцию и признак ECO.
#[derive(Default)]
pub(crate) struct EconomyState {
    buffer_mode: bool,
    buffering_since: Option<Instant>,
    /// Станция, для которой альтернатива уже искалась: не чаще раза в сессию.
    attempted_for: Option<String>,
    /// Текущая станция — подобранная лёгкая замена: индикатор ECO.
    eco_station: Option<String>,
    pending: Option<AlternativeRequest>,
}

struct AlternativeRequest {
    station_uuid: String,
    receiver: mpsc::UnboundedReceiver<Option<Station>>,
}

impl EconomyState {
    /// Реакция на аудио-событие: `Some(profile)` — аудио-ядру нужно переключить
    /// буфер (`true` — экономный, `false` — умолчания libmpv).
    fn observe(&mut self, event: &AudioEvent) -> Option<bool> {
        match event {
            AudioEvent::Started { .. } => {
                self.buffering_since = None;
                self.disable_buffer()
            }
            AudioEvent::Buffering => {
                self.buffering_since.get_or_insert_with(Instant::now);
                None
            }
            AudioEvent::Reconnecting { attempt } => {
                self.buffering_since = None;
                if *attempt >= RECONNECT_TRIGGER_ATTEMPT {
                    self.enable_buffer()
                } else {
                    None
                }
            }
            AudioEvent::Stopped | AudioEvent::Error { .. } => {
                self.buffering_since = None;
                self.eco_station = None;
                self.pending = None;
                self.attempted_for = None;
                self.disable_buffer()
            }
            AudioEvent::EndOfFile => None,
        }
    }

    /// Буферизация длится дольше порога — пора поднимать буфер.
    fn buffering_due(&self, state: &PlaybackState, now: Instant) -> bool {
        matches!(state, PlaybackState::Buffering)
            && self
                .buffering_since
                .is_some_and(|since| now.saturating_duration_since(since) >= BUFFERING_TRIGGER)
    }

    /// Новый выбор станции сбрасывает таймер и право на повторный подбор,
    /// гасит ECO у чужой станции и возвращает обычный буфер (ADR-0043 §3–4).
    fn on_station_selected(&mut self, station_uuid: &str) -> Option<bool> {
        self.buffering_since = None;
        self.pending = None;
        self.attempted_for = None;
        if self.eco_station.as_deref() != Some(station_uuid) {
            self.eco_station = None;
        }
        self.disable_buffer()
    }

    fn eco_active(&self) -> bool {
        self.eco_station.is_some()
    }

    fn mark_eco(&mut self, station_uuid: &str) {
        self.eco_station = Some(station_uuid.to_owned());
    }

    fn enable_buffer(&mut self) -> Option<bool> {
        if self.buffer_mode {
            return None;
        }
        self.buffer_mode = true;
        Some(true)
    }

    fn disable_buffer(&mut self) -> Option<bool> {
        if !self.buffer_mode {
            return None;
        }
        self.buffer_mode = false;
        Some(false)
    }

    /// `true` — можно искать альтернативу: для этой станции ещё не искали.
    fn begin_attempt(&mut self, station_uuid: &str) -> bool {
        if self.attempted_for.as_deref() == Some(station_uuid) {
            return false;
        }
        self.attempted_for = Some(station_uuid.to_owned());
        true
    }
}

impl super::Session {
    /// Передаёт аудио-событие экономному режиму: профиль буфера и, со второй
    /// попытки реконнекта, поиск более лёгкого потока (ADR-0043 §3–4).
    pub(crate) fn observe_economy(&mut self, event: &AudioEvent) {
        if let Some(profile) = self.economy.observe(event) {
            self.audio.set_cache_profile(profile);
        }
        if let AudioEvent::Reconnecting { attempt } = event {
            self.note_reconnect_attempt(*attempt);
        }
    }

    /// Запуск новой станции: ECO гаснет у чужой записи, буфер возвращается
    /// к умолчаниям, подбор альтернативы начинается заново (ADR-0043).
    pub(crate) fn note_station_selected(&mut self, station_uuid: &str) {
        if let Some(profile) = self.economy.on_station_selected(station_uuid) {
            self.audio.set_cache_profile(profile);
        }
    }

    /// Кадровое обслуживание экономного режима: таймер буферизации и ответ
    /// фонового поиска альтернативы.
    pub(crate) fn poll_economy(&mut self) {
        if self
            .economy
            .buffering_due(&self.audio.state(), Instant::now())
        {
            self.enter_economy();
        }
        self.poll_economy_alternative();
    }

    /// Вторая и последующие попытки реконнекта: буфер уже поднят событием,
    /// здесь запускается однократный подбор более лёгкого потока.
    pub(crate) fn note_reconnect_attempt(&mut self, attempt: u32) {
        if attempt >= RECONNECT_TRIGGER_ATTEMPT {
            self.try_economy_alternative();
        }
    }

    fn enter_economy(&mut self) {
        if let Some(profile) = self.economy.enable_buffer() {
            self.audio.set_cache_profile(profile);
        }
        self.try_economy_alternative();
    }

    fn try_economy_alternative(&mut self) {
        if self.economy.pending.is_some() {
            return;
        }
        let Some(current) = self.current_station.clone() else {
            return;
        };
        // У внешнего потока нет записи в каталоге, а его URL — явный выбор
        // пользователя: подменять его нечем (ADR-0040, 0043 §4).
        if current.source != StationSource::Radio || current.stationuuid.starts_with("url:") {
            return;
        }
        if !self.economy.begin_attempt(&current.stationuuid) {
            return;
        }

        let sample = self.world.sample_shared();
        if let Some(station) = alternative_from(&sample, &current) {
            self.start_alternative(station);
            return;
        }

        let radio = self.radio.clone();
        let waker = self.wake.waker();
        let (sender, receiver) = mpsc::unbounded_channel();
        self.economy.pending = Some(AlternativeRequest {
            station_uuid: current.stationuuid.clone(),
            receiver,
        });
        self.runtime.spawn(async move {
            let params = SearchParams {
                name: Some(current.name.clone()),
                limit: ALTERNATIVE_LIMIT,
                hide_broken: true,
                ..SearchParams::default()
            };
            let choice = match radio.search(params).await {
                Ok(stations) => alternative_from(&stations, &current),
                Err(err) => {
                    warn!(error = %err, "каталог не ответил на поиск лёгкого потока");
                    None
                }
            };
            let _ = sender.send(choice);
            waker();
        });
    }

    fn poll_economy_alternative(&mut self) {
        let Some(request) = self.economy.pending.as_mut() else {
            return;
        };
        let (station_uuid, choice) = match request.receiver.try_recv() {
            Ok(choice) => (request.station_uuid.clone(), choice),
            Err(mpsc::error::TryRecvError::Empty) => return,
            Err(mpsc::error::TryRecvError::Disconnected) => {
                self.economy.pending = None;
                return;
            }
        };
        self.economy.pending = None;

        let Some(station) = choice else {
            return;
        };
        let still_current = self
            .current_station
            .as_ref()
            .is_some_and(|current| current.stationuuid == station_uuid);
        let struggling = matches!(
            self.audio.state(),
            PlaybackState::Reconnecting | PlaybackState::Buffering
        );
        // Пока шёл поиск, пользователь мог выбрать другую станцию или поток
        // мог восстановиться: тогда подмена не нужна.
        if still_current && struggling {
            self.start_alternative(station);
        }
    }

    fn start_alternative(&mut self, station: Station) {
        let station_uuid = station.stationuuid.clone();
        let bitrate = station.bitrate;
        match self.start_station(station) {
            Ok(()) => {
                self.economy.mark_eco(&station_uuid);
                self.notify_info(NoticeKey::EconomyStream { bitrate });
            }
            Err(err) => warn!(error = %err, "экономная альтернатива не запустилась"),
        }
    }

    /// Показывать ли индикатор ECO рядом с битрейтом (ADR-0043 §4).
    pub(crate) fn economy_stream(&self) -> bool {
        self.economy.eco_active()
    }
}

/// Кандидат «легче» текущего: при неизвестном битрейте годится любой поток
/// с битрейтом, иначе — только строго меньший (ADR-0043 §4).
fn lighter(candidate: u32, current: u32) -> bool {
    candidate > 0 && (current == 0 || candidate < current)
}

/// Имя для сравнения дублей: регистр и пунктуация не важны, важен порядок слов.
fn normalized_name(name: &str) -> String {
    name.split(|ch: char| !ch.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Лёгкая копия станции из локальной выборки или ответа каталога. Требования
/// ADR-0043 §4: другое имя записи, битрейт меньше текущего, публичный адрес.
fn alternative_from(sample: &[Station], current: &Station) -> Option<Station> {
    let target = normalized_name(&current.name);
    if target.is_empty() {
        return None;
    }
    sample
        .iter()
        .filter(|candidate| candidate.stationuuid != current.stationuuid)
        .filter(|candidate| normalized_name(&candidate.name) == target)
        .filter(|candidate| lighter(candidate.bitrate, current.bitrate))
        .filter(|candidate| candidate.url_resolved != current.url_resolved)
        .filter(|candidate| publishable(&candidate.url_resolved).is_some())
        .min_by(|left, right| {
            left.bitrate
                .cmp(&right.bitrate)
                .then_with(|| left.stationuuid.cmp(&right.stationuuid))
        })
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::markers::sample_station;

    fn station(uuid: &str, name: &str, bitrate: u32, url: &str) -> Station {
        let mut station = sample_station();
        station.stationuuid = uuid.to_owned();
        station.name = name.to_owned();
        station.bitrate = bitrate;
        station.url_resolved = url.to_owned();
        station
    }

    fn current() -> Station {
        station("current", "Radio Nova", 320, "https://nova.example/hi")
    }

    #[test]
    fn normalized_names_ignore_case_and_punctuation() {
        assert_eq!(normalized_name("Radio  Nova!"), "radio nova");
        assert_eq!(normalized_name("  RADIO-NOVA "), "radio nova");
        assert_eq!(normalized_name("Радио «Волна»"), "радио волна");
        assert_eq!(normalized_name("!!!"), "");
    }

    #[test]
    fn lighter_prefers_strictly_smaller_bitrate_but_accepts_unknown_current() {
        assert!(lighter(128, 320));
        assert!(!lighter(320, 320));
        assert!(!lighter(0, 320));
        assert!(lighter(128, 0));
        assert!(!lighter(0, 0));
    }

    #[test]
    fn alternative_prefers_the_lightest_public_duplicate() {
        let heavy = station("heavy", "radio nova!", 256, "https://nova.example/heavy");
        let light = station("light", "RADIO  NOVA", 96, "https://nova.example/light");
        let private = station("private", "Radio Nova", 64, "http://127.0.0.1/stream");
        let same_url = station("same", "Radio Nova", 64, "https://nova.example/hi");
        let stranger = station(
            "stranger",
            "Radio Nova FM",
            64,
            "https://other.example/stream",
        );
        let sample = vec![heavy, light, private, same_url, stranger];

        let choice = alternative_from(&sample, &current()).expect("альтернатива найдена");
        assert_eq!(choice.stationuuid, "light");
        assert_eq!(choice.bitrate, 96);
    }

    #[test]
    fn alternative_ignores_current_entry_and_missing_names() {
        let mut nameless = current();
        nameless.name.clear();
        assert!(alternative_from(&[current()], &nameless).is_none());
        assert!(alternative_from(&[current()], &current()).is_none());
    }

    #[test]
    fn alternative_accepts_any_bitrate_when_current_is_unknown() {
        let mut unknown = current();
        unknown.bitrate = 0;
        let candidate = station("low", "Radio Nova", 64, "https://nova.example/low");
        let choice = alternative_from(&[candidate], &unknown).expect("битрейт не важен");
        assert_eq!(choice.stationuuid, "low");
    }

    #[test]
    fn buffering_enters_economy_after_three_seconds() {
        let mut economy = EconomyState::default();
        assert_eq!(economy.observe(&AudioEvent::Buffering), None);
        let started = Instant::now();

        let state = PlaybackState::Buffering;
        assert!(!economy.buffering_due(&state, started + Duration::from_millis(2_900)));
        assert!(economy.buffering_due(&state, started + Duration::from_secs(3)));
        assert!(!economy.buffering_due(&PlaybackState::Playing, started + Duration::from_secs(9)));
    }

    #[test]
    fn reconnect_enables_buffer_from_the_second_attempt() {
        let mut economy = EconomyState::default();
        assert_eq!(
            economy.observe(&AudioEvent::Reconnecting { attempt: 1 }),
            None
        );
        assert_eq!(
            economy.observe(&AudioEvent::Reconnecting { attempt: 2 }),
            Some(true)
        );
        assert_eq!(
            economy.observe(&AudioEvent::Reconnecting { attempt: 3 }),
            None,
            "повторное включение не шлёт команду"
        );
    }

    #[test]
    fn started_restores_defaults_and_keeps_eco_marker() {
        let mut economy = EconomyState::default();
        let _ = economy.observe(&AudioEvent::Reconnecting { attempt: 2 });
        economy.mark_eco("light");

        assert_eq!(
            economy.observe(&AudioEvent::Started {
                url: "https://nova.example/light".to_owned()
            }),
            Some(false),
            "после успешного старта буфер возвращается к умолчаниям"
        );
        assert!(economy.eco_active());
        assert_eq!(economy.observe(&AudioEvent::EndOfFile), None);
    }

    #[test]
    fn selecting_another_station_resets_eco_and_buffer() {
        let mut economy = EconomyState::default();
        let _ = economy.observe(&AudioEvent::Reconnecting { attempt: 2 });
        economy.mark_eco("light");

        assert_eq!(economy.on_station_selected("light"), Some(false));
        assert!(economy.eco_active(), "та же станция остаётся экономной");

        let _ = economy.observe(&AudioEvent::Reconnecting { attempt: 2 });
        economy.mark_eco("light");
        assert_eq!(economy.on_station_selected("other"), Some(false));
        assert!(!economy.eco_active(), "другая станция снимает ECO");
        assert_eq!(economy.on_station_selected("other"), None);
    }

    #[test]
    fn stopped_and_errors_clear_eco_and_timers() {
        for event in [
            AudioEvent::Stopped,
            AudioEvent::Error {
                message: "нет сети".to_owned(),
            },
        ] {
            let mut economy = EconomyState::default();
            let _ = economy.observe(&AudioEvent::Reconnecting { attempt: 2 });
            economy.mark_eco("light");
            let _ = economy.observe(&AudioEvent::Buffering);
            assert_eq!(economy.observe(&event), Some(false));
            assert!(!economy.eco_active());
            assert!(!economy.buffering_due(
                &PlaybackState::Buffering,
                Instant::now() + Duration::from_secs(10)
            ));
        }
    }

    #[test]
    fn alternative_is_searched_once_per_station() {
        let mut economy = EconomyState::default();
        assert!(economy.begin_attempt("current"));
        assert!(!economy.begin_attempt("current"));
        economy.on_station_selected("next");
        assert!(economy.begin_attempt("next"));
    }
}
