use std::io::ErrorKind;
use std::io::Write as _;
use std::path::Path;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::timeout;

use super::protocol::{IpcError, MAX_COMMAND_BYTES};
use super::transport;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(12);
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const SUBSCRIPTION_QUEUE: usize = 64;

/// Отправляет одну команду уже запущенному экземпляру и читает ответ до EOF.
pub(super) async fn send(path: &Path, command: &str) -> Result<String, IpcError> {
    if command.len() > MAX_COMMAND_BYTES {
        return Err(IpcError::CommandTooLong);
    }

    let mut stream = open(path).await?;

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

/// Подписка на поток событий: одна команда `events`, затем печать кадров до
/// EOF (ADR-0039). Возвращает ошибку, если сервер отклонил подписку.
pub(super) async fn subscribe(path: &Path) -> Result<(), IpcError> {
    let mut subscription = subscribe_frames(path).await?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    while let Some(line) = subscription.lines.recv().await {
        let line = line?;
        writeln!(out, "{line}").map_err(IpcError::Io)?;
        out.flush().map_err(IpcError::Io)?;
    }
    Ok(())
}

/// Кадры подписки `events` для передачи другому потребителю (native host).
pub(crate) struct Subscription {
    pub(crate) lines: mpsc::Receiver<Result<String, IpcError>>,
    task: JoinHandle<()>,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Открывает подписку и отдаёт кадры каналом. Первый кадр читается здесь же,
/// поэтому отказ сервера (`error …`) возвращается как ошибка, а не как событие.
pub(super) async fn subscribe_frames(path: &Path) -> Result<Subscription, IpcError> {
    let mut stream = open(path).await?;
    stream.write_all(b"events\n").await?;
    stream.shutdown().await?;

    let mut lines = BufReader::new(stream).lines();
    let first = lines
        .next_line()
        .await
        .map_err(|err| IpcError::Connect(format!("поток событий {}: {err}", path.display())))?
        .ok_or_else(|| IpcError::Connect("поток событий закрыт без первого кадра".into()))?;
    if first.starts_with("error") {
        return Err(IpcError::Subscription(first));
    }

    let (sender, receiver) = mpsc::channel(SUBSCRIPTION_QUEUE);
    let path = path.to_owned();
    let task = tokio::spawn(async move {
        if sender.send(Ok(first)).await.is_err() {
            return;
        }
        loop {
            let frame = match lines.next_line().await {
                Ok(Some(line)) => Ok(line),
                Ok(None) => break,
                Err(err) => {
                    let error =
                        IpcError::Connect(format!("поток событий {}: {err}", path.display()));
                    let _ = sender.send(Err(error)).await;
                    break;
                }
            };
            let failed = frame.is_err();
            if sender.send(frame).await.is_err() || failed {
                break;
            }
        }
    });

    Ok(Subscription {
        lines: receiver,
        task,
    })
}

async fn open(path: &Path) -> Result<transport::ClientStream, IpcError> {
    match timeout(CONNECT_TIMEOUT, transport::connect(path)).await {
        Err(_) => Err(IpcError::Timeout(format!(
            "подключение к {}",
            path.display()
        ))),
        Ok(Err(err)) if is_not_running(&err) => Err(IpcError::NotRunning),
        Ok(Err(err)) => Err(IpcError::Connect(format!("{}: {err}", path.display()))),
        Ok(Ok(stream)) => Ok(stream),
    }
}

fn is_not_running(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        ErrorKind::NotFound | ErrorKind::ConnectionRefused
    )
}
