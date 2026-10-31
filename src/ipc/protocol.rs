use thiserror::Error;

/// Верхняя граница длины одной команды.
pub const MAX_COMMAND_BYTES: usize = 4096;
/// Предел поискового запроса companion-попапа (ADR-0038, ревизия 0.16).
pub const MAX_SEARCH_CHARS: usize = 128;
/// Предел длины URL в `play-url`: длиннее не нужен ни одному каталогу.
pub const MAX_PLAY_URL_BYTES: usize = 2048;

/// Аргумент глагола `mode` — целевой режим окна (ADR-0021 п. 5).
///
/// Wire-тип живёт в инфраструктуре; в доменное `WindowMode` его превращает
/// Application Core (`src/session/commands.rs`), потому что направление
/// зависимостей — Core → инфраструктура, а не наоборот.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowModeArg {
    Focus,
    Overlay,
    Listening,
    Panel,
}

impl WindowModeArg {
    /// Имя режима в протоколе.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Focus => "focus",
            Self::Overlay => "overlay",
            Self::Listening => "listening",
            Self::Panel => "panel",
        }
    }

    fn parse(raw: &str) -> Result<Self, IpcError> {
        match raw {
            "focus" => Ok(Self::Focus),
            "overlay" => Ok(Self::Overlay),
            "listening" => Ok(Self::Listening),
            "panel" => Ok(Self::Panel),
            other => Err(IpcError::InvalidArgument(format!(
                "mode: ожидаются focus, overlay, listening или panel, получено {other}"
            ))),
        }
    }
}

/// Аргумент глагола `theme` — целевая тема оформления (ADR-0037).
///
/// Wire-тип живёт в инфраструктуре; в доменное `ThemeMode` его превращает
/// Application Core (`src/session/commands.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeArg {
    System,
    Dark,
    Light,
}

impl ThemeArg {
    /// Имя темы в протоколе.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    fn parse(raw: &str) -> Result<Self, IpcError> {
        match raw {
            "system" => Ok(Self::System),
            "dark" => Ok(Self::Dark),
            "light" => Ok(Self::Light),
            other => Err(IpcError::InvalidArgument(format!(
                "theme: ожидаются system, dark или light, получено {other}"
            ))),
        }
    }
}

/// Команды IPC-протокола (docs/interfaces/05-ipc.md).
#[derive(Debug, Clone, PartialEq)]
pub enum IpcCommand {
    Toggle,
    Show,
    Hide,
    Play {
        station_uuid: String,
    },
    Stop,
    Status,
    /// `status --json`: машиночитаемый снимок для интеграций (ADR-0037).
    StatusJson,
    /// `events`: поток снимков состояния NDJSON (ADR-0039).
    Events,
    Quit,
    Volume {
        level: f64,
    },
    /// `mode` без аргумента: спросить текущий режим.
    ModeQuery,
    /// `mode <режим>`: переключить режим окна (ADR-0021).
    SetMode {
        mode: WindowModeArg,
    },
    /// `panel`: выпадающая панель бар-виджета (ADR-0033). Повторная команда
    /// при открытой панели скрывает её, из другого режима — переключает.
    Panel,
    /// `overlay`: переключатель капсулы Overlay из CLI (ADR-0033).
    /// Открытый Overlay скрывается, из другого режима — переключается и показывается.
    Overlay,
    /// `playpause`: переключить воспроизведение (ADR-0037).
    PlayPause,
    /// `next`: следующая станция видимой очереди.
    Next,
    /// `previous`: предыдущая станция видимой очереди.
    Previous,
    /// `roulette`: запустить радиорулетку.
    Roulette,
    /// `theme <system|dark|light>`: сменить тему оформления.
    Theme {
        mode: ThemeArg,
    },
    /// `settings`: открыть окно настроек; при необходимости показывает окно.
    Settings,
    /// `volume-up`: громкость на один шаг вверх.
    VolumeUp,
    /// `volume-down`: громкость на один шаг вниз.
    VolumeDown,
    /// `search <запрос>`: поиск для companion-попапа; ответ — кадр `search`
    /// в потоке событий (ADR-0038, ревизия 0.16).
    Search {
        query: String,
    },
    /// `library`: избранное и история для попапа.
    Library,
    /// `favorite [uuid]`: переключить избранное текущей станции или станции из
    /// выдачи/библиотеки.
    Favorite {
        station_uuid: Option<String>,
    },
    /// `play-result <uuid>`: играть станцию только из последней выдачи попапа.
    PlayResult {
        station_uuid: String,
    },
    /// `play-library <uuid>`: играть станцию только из избранного или истории.
    PlayLibrary {
        station_uuid: String,
    },
    /// `play-url <url>`: импорт внешнего потока (ADR-0040), только явный вызов.
    PlayUrl {
        url: String,
    },
    /// Неизвестная команда сохраняется для расширения протокола.
    Raw(String),
}

impl IpcCommand {
    /// Разбирает одну строку протокола (без завершающего перевода строки).
    pub fn parse(line: &str) -> Result<Self, IpcError> {
        let line = line.trim_end_matches(['\r', '\n']).trim();
        if line.is_empty() {
            return Err(IpcError::InvalidArgument("пустая команда".into()));
        }
        if line.len() > MAX_COMMAND_BYTES {
            return Err(IpcError::CommandTooLong);
        }

        let mut parts = line.split_whitespace();
        let name = parts.next().unwrap_or_default();

        match name {
            "toggle" => {
                reject_extra_args(parts.next())?;
                Ok(Self::Toggle)
            }
            "show" => {
                reject_extra_args(parts.next())?;
                Ok(Self::Show)
            }
            "hide" => {
                reject_extra_args(parts.next())?;
                Ok(Self::Hide)
            }
            "stop" => {
                reject_extra_args(parts.next())?;
                Ok(Self::Stop)
            }
            "status" => match parts.next() {
                None => Ok(Self::Status),
                Some("--json") => {
                    reject_extra_args(parts.next())?;
                    Ok(Self::StatusJson)
                }
                Some(other) => Err(IpcError::InvalidArgument(format!(
                    "status: ожидается --json, получено {other}"
                ))),
            },
            "events" => {
                reject_extra_args(parts.next())?;
                Ok(Self::Events)
            }
            "playpause" => {
                reject_extra_args(parts.next())?;
                Ok(Self::PlayPause)
            }
            "next" => {
                reject_extra_args(parts.next())?;
                Ok(Self::Next)
            }
            "previous" => {
                reject_extra_args(parts.next())?;
                Ok(Self::Previous)
            }
            "roulette" => {
                reject_extra_args(parts.next())?;
                Ok(Self::Roulette)
            }
            "theme" => {
                let raw = parts.next().ok_or_else(|| {
                    IpcError::InvalidArgument("theme: ожидается system, dark или light".into())
                })?;
                let mode = ThemeArg::parse(raw)?;
                reject_extra_args(parts.next())?;
                Ok(Self::Theme { mode })
            }
            "settings" => {
                reject_extra_args(parts.next())?;
                Ok(Self::Settings)
            }
            "volume-up" => {
                reject_extra_args(parts.next())?;
                Ok(Self::VolumeUp)
            }
            "volume-down" => {
                reject_extra_args(parts.next())?;
                Ok(Self::VolumeDown)
            }
            "search" => {
                let query = rest_of_line(line, name);
                if query.is_empty() {
                    return Err(IpcError::InvalidArgument(
                        "search: ожидается поисковый запрос".into(),
                    ));
                }
                if query.chars().count() > MAX_SEARCH_CHARS {
                    return Err(IpcError::InvalidArgument(format!(
                        "search: запрос длиннее {MAX_SEARCH_CHARS} символов"
                    )));
                }
                Ok(Self::Search {
                    query: query.to_owned(),
                })
            }
            "library" => {
                reject_extra_args(parts.next())?;
                Ok(Self::Library)
            }
            "favorite" => {
                let station_uuid = parts.next().map(str::to_owned);
                reject_extra_args(parts.next())?;
                Ok(Self::Favorite { station_uuid })
            }
            "play-result" => {
                let station_uuid = required_uuid(parts.next(), "play-result")?;
                reject_extra_args(parts.next())?;
                Ok(Self::PlayResult { station_uuid })
            }
            "play-library" => {
                let station_uuid = required_uuid(parts.next(), "play-library")?;
                reject_extra_args(parts.next())?;
                Ok(Self::PlayLibrary { station_uuid })
            }
            "play-url" => {
                let url = parse_play_url(line, name)?;
                Ok(Self::PlayUrl { url })
            }
            "quit" => {
                reject_extra_args(parts.next())?;
                Ok(Self::Quit)
            }
            "panel" => {
                reject_extra_args(parts.next())?;
                Ok(Self::Panel)
            }
            "overlay" => {
                reject_extra_args(parts.next())?;
                Ok(Self::Overlay)
            }
            "mode" => match parts.next() {
                None => Ok(Self::ModeQuery),
                Some(raw) => {
                    let mode = WindowModeArg::parse(raw)?;
                    reject_extra_args(parts.next())?;
                    Ok(Self::SetMode { mode })
                }
            },
            "play" => {
                let station_uuid = parts.next().ok_or_else(|| {
                    IpcError::InvalidArgument("play: ожидается stationuuid".into())
                })?;
                reject_extra_args(parts.next())?;
                Ok(Self::Play {
                    station_uuid: station_uuid.to_owned(),
                })
            }
            "volume" => {
                let raw = parts.next().ok_or_else(|| {
                    IpcError::InvalidArgument("volume: ожидается 0.0..=1.0".into())
                })?;
                let level = raw.parse::<f64>().map_err(|_| {
                    IpcError::InvalidArgument(format!("volume: некорректное число: {raw}"))
                })?;
                if !level.is_finite() || !(0.0..=1.0).contains(&level) {
                    return Err(IpcError::InvalidArgument(
                        "volume: значение должно быть в диапазоне 0.0..=1.0".into(),
                    ));
                }
                reject_extra_args(parts.next())?;
                Ok(Self::Volume { level })
            }
            _ => Ok(Self::Raw(line.to_owned())),
        }
    }

    /// Возвращает строковое представление для отправки другому экземпляру.
    pub fn to_wire(&self) -> String {
        match self {
            Self::Toggle => "toggle".to_owned(),
            Self::Show => "show".to_owned(),
            Self::Hide => "hide".to_owned(),
            Self::Play { station_uuid } => format!("play {station_uuid}"),
            Self::Stop => "stop".to_owned(),
            Self::Status => "status".to_owned(),
            Self::StatusJson => "status --json".to_owned(),
            Self::Events => "events".to_owned(),
            Self::Quit => "quit".to_owned(),
            Self::Volume { level } => format!("volume {level}"),
            Self::ModeQuery => "mode".to_owned(),
            Self::SetMode { mode } => format!("mode {}", mode.as_str()),
            Self::Panel => "panel".to_owned(),
            Self::Overlay => "overlay".to_owned(),
            Self::PlayPause => "playpause".to_owned(),
            Self::Next => "next".to_owned(),
            Self::Previous => "previous".to_owned(),
            Self::Roulette => "roulette".to_owned(),
            Self::Theme { mode } => format!("theme {}", mode.as_str()),
            Self::Settings => "settings".to_owned(),
            Self::VolumeUp => "volume-up".to_owned(),
            Self::VolumeDown => "volume-down".to_owned(),
            Self::Search { query } => format!("search {query}"),
            Self::Library => "library".to_owned(),
            Self::Favorite { station_uuid } => match station_uuid {
                Some(uuid) => format!("favorite {uuid}"),
                None => "favorite".to_owned(),
            },
            Self::PlayResult { station_uuid } => format!("play-result {station_uuid}"),
            Self::PlayLibrary { station_uuid } => format!("play-library {station_uuid}"),
            Self::PlayUrl { url } => format!("play-url {url}"),
            Self::Raw(raw) => raw.clone(),
        }
    }
}

fn reject_extra_args(extra: Option<&str>) -> Result<(), IpcError> {
    match extra {
        Some(value) => Err(IpcError::InvalidArgument(format!(
            "лишний аргумент: {value}"
        ))),
        None => Ok(()),
    }
}

/// Остаток строки после имени команды: запрос поиска может содержать пробелы.
fn rest_of_line<'a>(line: &'a str, name: &str) -> &'a str {
    line[name.len()..].trim()
}

fn required_uuid(value: Option<&str>, verb: &str) -> Result<String, IpcError> {
    let uuid =
        value.ok_or_else(|| IpcError::InvalidArgument(format!("{verb}: ожидается stationuuid")))?;
    if uuid.chars().any(char::is_whitespace) {
        return Err(IpcError::InvalidArgument(format!(
            "{verb}: stationuuid не содержит пробелов"
        )));
    }
    Ok(uuid.to_owned())
}

/// `play-url` принимает ровно один `http(s)`-адрес без пробелов: остальное
/// отклоняется до сетевого резолвера (ADR-0040).
fn parse_play_url(line: &str, name: &str) -> Result<String, IpcError> {
    let rest = rest_of_line(line, name);
    if rest.is_empty() {
        return Err(IpcError::InvalidArgument(
            "play-url: ожидается http(s)-URL".into(),
        ));
    }
    if rest.split_whitespace().count() != 1 {
        return Err(IpcError::InvalidArgument(
            "play-url: URL не содержит пробелов".into(),
        ));
    }
    if rest.len() > MAX_PLAY_URL_BYTES {
        return Err(IpcError::InvalidArgument(format!(
            "play-url: URL длиннее {MAX_PLAY_URL_BYTES} байт"
        )));
    }
    let lower = rest.to_ascii_lowercase();
    let host = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"));
    match host {
        Some(host) if !host.is_empty() => Ok(rest.to_owned()),
        _ => Err(IpcError::InvalidArgument(
            "play-url: поддерживаются только http:// и https://".into(),
        )),
    }
}

/// Ошибки IPC-подсистемы.
#[derive(Debug, Error)]
pub enum IpcError {
    #[error("экземпляр уже запущен")]
    AlreadyRunning,
    #[error("OrbitFM не запущен")]
    NotRunning,
    #[error("не удалось подключиться к существующему экземпляру: {0}")]
    Connect(String),
    #[error("ошибка сокета: {0}")]
    Io(#[from] std::io::Error),
    #[error("некорректный аргумент команды: {0}")]
    InvalidArgument(String),
    #[error("превышен допустимый размер команды")]
    CommandTooLong,
    #[error("таймаут IPC: {0}")]
    Timeout(String),
    #[error("сервер отклонил подписку: {0}")]
    Subscription(String),
    #[error("не удалось определить путь сокета: {0}")]
    #[cfg_attr(
        windows,
        allow(dead_code, reason = "путь сокета проверяется только Unix-транспортом")
    )]
    SocketPath(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_commands() {
        assert_eq!(IpcCommand::parse("toggle").unwrap(), IpcCommand::Toggle);
        assert_eq!(IpcCommand::parse("show\n").unwrap(), IpcCommand::Show);
        assert_eq!(IpcCommand::parse("status").unwrap(), IpcCommand::Status);
        assert_eq!(IpcCommand::parse("quit").unwrap(), IpcCommand::Quit);
    }

    #[test]
    fn parses_status_json() {
        assert_eq!(
            IpcCommand::parse("status --json").unwrap(),
            IpcCommand::StatusJson
        );
        assert_eq!(IpcCommand::StatusJson.to_wire(), "status --json");
        assert!(IpcCommand::parse("status --xml").is_err());
        assert!(IpcCommand::parse("status --json extra").is_err());
    }

    #[test]
    fn parses_events_subscription() {
        assert_eq!(IpcCommand::parse("events").unwrap(), IpcCommand::Events);
        assert_eq!(IpcCommand::Events.to_wire(), "events");
        assert!(IpcCommand::parse("events now").is_err());
    }

    #[test]
    fn parses_playback_and_theme_commands() {
        for (wire, command) in [
            ("playpause", IpcCommand::PlayPause),
            ("next", IpcCommand::Next),
            ("previous", IpcCommand::Previous),
            ("roulette", IpcCommand::Roulette),
            ("settings", IpcCommand::Settings),
            ("volume-up", IpcCommand::VolumeUp),
            ("volume-down", IpcCommand::VolumeDown),
        ] {
            assert_eq!(IpcCommand::parse(wire).unwrap(), command, "{wire}");
            assert_eq!(command.to_wire(), wire);
        }
        assert_eq!(
            IpcCommand::parse("theme dark").unwrap(),
            IpcCommand::Theme {
                mode: ThemeArg::Dark
            }
        );
        assert_eq!(
            IpcCommand::parse("theme light").unwrap(),
            IpcCommand::Theme {
                mode: ThemeArg::Light
            }
        );
        assert_eq!(
            IpcCommand::parse("theme system").unwrap(),
            IpcCommand::Theme {
                mode: ThemeArg::System
            }
        );
    }

    #[test]
    fn rejects_invalid_theme_arguments() {
        assert!(IpcCommand::parse("theme").is_err());
        assert!(IpcCommand::parse("theme sepia").is_err());
        assert!(IpcCommand::parse("theme Dark").is_err());
        assert!(IpcCommand::parse("theme dark extra").is_err());
        assert!(IpcCommand::parse("playpause now").is_err());
    }

    #[test]
    fn parses_play_and_volume() {
        assert_eq!(
            IpcCommand::parse("play 9604b0f1-aaaa").unwrap(),
            IpcCommand::Play {
                station_uuid: "9604b0f1-aaaa".to_owned()
            }
        );
        assert_eq!(
            IpcCommand::parse("volume 0.25").unwrap(),
            IpcCommand::Volume { level: 0.25 }
        );
    }

    #[test]
    fn rejects_invalid_volume() {
        assert!(IpcCommand::parse("volume 1.5").is_err());
        assert!(IpcCommand::parse("volume -0.1").is_err());
        assert!(IpcCommand::parse("volume NaN").is_err());
        assert!(IpcCommand::parse("volume").is_err());
    }

    #[test]
    fn rejects_extra_arguments() {
        assert!(IpcCommand::parse("toggle now").is_err());
        assert!(IpcCommand::parse("play").is_err());
    }

    #[test]
    fn parses_mode_as_query_or_switch() {
        assert_eq!(IpcCommand::parse("mode").unwrap(), IpcCommand::ModeQuery);
        assert_eq!(
            IpcCommand::parse("mode overlay").unwrap(),
            IpcCommand::SetMode {
                mode: WindowModeArg::Overlay
            }
        );
        assert_eq!(
            IpcCommand::parse("mode panel").unwrap(),
            IpcCommand::SetMode {
                mode: WindowModeArg::Panel
            }
        );
        assert!(IpcCommand::parse("mode fullscreen").is_err());
        assert!(IpcCommand::parse("mode focus extra").is_err());
    }

    #[test]
    fn parses_panel_toggle() {
        assert_eq!(IpcCommand::parse("panel").unwrap(), IpcCommand::Panel);
        assert_eq!(IpcCommand::Panel.to_wire(), "panel");
        assert!(IpcCommand::parse("panel extra").is_err());
    }

    #[test]
    fn parses_overlay_toggle() {
        assert_eq!(
            IpcCommand::parse("overlay").unwrap(),
            IpcCommand::Overlay,
            "голая команда не путается с `mode overlay`"
        );
        assert_eq!(IpcCommand::Overlay.to_wire(), "overlay");
        assert!(IpcCommand::parse("overlay extra").is_err());
    }

    #[test]
    fn parses_companion_search_with_spaces() {
        assert_eq!(
            IpcCommand::parse("search  jazz   lounge ").unwrap(),
            IpcCommand::Search {
                query: "jazz   lounge".to_owned()
            }
        );
        assert_eq!(
            IpcCommand::Search {
                query: "lofi beats".to_owned()
            }
            .to_wire(),
            "search lofi beats"
        );
        assert!(IpcCommand::parse("search").is_err());
        assert!(IpcCommand::parse("search   ").is_err());
        let long = "я".repeat(MAX_SEARCH_CHARS + 1);
        assert!(IpcCommand::parse(&format!("search {long}")).is_err());
        let limit = "я".repeat(MAX_SEARCH_CHARS);
        assert!(IpcCommand::parse(&format!("search {limit}")).is_ok());
    }

    #[test]
    fn parses_library_and_favorite() {
        assert_eq!(IpcCommand::parse("library").unwrap(), IpcCommand::Library);
        assert_eq!(IpcCommand::Library.to_wire(), "library");
        assert!(IpcCommand::parse("library now").is_err());

        assert_eq!(
            IpcCommand::parse("favorite").unwrap(),
            IpcCommand::Favorite { station_uuid: None }
        );
        assert_eq!(
            IpcCommand::parse("favorite uuid-1").unwrap(),
            IpcCommand::Favorite {
                station_uuid: Some("uuid-1".to_owned())
            }
        );
        assert_eq!(
            IpcCommand::Favorite {
                station_uuid: Some("uuid-1".to_owned())
            }
            .to_wire(),
            "favorite uuid-1"
        );
        assert!(IpcCommand::parse("favorite a b").is_err());
    }

    #[test]
    fn parses_play_result_and_play_library() {
        assert_eq!(
            IpcCommand::parse("play-result uuid-1").unwrap(),
            IpcCommand::PlayResult {
                station_uuid: "uuid-1".to_owned()
            }
        );
        assert_eq!(
            IpcCommand::parse("play-library uuid-2").unwrap(),
            IpcCommand::PlayLibrary {
                station_uuid: "uuid-2".to_owned()
            }
        );
        assert!(IpcCommand::parse("play-result").is_err());
        assert!(IpcCommand::parse("play-library").is_err());
        assert!(IpcCommand::parse("play-result a b").is_err());
    }

    #[test]
    fn parses_play_url_strictly() {
        assert_eq!(
            IpcCommand::parse("play-url https://example.org/stream.m3u").unwrap(),
            IpcCommand::PlayUrl {
                url: "https://example.org/stream.m3u".to_owned()
            }
        );
        assert_eq!(
            IpcCommand::parse("play-url HTTP://127.0.0.1:8080/a.mp3").unwrap(),
            IpcCommand::PlayUrl {
                url: "HTTP://127.0.0.1:8080/a.mp3".to_owned()
            }
        );
        assert!(IpcCommand::parse("play-url").is_err());
        assert!(IpcCommand::parse("play-url file:///etc/passwd").is_err());
        assert!(IpcCommand::parse("play-url example.org/stream").is_err());
        assert!(IpcCommand::parse("play-url https://example.org/a b").is_err());
        assert!(IpcCommand::parse("play-url https://").is_err());
    }

    #[test]
    fn keeps_unknown_commands_as_raw() {
        assert_eq!(
            IpcCommand::parse("future-command payload").unwrap(),
            IpcCommand::Raw("future-command payload".to_owned())
        );
    }

    #[test]
    fn round_trips_through_wire_format() {
        let commands = [
            IpcCommand::Toggle,
            IpcCommand::Play {
                station_uuid: "uuid".to_owned(),
            },
            IpcCommand::Volume { level: 0.5 },
            IpcCommand::ModeQuery,
            IpcCommand::SetMode {
                mode: WindowModeArg::Listening,
            },
            IpcCommand::StatusJson,
            IpcCommand::Events,
            IpcCommand::PlayPause,
            IpcCommand::Next,
            IpcCommand::Previous,
            IpcCommand::Roulette,
            IpcCommand::Theme {
                mode: ThemeArg::Dark,
            },
            IpcCommand::Settings,
            IpcCommand::VolumeUp,
            IpcCommand::VolumeDown,
            IpcCommand::Search {
                query: "jazz lounge".to_owned(),
            },
            IpcCommand::Library,
            IpcCommand::Favorite {
                station_uuid: Some("uuid".to_owned()),
            },
            IpcCommand::PlayResult {
                station_uuid: "uuid".to_owned(),
            },
            IpcCommand::PlayLibrary {
                station_uuid: "uuid".to_owned(),
            },
            IpcCommand::PlayUrl {
                url: "https://example.org/live.m3u".to_owned(),
            },
            IpcCommand::Raw("custom arg".to_owned()),
        ];
        for command in commands {
            assert_eq!(IpcCommand::parse(&command.to_wire()).unwrap(), command);
        }
    }
}
