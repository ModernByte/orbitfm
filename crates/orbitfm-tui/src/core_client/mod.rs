//! Граница `CoreClient` — единственный способ TUI говорить с ядром.
//!
//! UI не знает о процессах и JSON; транспорт (v1 — CLI/IPC) скрыт за трейтом
//! (`docs/interfaces/16-tui.md` §2). Второго аудио-движка здесь нет.

mod cli;
pub mod parse;

pub use cli::CliCoreClient;

use std::sync::mpsc::Receiver;

use crate::model::{LibraryPayload, SearchFrame, SpectrumFrame, StatusSnapshot};

/// Поток кадров `events`.
pub type EventStream = Receiver<CoreEvent>;

/// Клиент ядра: долгоживущая подписка + one-shot команды.
pub trait CoreClient: Send {
    /// Запускает `orbitfm events` (`spectrum` добавляет `--spectrum`).
    fn start_events(&mut self, spectrum: bool) -> Result<EventStream, CoreError>;
    /// Выполняет one-shot команду.
    fn execute(&mut self, command: CoreCommand) -> Result<CoreReply, CoreError>;
}

/// Команда ядру (аргументы CLI без shell).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreCommand {
    PlayResult(String),
    Play(String),
    PlayLibrary(String),
    PlayPause,
    Stop,
    Next,
    Previous,
    Roulette,
    VolumeUp,
    VolumeDown,
    Favorite(Option<String>),
    Search(String),
    Library,
    Sleep(u16),
    ClearSleep,
    Quit,
}

impl CoreCommand {
    /// Аргументы `orbitfm …` без имени бинарника.
    pub fn args(&self) -> Vec<String> {
        match self {
            Self::PlayResult(uuid) => vec!["play-result".into(), uuid.clone()],
            Self::Play(uuid) => vec!["play".into(), uuid.clone()],
            Self::PlayLibrary(uuid) => vec!["play-library".into(), uuid.clone()],
            Self::PlayPause => vec!["playpause".into()],
            Self::Stop => vec!["stop".into()],
            Self::Next => vec!["next".into()],
            Self::Previous => vec!["previous".into()],
            Self::Roulette => vec!["roulette".into()],
            Self::VolumeUp => vec!["volume-up".into()],
            Self::VolumeDown => vec!["volume-down".into()],
            Self::Favorite(None) => vec!["favorite".into()],
            Self::Favorite(Some(uuid)) => vec!["favorite".into(), uuid.clone()],
            Self::Search(query) => vec!["search".into(), query.clone()],
            Self::Library => vec!["library".into()],
            Self::Sleep(minutes) => vec!["sleep".into(), minutes.to_string()],
            Self::ClearSleep => vec!["sleep".into(), "clear".into()],
            Self::Quit => vec!["quit".into()],
        }
    }
}

/// Ответ one-shot команды.
#[derive(Debug, Clone, PartialEq)]
pub enum CoreReply {
    Ok { text: String },
    Search { id: u64 },
    Favorite { favorite: bool },
    Library(LibraryPayload),
}

/// Кадр потока (`ev: 1`).
#[derive(Debug, Clone, PartialEq)]
pub enum CoreEvent {
    State(Box<StatusSnapshot>),
    Search(SearchFrame),
    Spectrum(SpectrumFrame),
    Keepalive,
    /// Ядро несовместимо (`ev`/`schema`/`app` не те): управление блокируется.
    Incompatible {
        message: String,
    },
    Disconnected {
        message: String,
    },
}

/// Ошибка клиента ядра.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CoreError {
    #[error("OrbitFM не запущен")]
    NotRunning,
    #[error("команда отклонена: {0}")]
    Command(String),
    #[error("некорректный ответ ядра: {0}")]
    Protocol(String),
    #[error("таймаут команды")]
    Timeout,
    #[error("не удалось запустить orbitfm: {0}")]
    Io(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_args_are_shell_free() {
        assert_eq!(CoreCommand::Play("abc".into()).args(), vec!["play", "abc"]);
        assert_eq!(CoreCommand::Sleep(30).args(), vec!["sleep", "30"]);
        assert_eq!(CoreCommand::ClearSleep.args(), vec!["sleep", "clear"]);
        assert_eq!(CoreCommand::VolumeUp.args(), vec!["volume-up"]);
        // Запрос не склеивается с аргументом и не проходит через shell.
        assert_eq!(
            CoreCommand::Search("a b; rm -rf /".into()).args(),
            vec!["search", "a b; rm -rf /"]
        );
    }
}
