//! Single-instance и IPC (ADR-0005).
//!
//! Модуль отвечает только за транспорт и передачу команд в Application Layer.
//! Бизнес-логика воспроизведения и поиска здесь отсутствует.

mod client;
mod events;
#[cfg(windows)]
mod pipe;
mod protocol;
mod server;
#[cfg(unix)]
mod socket;

#[cfg(windows)]
use pipe as transport;
#[cfg(unix)]
use socket as transport;

pub(crate) use events::{SearchFeed, StatusFeed};
pub use protocol::{IpcCommand, IpcError, ThemeArg, WindowModeArg};
pub(crate) use server::EventSubscription;
pub use server::{IpcRequest, IpcServer, WakeFn};
