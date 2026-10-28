//! Single-instance и IPC (ADR-0005).
//!
//! Модуль отвечает только за транспорт и передачу команд в Application Layer.
//! Бизнес-логика воспроизведения и поиска здесь отсутствует.

mod client;
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

pub use protocol::{IpcCommand, IpcError, ThemeArg, WindowModeArg};
pub use server::{IpcRequest, IpcServer, WakeFn};
