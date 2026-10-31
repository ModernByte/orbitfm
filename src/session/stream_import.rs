//! Импорт внешних потоков по явной команде `play-url` (ADR-0040).
//!
//! Резолв идёт в tokio-задаче, а не в `execute`: сетевой запрос не должен
//! останавливать UI-цикл и припаркованный простой. Клиент получает ответ после
//! `start_station`; второй параллельный импорт отклоняется.

use tokio::sync::mpsc;
use tracing::warn;

use crate::ipc::{IpcCommand, IpcRequest};
use crate::services::Station;
use crate::services::stream::{self, ResolvedStream, StreamError};

/// Имя синтетической станции, когда ни плейлист, ни страница его не дали.
const FALLBACK_NAME: &str = "Поток";

/// Незавершённый импорт: запрос ждёт ответа, резолвер — результата.
pub(crate) struct PendingStream {
    request: IpcRequest,
    pub(crate) rx: mpsc::UnboundedReceiver<Result<ResolvedStream, StreamError>>,
}

impl super::Session {
    /// Ставит `play-url` в работу. Повторная команда до ответа отклоняется:
    /// два параллельных резолва не нужны и путали бы ответы по времени.
    pub(crate) fn begin_play_url(&mut self, request: IpcRequest) {
        if self.pending_stream.is_some() {
            request.try_respond("error импорт уже выполняется");
            return;
        }
        let IpcCommand::PlayUrl { url } = request.command().clone() else {
            request.try_respond("error play-url: ожидается URL");
            return;
        };

        let (sender, receiver) = mpsc::unbounded_channel();
        self.pending_stream = Some(PendingStream {
            request,
            rx: receiver,
        });
        let waker = self.wake.waker();
        self.runtime.spawn(async move {
            let result = stream::resolve(&url).await;
            let _ = sender.send(result);
            waker();
        });
    }

    /// Забирает результат завершившегося резолва при открытом окне.
    pub(crate) fn poll_stream_import(&mut self) {
        let outcome = {
            let Some(pending) = self.pending_stream.as_mut() else {
                return;
            };
            match pending.rx.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::error::TryRecvError::Empty) => return,
                Err(mpsc::error::TryRecvError::Disconnected) => None,
            }
        };
        self.finish_stream_import(outcome);
    }

    /// Ответ клиенту после резолва: `ok`, ошибка резолвера или отказ запуска.
    pub(crate) fn finish_stream_import(
        &mut self,
        outcome: Option<Result<ResolvedStream, StreamError>>,
    ) {
        let Some(PendingStream { request, .. }) = self.pending_stream.take() else {
            return;
        };
        match outcome {
            Some(Ok(resolved)) => match self.start_play_url(&resolved) {
                Ok(()) => {
                    request.try_respond("ok");
                }
                Err(err) => {
                    request.try_respond(format!("error {err}"));
                }
            },
            Some(Err(err)) => {
                request.try_respond(format!("error {err}"));
            }
            None => {
                request.try_respond("error резолвер потока завершился");
            }
        }
    }

    /// Запускает разрешённый поток как станцию: история, MPRIS и громкость —
    /// тот же путь, что у радиостанции. Click-трекинг Radio Browser пропускается.
    pub(crate) fn start_play_url(&mut self, resolved: &ResolvedStream) -> Result<(), String> {
        let station = stream_station(resolved);
        self.start_station(station).map_err(|err| err.to_string())?;
        warn!(url = %resolved.url, "внешний поток добавлен в историю");
        Ok(())
    }
}

fn stream_station(resolved: &ResolvedStream) -> Station {
    let name = resolved
        .title
        .as_deref()
        .map(crate::ui::single_line)
        .filter(|title| !title.is_empty())
        .or_else(|| file_name(&resolved.url))
        .unwrap_or_else(|| FALLBACK_NAME.to_owned());
    Station {
        stationuuid: stream_id(resolved.homepage.as_deref().unwrap_or(&resolved.url)),
        name,
        url: resolved.url.clone(),
        url_resolved: resolved.url.clone(),
        homepage: resolved.homepage.clone(),
        favicon: None,
        tags: String::new(),
        countrycode: String::new(),
        language: String::new(),
        votes: 0,
        country: String::new(),
        state: String::new(),
        bitrate: 0,
        codec: String::new(),
        geo_lat: None,
        geo_long: None,
        source: crate::services::StationSource::Stream,
    }
}

/// Имя файла из пути — запасное название для прямых ссылок на аудио.
fn file_name(url: &str) -> Option<String> {
    let without_query = url.split(['?', '#']).next().unwrap_or(url);
    let segment = without_query.rsplit('/').next()?.trim();
    let name = segment.split_once('.').map_or(segment, |(name, _)| name);
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

/// Идентификатор синтетической станции: FNV-1a 64 от исходного URL. Хеш нужен,
/// чтобы избранное и история ссылались на один и тот же поток между сессиями.
pub(crate) fn stream_id(source: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in source.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("url:{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_id_is_stable_and_distinct() {
        let first = stream_id("https://example.org/page");
        assert_eq!(first, stream_id("https://example.org/page"));
        assert!(first.starts_with("url:"));
        assert_eq!(first.len(), 4 + 16);
        assert_ne!(first, stream_id("https://example.org/other"));
    }

    #[test]
    fn station_uses_title_homepage_and_stream_source() {
        let resolved = ResolvedStream {
            url: "https://cdn.example.org/live.mp3".to_owned(),
            title: Some("Deep Focus".to_owned()),
            homepage: Some("https://example.org/page".to_owned()),
        };
        let station = stream_station(&resolved);
        assert_eq!(station.name, "Deep Focus");
        assert_eq!(station.url_resolved, resolved.url);
        assert_eq!(
            station.homepage.as_deref(),
            Some("https://example.org/page")
        );
        assert_eq!(station.source, crate::services::StationSource::Stream);
        assert_eq!(station.stationuuid, stream_id("https://example.org/page"));
        assert!(station.favicon.is_none());
        assert!(station.tags.is_empty());
    }

    #[test]
    fn station_falls_back_to_file_name_then_stream() {
        let resolved = ResolvedStream {
            url: "https://cdn.example.org/path/jazz-live.ogg?token=1".to_owned(),
            title: None,
            homepage: Some("https://example.org/jazz".to_owned()),
        };
        assert_eq!(stream_station(&resolved).name, "jazz-live");

        let resolved = ResolvedStream {
            url: "https://cdn.example.org/".to_owned(),
            title: None,
            homepage: Some("https://example.org/".to_owned()),
        };
        assert_eq!(stream_station(&resolved).name, FALLBACK_NAME);
    }

    #[test]
    fn file_name_ignores_query_and_extension() {
        assert_eq!(
            file_name("https://x/y/My%20Radio.mp3?x=1#t").as_deref(),
            Some("My%20Radio")
        );
        assert_eq!(file_name("https://x/"), None);
        assert_eq!(file_name("https://x").as_deref(), Some("x"));
    }
}
