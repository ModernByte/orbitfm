use thiserror::Error;

/// Верхняя граница длины одной команды.
pub const MAX_COMMAND_BYTES: usize = 4096;

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
    /// `panel`: выпадающая панель из CLI (ADR-0033). Повторная команда
    /// при открытой панели скрывает её, из другого режима — переключает.
    Panel,
    /// `overlay`: переключатель капсулы Overlay для бар-виджета (ADR-0033).
    /// Открытый Overlay скрывается, из другого режима — переключается и показывается.
    Overlay,
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
            "status" => {
                reject_extra_args(parts.next())?;
                Ok(Self::Status)
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
            Self::Quit => "quit".to_owned(),
            Self::Volume { level } => format!("volume {level}"),
            Self::ModeQuery => "mode".to_owned(),
            Self::SetMode { mode } => format!("mode {}", mode.as_str()),
            Self::Panel => "panel".to_owned(),
            Self::Overlay => "overlay".to_owned(),
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
            IpcCommand::Raw("custom arg".to_owned()),
        ];
        for command in commands {
            assert_eq!(IpcCommand::parse(&command.to_wire()).unwrap(), command);
        }
    }
}
