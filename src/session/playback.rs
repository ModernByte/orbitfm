//! Воспроизведение, избранное и системные медиа-контролы.
//!
//! Здесь нет egui: о проблемах `Session` сообщает через [`NoticeKey`], а UI сам
//! решает, какой строкой каталога показать тост (ADR-0030).

use std::fmt;

use crate::audio::{AudioEvent, PlaybackState};
use crate::geo::Country;
use crate::media::{MediaCommand, PlaybackInfo};
use crate::services::{RadioError, Station};
use crate::ui::single_line;
use crate::ui::station_filter::filtered_results;

use super::LibraryTab;
use super::WINDOW_TITLE;
use super::notice::NoticeKey;

/// Причина, по которой станция не запустилась: сообщение выбирает UI по ключу.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StationStartError {
    /// В записи станции пустой `url_resolved`.
    NoUrl,
    /// Отказ аудио-ядра; техническая подробность не переводится.
    Audio(String),
}

impl fmt::Display for StationStartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoUrl => f.write_str("station has no url_resolved"),
            Self::Audio(err) => f.write_str(err),
        }
    }
}

impl super::Session {
    /// Собирает аудио-события, накопившиеся с прошлого кадра.
    pub(crate) fn process_audio_events(&mut self) {
        loop {
            let event = match self.audio_events.try_recv() {
                Ok(event) => event,
                Err(tokio::sync::broadcast::error::TryRecvError::Empty)
                | Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "часть аудио-событий пропущена");
                    continue;
                }
            };
            self.apply_audio_event(event);
        }
    }

    pub(crate) fn apply_audio_event(&mut self, event: AudioEvent) {
        if let AudioEvent::Started { url } = event {
            self.current_url = Some(url);
        }
    }

    /// Ищет станцию в видимом списке, иначе тянет из Radio Browser по UUID.
    pub(crate) fn resolve_station(&self, station_uuid: &str) -> Result<Station, RadioError> {
        if let Some(station) = self
            .search
            .results
            .iter()
            .find(|station| station.stationuuid == station_uuid)
        {
            return Ok(station.clone());
        }

        let result = self.runtime.block_on(async {
            tokio::time::timeout(
                super::search::STATION_LOOKUP_TIMEOUT,
                self.radio.get_station(station_uuid),
            )
            .await
        });

        match result {
            Ok(result) => result,
            Err(_) => Err(RadioError::Network("stations catalog timeout".to_owned())),
        }
    }

    /// Запуск станции: mpv, история, click-трекинг и витрина MPRIS.
    pub(crate) fn start_station(&mut self, station: Station) -> Result<(), StationStartError> {
        let url = station.url_resolved.trim().to_owned();
        if url.is_empty() {
            return Err(StationStartError::NoUrl);
        }

        self.runtime
            .block_on(self.audio.play(&url))
            .map_err(|err| StationStartError::Audio(err.to_string()))?;

        self.current_url = Some(url);
        self.current_station = Some(station.clone());
        // Запуск из списка или hotkey тоже выбирает станцию: маркер глобуса
        // обязан отозваться тем же выделением (ADR-0036).
        self.selected_station = Some(station.stationuuid.clone());
        if let Err(err) = self.library.record_played(&station) {
            tracing::warn!(error = %err, "не удалось сохранить историю станций");
            self.notify_error_detail(NoticeKey::HistoryNotSaved, err.to_string());
        }
        self.spawn_click(station.stationuuid);
        self.sync_media();
        Ok(())
    }

    /// Клик по маркеру: станция становится выбранной, панель переключается на
    /// результаты и один раз прокручивает строку в видимую часть (ADR-0036).
    pub(crate) fn select_station(&mut self, station: &Station) {
        self.selected_station = Some(station.stationuuid.clone());
        self.reveal_station = Some(station.stationuuid.clone());
        // Локальный фильтр мог спрятать строку выбранной станции: клик по
        // маркеру важнее фильтра, иначе прокрутка уйдёт в пустоту.
        self.country_filter.reset();
        self.library_tab = LibraryTab::Results;
    }

    /// Забирает разовую просьбу прокрутки: кадр панели потребляет её один раз.
    pub(crate) fn take_station_reveal(&mut self) -> Option<String> {
        self.reveal_station.take()
    }

    /// Click-трекинг в Radio Browser; ошибка не мешает слушать.
    pub(crate) fn spawn_click(&self, station_uuid: String) {
        let radio = self.radio.clone();
        self.runtime.spawn(async move {
            if let Err(err) = radio.send_click(&station_uuid).await {
                tracing::warn!(error = %err, "не удалось отправить click в Radio Browser");
            }
        });
    }

    pub(crate) fn toggle_favorite(&mut self, station: &Station) {
        match self.library.toggle_favorite(station) {
            Ok(favorite) => tracing::debug!(favorite, "избранное обновлено"),
            Err(err) => {
                tracing::warn!(error = %err, "не удалось сохранить избранное");
                self.notify_error_detail(NoticeKey::FavoritesNotSaved, err.to_string());
            }
        }
    }

    pub(crate) fn clear_history(&mut self) {
        match self.library.clear_history() {
            Ok(()) => self.notify_info(NoticeKey::HistoryCleared),
            Err(err) => {
                tracing::warn!(error = %err, "не удалось очистить историю");
                self.notify_error_detail(NoticeKey::ClearHistoryFailed, err.to_string());
            }
        }
    }

    /// Наводит камеру глобуса на страну.
    pub(crate) fn focus_country(&mut self, country: &Country) {
        if let Some((lat, lon)) = self
            .geo
            .country_shape(&country.iso_a2)
            .and_then(|shape| shape.focus_point())
        {
            self.globe_camera
                .fly_to(lat, lon, std::time::Duration::from_millis(650));
        }
    }

    /// Дискретная команда смены громкости (IPC, трей, MPRIS): применить и
    /// сохранить сразу — она приходит готовым значением.
    pub(crate) fn set_volume(&mut self, volume: f64) -> Result<(), String> {
        self.pending_volume = None;
        self.apply_and_persist_volume(volume)
    }

    /// Preview перетаскивания слайдера: звук меняется сразу, файл — нет
    /// (ADR-0031, commit-on-release).
    pub(crate) fn preview_volume(&mut self, volume: f64) -> Result<(), String> {
        self.runtime
            .block_on(self.audio.set_volume(volume))
            .map_err(|err| err.to_string())?;
        self.pending_volume = Some(volume);
        Ok(())
    }

    /// Завершение перетаскивания, клик по треку или клавиатура: значение
    /// становится настройкой.
    pub(crate) fn commit_volume(&mut self, volume: f64) -> Result<(), String> {
        self.pending_volume = None;
        self.apply_and_persist_volume(volume)
    }

    /// Записывает незакрытый preview в настройки: зовётся при закрытии окна,
    /// чтобы drag, прерванный скрытием, не потерял значение.
    pub(crate) fn flush_pending_volume(&mut self) {
        let Some(volume) = self.pending_volume.take() else {
            return;
        };
        if let Err(err) = self.settings.set_volume(volume) {
            tracing::warn!(error = %err, "не удалось сохранить громкость при закрытии окна");
        }
    }

    fn apply_and_persist_volume(&mut self, volume: f64) -> Result<(), String> {
        self.runtime
            .block_on(self.audio.set_volume(volume))
            .map_err(|err| err.to_string())?;
        if let Err(err) = self.settings.set_volume(volume) {
            tracing::warn!(error = %err, "не удалось сохранить громкость");
            self.notify_error_detail(NoticeKey::VolumeNotSaved, err.to_string());
        }
        Ok(())
    }

    pub(crate) fn set_audio_paused(&mut self, paused: bool) -> Result<(), String> {
        self.runtime
            .block_on(self.audio.set_paused(paused))
            .map_err(|err| err.to_string())
    }

    pub(crate) fn resume_playback(&mut self) -> Result<(), String> {
        match self.audio.state() {
            PlaybackState::Paused => self.set_audio_paused(false),
            PlaybackState::Playing | PlaybackState::Buffering | PlaybackState::Reconnecting => {
                Ok(())
            }
            PlaybackState::Stopped | PlaybackState::Error(_) => match self.current_url.clone() {
                Some(url) => self
                    .runtime
                    .block_on(self.audio.play(&url))
                    .map_err(|err| err.to_string()),
                None => Ok(()),
            },
        }
    }

    pub(crate) fn toggle_playback(&mut self) -> Result<(), String> {
        match self.audio.state() {
            PlaybackState::Paused => self.set_audio_paused(false),
            PlaybackState::Playing | PlaybackState::Buffering => self.set_audio_paused(true),
            PlaybackState::Reconnecting => Ok(()),
            PlaybackState::Stopped | PlaybackState::Error(_) => self.resume_playback(),
        }
    }

    pub(crate) fn stop_playback(&mut self) -> Result<(), String> {
        self.runtime
            .block_on(self.audio.stop())
            .map_err(|err| err.to_string())
    }

    pub(crate) fn process_media_commands(&mut self) {
        loop {
            let next = self
                .media
                .as_mut()
                .and_then(|media| media.events().try_recv().ok());
            let Some(command) = next else { break };
            self.execute_media_command(command);
        }
    }

    pub(crate) fn execute_media_command(&mut self, command: MediaCommand) {
        match command {
            MediaCommand::Stop => {
                if let Err(err) = self.stop_playback() {
                    tracing::warn!(error = %err, "не удалось остановить воспроизведение из MPRIS");
                }
            }
            MediaCommand::SetVolume(volume) => {
                if let Err(err) = self.set_volume(volume) {
                    tracing::warn!(error = %err, "не удалось изменить громкость из MPRIS");
                }
            }
            MediaCommand::Play => {
                if let Err(err) = self.resume_playback() {
                    tracing::warn!(error = %err, "не удалось возобновить воспроизведение из MPRIS");
                }
            }
            MediaCommand::Pause => {
                if let Err(err) = self.set_audio_paused(true) {
                    tracing::warn!(error = %err, "не удалось поставить на паузу из MPRIS");
                }
            }
            MediaCommand::PlayPause => match self.audio.state() {
                PlaybackState::Paused => {
                    if let Err(err) = self.set_audio_paused(false) {
                        tracing::warn!(error = %err, "не удалось снять паузу из MPRIS");
                    }
                }
                PlaybackState::Playing | PlaybackState::Buffering => {
                    if let Err(err) = self.set_audio_paused(true) {
                        tracing::warn!(error = %err, "не удалось поставить на паузу из MPRIS");
                    }
                }
                PlaybackState::Stopped | PlaybackState::Error(_) => {
                    if let Err(err) = self.resume_playback() {
                        tracing::warn!(error = %err, "не удалось запустить воспроизведение из MPRIS");
                    }
                }
                PlaybackState::Reconnecting => {}
            },
            MediaCommand::Next => self.play_relative(1),
            MediaCommand::Previous => self.play_relative(-1),
        }
    }

    /// Снимок видимого списка станций — очередь навигации prev/next.
    pub(crate) fn visible_queue(&self) -> Vec<Station> {
        match self.library_tab {
            LibraryTab::Favorites => self.library.favorites().to_vec(),
            LibraryTab::History => self.library.history().to_vec(),
            LibraryTab::Results => {
                if self.search.context.is_active() {
                    filtered_results(&self.search.results, &self.country_filter)
                        .into_iter()
                        .cloned()
                        .collect()
                } else {
                    self.search.results.clone()
                }
            }
        }
    }

    /// Число станций в видимой очереди без клонирования.
    pub(crate) fn visible_queue_len(&self) -> usize {
        match self.library_tab {
            LibraryTab::Favorites => self.library.favorites().len(),
            LibraryTab::History => self.library.history().len(),
            LibraryTab::Results => {
                if self.search.context.is_active() {
                    filtered_results(&self.search.results, &self.country_filter).len()
                } else {
                    self.search.results.len()
                }
            }
        }
    }

    /// Шаг по видимому списку с wrap; при пустой очереди — no-op.
    pub(crate) fn play_relative(&mut self, step: i8) {
        let queue = self.visible_queue();
        if queue.is_empty() {
            return;
        }
        let current = self.current_station.as_ref().and_then(|station| {
            queue
                .iter()
                .position(|item| item.stationuuid == station.stationuuid)
        });
        let next = match current {
            Some(index) => {
                (index as isize + step as isize).rem_euclid(queue.len() as isize) as usize
            }
            None if step >= 0 => 0,
            None => queue.len() - 1,
        };
        if let Err(err) = self.start_station(queue[next].clone()) {
            match err {
                StationStartError::NoUrl => self.notify_error(NoticeKey::StationWithoutUrl),
                StationStartError::Audio(detail) => {
                    self.notify_error_detail(NoticeKey::PlaybackFailed, detail);
                }
            }
        }
    }

    /// Публикует текущее состояние в систему (MPRIS / Now Playing).
    pub(crate) fn sync_media(&mut self) {
        let state = self.audio.state();
        let has_track = !matches!(state, PlaybackState::Stopped | PlaybackState::Error(_));
        let playing = matches!(
            state,
            PlaybackState::Playing | PlaybackState::Buffering | PlaybackState::Reconnecting
        );
        let fallback_url = self.current_url.clone().unwrap_or_default();
        // Публикуем только ссылки, прошедшие ярус 1 и подтверждённые ярусом 2:
        // приватный адрес не должен уходить наружу DE-фетчеру обложек
        // (ADR-0025, п. 12). Вердикт яруса 2 считается в фоне, поэтому первый
        // кадр может отдать пустое поле — это лучше, чем непроверенный адрес.
        let published_fallback = if self.publish_gate.permits(&fallback_url) {
            fallback_url.clone()
        } else {
            String::new()
        };
        let (base_title, artist, art_url, url) = match &self.current_station {
            Some(station) => (
                single_line(&station.name),
                mpris_artist(station),
                station
                    .favicon
                    .as_deref()
                    .filter(|reference| self.publish_gate.permits(reference))
                    .map(str::to_owned),
                if station.url_resolved.trim().is_empty() {
                    published_fallback
                } else if self.publish_gate.permits(&station.url_resolved) {
                    station.url_resolved.clone()
                } else {
                    String::new()
                },
            ),
            None => (fallback_url, None, None, published_fallback),
        };
        let title = if has_track {
            if base_title.is_empty() {
                WINDOW_TITLE.to_owned()
            } else {
                base_title
            }
        } else {
            String::new()
        };

        let can_skip = self.visible_queue_len() > 0;
        let info = PlaybackInfo {
            title,
            artist,
            art_url,
            url,
            playing,
            volume: self.audio.volume(),
            can_go_next: can_skip,
            can_go_previous: can_skip,
        };

        if let Some(media) = self.media.as_mut()
            && let Err(err) = media.set_playback(&info)
        {
            tracing::warn!(error = %err, "не удалось обновить MPRIS");
        }
    }
}

/// Артист в MPRIS: сначала место, при его отсутствии — жанры.
fn mpris_artist(station: &Station) -> Option<String> {
    let state = single_line(&station.state);
    let country = single_line(&station.country);
    let location = match (state.is_empty(), country.is_empty()) {
        (false, false) => Some(format!("{state}, {country}")),
        (false, true) => Some(state),
        (true, false) => Some(country),
        (true, true) => None,
    };

    location.or_else(|| {
        let tags = single_line(&station.tags);
        (!tags.is_empty()).then_some(tags)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::markers::sample_station;

    #[test]
    fn mpris_artist_prefers_location_over_tags() {
        let mut station = sample_station();
        assert_eq!(
            mpris_artist(&station).as_deref(),
            Some("New York, United States")
        );

        station.state.clear();
        assert_eq!(mpris_artist(&station).as_deref(), Some("United States"));

        station.country.clear();
        assert_eq!(mpris_artist(&station).as_deref(), Some("jazz"));

        station.tags.clear();
        assert_eq!(mpris_artist(&station), None);
    }
}
