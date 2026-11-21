//! Разбор кадров потока и ответов команд.
//!
//! Чистые функции без процессов: сюда приходят строки NDJSON и stdout/stderr
//! one-shot команд (`docs/interfaces/13-events.md`, `16-tui.md`).

use serde_json::{Map, Value};

use crate::core_client::{CoreCommand, CoreError, CoreEvent, CoreReply};
use crate::model::{LibraryPayload, SearchFrame, SpectrumBands, SpectrumFrame, StatusSnapshot};

/// Максимальный размер ответа `library`.
const LIBRARY_LIMIT: usize = 64 * 1024;

/// Ошибка разбора кадра.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("несовместимо: {0}")]
    Incompatible(String),
    #[error("некорректный кадр: {0}")]
    Malformed(String),
}

/// Результат разбора строки: событие или намеренно пропущенный кадр.
#[derive(Debug, Clone, PartialEq)]
pub enum Parsed {
    Event(CoreEvent),
    Ignored,
}

/// Разбор одной строки NDJSON-потока `events`.
pub fn parse_event(line: &str) -> Result<Parsed, ParseError> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(Parsed::Ignored);
    }
    let value: Value =
        serde_json::from_str(line).map_err(|error| ParseError::Malformed(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| ParseError::Malformed("кадр не объект".to_owned()))?;

    if object.get("ev").and_then(Value::as_u64) != Some(1) {
        return Err(ParseError::Incompatible("ev != 1".to_owned()));
    }

    match object.get("type").and_then(Value::as_str).unwrap_or("") {
        "snapshot" | "state" => parse_status(object),
        "search" => {
            let frame: SearchFrame = serde_json::from_value(value)
                .map_err(|error| ParseError::Malformed(error.to_string()))?;
            Ok(Parsed::Event(CoreEvent::Search(frame)))
        }
        "spectrum" => Ok(Parsed::Event(CoreEvent::Spectrum(parse_spectrum(object)?))),
        "keepalive" => Ok(Parsed::Event(CoreEvent::Keepalive)),
        _ => Ok(Parsed::Ignored),
    }
}

fn parse_status(object: &Map<String, Value>) -> Result<Parsed, ParseError> {
    let status = object
        .get("status")
        .ok_or_else(|| ParseError::Malformed("нет status".to_owned()))?;
    let snapshot: StatusSnapshot = serde_json::from_value(status.clone())
        .map_err(|error| ParseError::Malformed(error.to_string()))?;
    if !snapshot.is_compatible() {
        return Err(ParseError::Incompatible(format!(
            "schema {} / app {}",
            snapshot.schema, snapshot.app
        )));
    }
    Ok(Parsed::Event(CoreEvent::State(Box::new(snapshot))))
}

fn parse_spectrum(object: &Map<String, Value>) -> Result<SpectrumFrame, ParseError> {
    let level = number(object, "level")?;
    let flatness = number(object, "flatness")?;
    let centroid_hz = number(object, "centroid_hz")?;
    let spread_hz = number(object, "spread_hz")?;
    let flux = number(object, "flux")?;

    // Вне диапазона — кадр отбрасывается целиком (интерфейс 13 §3.5).
    if !(0.0..=1.0).contains(&level) || !(0.0..=1.0).contains(&flatness) {
        return Err(ParseError::Malformed("spectrum level/flatness".to_owned()));
    }
    if spread_hz < 0.0 || flux < 0.0 {
        return Err(ParseError::Malformed("spectrum spread/flux".to_owned()));
    }

    let bands = match object.get("bands") {
        None | Some(Value::Null) => None,
        Some(Value::Object(map)) => {
            let band = |key: &str| map.get(key).and_then(Value::as_f64).unwrap_or(0.0) as f32;
            Some(SpectrumBands {
                bass: band("bass"),
                mid: band("mid"),
                high: band("high"),
            })
        }
        Some(_) => return Err(ParseError::Malformed("spectrum.bands".to_owned())),
    };

    Ok(SpectrumFrame {
        seq: object.get("seq").and_then(Value::as_u64).unwrap_or(0),
        level,
        centroid_hz,
        spread_hz,
        flatness,
        flux,
        bands,
    })
}

fn number(object: &Map<String, Value>, key: &str) -> Result<f32, ParseError> {
    let value = object
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| ParseError::Malformed(format!("spectrum.{key}")))? as f32;
    if !value.is_finite() {
        return Err(ParseError::Malformed(format!("spectrum.{key} не конечное")));
    }
    Ok(value)
}

/// Разбор ответа `orbitfm library`.
pub fn parse_library(stdout: &str) -> Result<LibraryPayload, ParseError> {
    if stdout.len() > LIBRARY_LIMIT {
        return Err(ParseError::Malformed("library > 64 KiB".to_owned()));
    }
    serde_json::from_str(stdout).map_err(|error| ParseError::Malformed(error.to_string()))
}

/// Разбор ответа one-shot команды по коду возврата и потокам.
pub fn parse_reply(
    command: &CoreCommand,
    code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> Result<CoreReply, CoreError> {
    let stderr = stderr.trim();
    if code == Some(1) && stderr.contains("OrbitFM не запущен") {
        return Err(CoreError::NotRunning);
    }
    if code != Some(0) {
        let detail = if stderr.is_empty() {
            format!("exit {code:?}")
        } else {
            stderr.to_owned()
        };
        return Err(CoreError::Command(detail));
    }

    let stdout = stdout.trim();
    match command {
        CoreCommand::Library => parse_library(stdout)
            .map(CoreReply::Library)
            .map_err(|error| CoreError::Protocol(error.to_string())),
        CoreCommand::Search(_) => parse_search_id(stdout).map(|id| CoreReply::Search { id }),
        CoreCommand::Favorite(_) => parse_favorite(stdout),
        _ => Ok(CoreReply::Ok {
            text: stdout.to_owned(),
        }),
    }
}

fn parse_search_id(stdout: &str) -> Result<u64, CoreError> {
    stdout
        .strip_prefix("search")
        .and_then(|rest| rest.trim().parse::<u64>().ok())
        .ok_or_else(|| CoreError::Protocol(stdout.to_owned()))
}

fn parse_favorite(stdout: &str) -> Result<CoreReply, CoreError> {
    match stdout.split_whitespace().last() {
        Some("true") => Ok(CoreReply::Favorite { favorite: true }),
        Some("false") => Ok(CoreReply::Favorite { favorite: false }),
        _ => Err(CoreError::Protocol(stdout.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PlayState;

    const SNAPSHOT: &str = r#"{"ev":1,"type":"snapshot","status":{"schema":1,"app":"orbitfm","version":"0.23.0","state":"playing","station":{"uuid":"u1","name":"Radio","country":"DE","countrycode":"de","bitrate":128,"codec":"MP3","source":"radio","favorite":true},"volume":0.7,"can":{"previous":true,"next":false},"sleep":null}}"#;

    #[test]
    fn parses_snapshot() {
        let Parsed::Event(CoreEvent::State(snapshot)) = parse_event(SNAPSHOT).expect("ok") else {
            panic!("expected state");
        };
        assert_eq!(snapshot.play_state(), PlayState::Playing);
        assert_eq!(snapshot.volume_percent(), 70);
        assert_eq!(
            snapshot.current_station().map(|s| s.name.as_str()),
            Some("Radio")
        );
        assert!(snapshot.can.previous);
        assert!(!snapshot.can.next);
    }

    #[test]
    fn keepalive_and_unknown_types() {
        assert_eq!(
            parse_event(r#"{"ev":1,"type":"keepalive"}"#),
            Ok(Parsed::Event(CoreEvent::Keepalive))
        );
        assert_eq!(
            parse_event(r#"{"ev":1,"type":"something-new","x":1}"#),
            Ok(Parsed::Ignored)
        );
    }

    #[test]
    fn wrong_envelope_is_incompatible() {
        assert!(matches!(
            parse_event(r#"{"ev":2,"type":"state","status":{"schema":1,"app":"orbitfm"}}"#),
            Err(ParseError::Incompatible(_))
        ));
    }

    #[test]
    fn wrong_schema_is_incompatible() {
        let frame =
            r#"{"ev":1,"type":"state","status":{"schema":2,"app":"orbitfm","state":"playing"}}"#;
        assert!(matches!(
            parse_event(frame),
            Err(ParseError::Incompatible(_))
        ));
    }

    #[test]
    fn malformed_json_is_error() {
        assert!(matches!(
            parse_event("{not json"),
            Err(ParseError::Malformed(_))
        ));
        assert_eq!(parse_event("   "), Ok(Parsed::Ignored));
    }

    #[test]
    fn parses_search_frame() {
        let frame = r#"{"ev":1,"type":"search","id":7,"query":"lofi","results":[{"uuid":"u","name":"N","country":"DE","countrycode":"DE","bitrate":128,"source":"radio"}],"error":null}"#;
        let Parsed::Event(CoreEvent::Search(search)) = parse_event(frame).expect("ok") else {
            panic!("expected search");
        };
        assert_eq!(search.id, 7);
        assert_eq!(search.results.len(), 1);
    }

    #[test]
    fn spectrum_valid_dropped_and_nan() {
        let ok = r#"{"ev":1,"type":"spectrum","seq":3,"level":0.6,"centroid_hz":1700.0,"spread_hz":1600.0,"flatness":0.4,"flux":0.001,"bands":{"bass":0.7,"mid":0.5,"high":0.3}}"#;
        let Parsed::Event(CoreEvent::Spectrum(frame)) = parse_event(ok).expect("ok") else {
            panic!("expected spectrum");
        };
        assert_eq!(frame.seq, 3);
        assert!(frame.bands.is_some());

        // level вне диапазона — кадр отбрасывается.
        let bad = r#"{"ev":1,"type":"spectrum","seq":1,"level":2.0,"centroid_hz":1.0,"spread_hz":1.0,"flatness":0.1,"flux":0.0}"#;
        assert!(matches!(parse_event(bad), Err(ParseError::Malformed(_))));
    }

    #[test]
    fn reply_parsing() {
        assert_eq!(
            parse_reply(&CoreCommand::Search("q".into()), Some(0), "search 42\n", ""),
            Ok(CoreReply::Search { id: 42 })
        );
        assert_eq!(
            parse_reply(&CoreCommand::Favorite(None), Some(0), "favorite true\n", ""),
            Ok(CoreReply::Favorite { favorite: true })
        );
        assert_eq!(
            parse_reply(&CoreCommand::Stop, Some(0), "ok\n", ""),
            Ok(CoreReply::Ok {
                text: "ok".to_owned()
            })
        );
        assert_eq!(
            parse_reply(&CoreCommand::Stop, Some(1), "", "OrbitFM не запущен\n"),
            Err(CoreError::NotRunning)
        );
        assert_eq!(
            parse_reply(&CoreCommand::Stop, Some(1), "", "boom\n"),
            Err(CoreError::Command("boom".to_owned()))
        );
    }

    #[test]
    fn library_limit_and_shape() {
        let payload = r#"{"favorites":[{"uuid":"u","name":"N"}],"history":[]}"#;
        let parsed = parse_library(payload).expect("ok");
        assert_eq!(parsed.favorites.len(), 1);
        assert!(parsed.history.is_empty());
        let big = "x".repeat(LIBRARY_LIMIT + 1);
        assert!(matches!(parse_library(&big), Err(ParseError::Malformed(_))));
    }
}
