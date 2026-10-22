//! Single-instance и IPC (ADR-0005).
//!
//! Модуль отвечает только за транспорт и передачу команд в Application Layer.
//! Бизнес-логика воспроизведения и поиска здесь отсутствует.

mod client;
mod protocol;
mod server;
mod socket;

pub use protocol::{IpcCommand, IpcError};
pub use server::{IpcRequest, IpcServer, WakeFn};
