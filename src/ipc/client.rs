use std::io::ErrorKind;
use std::path::Path;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::timeout;

use super::protocol::{IpcError, MAX_COMMAND_BYTES};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(12);
const MAX_RESPONSE_BYTES: usize = 64 * 1024;

/// Отправляет одну команду уже запущенному экземпляру и читает ответ до EOF.
pub(super) async fn send(path: &Path, command: &str) -> Result<String, IpcError> {
    if command.len() > MAX_COMMAND_BYTES {
        return Err(IpcError::CommandTooLong);
    }

    let mut stream = match timeout(CONNECT_TIMEOUT, UnixStream::connect(path)).await {
        Err(_) => {
            return Err(IpcError::Timeout(format!(
                "подключение к {}",
                path.display()
            )));
        }
        Ok(Err(err)) if is_not_running(&err) => return Err(IpcError::NotRunning),
        Ok(Err(err)) => {
            return Err(IpcError::Connect(format!("{}: {err}", path.display())));
        }
        Ok(Ok(stream)) => stream,
    };

    stream.write_all(command.trim_end().as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.shutdown().await?;

    let mut response = Vec::with_capacity(256);
    let mut limited = stream.take((MAX_RESPONSE_BYTES + 1) as u64);
    match timeout(RESPONSE_TIMEOUT, limited.read_to_end(&mut response)).await {
        Err(_) => Err(IpcError::Timeout("ожидание ответа".into())),
        Ok(Err(err)) => Err(IpcError::Connect(format!(
            "ответ от {}: {err}",
            path.display()
        ))),
        Ok(Ok(_)) if response.is_empty() => Err(IpcError::Connect("пустой ответ сервера".into())),
        Ok(Ok(_)) if response.len() > MAX_RESPONSE_BYTES => {
            Err(IpcError::Connect("ответ сервера слишком большой".into()))
        }
        Ok(Ok(_)) => Ok(String::from_utf8_lossy(&response).trim_end().to_owned()),
    }
}

fn is_not_running(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        ErrorKind::NotFound | ErrorKind::ConnectionRefused
    )
}
