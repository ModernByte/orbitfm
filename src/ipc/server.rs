use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Semaphore, mpsc, oneshot};
use tokio::task::JoinSet;
use tokio::time::timeout;
use tracing::warn;

use super::client;
use super::protocol::{IpcCommand, IpcError, MAX_COMMAND_BYTES};
use super::socket::{self, SocketGuard};

const COMMAND_QUEUE: usize = 64;
const MAX_CONNECTIONS: usize = 64;
const READ_TIMEOUT: Duration = Duration::from_secs(2);
const ENQUEUE_TIMEOUT: Duration = Duration::from_secs(1);
const APP_REPLY_TIMEOUT: Duration = Duration::from_secs(10);
const DRAIN_TIMEOUT: Duration = Duration::from_secs(1);

/// Пробуждает GUI-цикл после постановки команды в очередь.
pub type WakeFn = Arc<dyn Fn() + Send + Sync>;

/// Запрос, доставленный в Application Layer. Ответ отправляется клиенту
/// после фактической обработки команды.
pub struct IpcRequest {
    command: IpcCommand,
    reply: oneshot::Sender<String>,
}

impl IpcRequest {
    pub fn command(&self) -> &IpcCommand {
        &self.command
    }

    /// Возвращает `false`, если клиент уже отключился.
    pub fn try_respond(self, response: impl Into<String>) -> bool {
        self.reply.send(response.into()).is_ok()
    }
}

/// Серверная часть IPC: занятый сокет и очередь запросов для App.
pub struct IpcServer {
    listener: UnixListener,
    _socket_guard: SocketGuard,
    sender: mpsc::Sender<IpcRequest>,
    receiver: Option<mpsc::Receiver<IpcRequest>>,
}

impl IpcServer {
    /// Занимает сокет по умолчанию.
    pub async fn try_bind() -> Result<Self, IpcError> {
        let path = socket::default_path()?;
        Self::try_bind_at(&path).await
    }

    pub(super) async fn try_bind_at(path: &Path) -> Result<Self, IpcError> {
        let (listener, socket_guard) = socket::bind(path).await?;
        let (sender, receiver) = mpsc::channel(COMMAND_QUEUE);
        Ok(Self {
            listener,
            _socket_guard: socket_guard,
            sender,
            receiver: Some(receiver),
        })
    }

    /// Забирает очередь входящих запросов. Вызывается один раз при старте App.
    pub fn commands(&mut self) -> mpsc::Receiver<IpcRequest> {
        self.receiver
            .take()
            .expect("commands() вызывается один раз при старте приложения")
    }

    /// Отправляет команду уже работающему экземпляру.
    pub async fn send_to_existing(command: &str) -> Result<String, IpcError> {
        let path = socket::default_path()?;
        client::send(&path, command).await
    }

    /// Принимает соединения до сигнала shutdown, затем дожидается ответов.
    pub async fn run(self, wake: WakeFn, mut shutdown: oneshot::Receiver<()>) {
        let Self {
            listener,
            _socket_guard,
            sender,
            receiver: _,
        } = self;

        let permits = Arc::new(Semaphore::new(MAX_CONNECTIONS));
        let mut connections = JoinSet::new();

        loop {
            tokio::select! {
                biased;
                _ = &mut shutdown => break,
                accepted = listener.accept() => {
                    let (stream, _addr) = match accepted {
                        Ok(accepted) => accepted,
                        Err(err) => {
                            warn!(error = %err, "не удалось принять IPC-соединение");
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            continue;
                        }
                    };

                    let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else {
                        warn!("слишком много одновременных IPC-соединений");
                        drop(stream);
                        continue;
                    };

                    let sender = sender.clone();
                    let wake = Arc::clone(&wake);
                    connections.spawn(async move {
                        let _permit = permit;
                        if let Err(err) = handle_connection(stream, sender, wake).await {
                            warn!(error = %err, "IPC-соединение завершилось с ошибкой");
                        }
                    });
                }
            }
        }

        drop(listener);

        let drain = async { while connections.join_next().await.is_some() {} };
        if timeout(DRAIN_TIMEOUT, drain).await.is_err() {
            warn!("не все IPC-соединения завершились, принудительная остановка");
            connections.shutdown().await;
        }
    }
}

async fn handle_connection(
    stream: UnixStream,
    sender: mpsc::Sender<IpcRequest>,
    wake: WakeFn,
) -> Result<(), IpcError> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half).take((MAX_COMMAND_BYTES + 2) as u64);
    let mut buffer = Vec::with_capacity(MAX_COMMAND_BYTES + 2);

    let received = match timeout(READ_TIMEOUT, reader.read_until(b'\n', &mut buffer)).await {
        Ok(Ok(received)) => received,
        Ok(Err(err)) => return Err(IpcError::Io(err)),
        Err(_) => {
            write_response(&mut write_half, "error таймаут чтения команды").await?;
            return Ok(());
        }
    };

    if received == 0 {
        return Ok(());
    }

    let payload = buffer.strip_suffix(b"\n").unwrap_or(&buffer);
    let payload = payload.strip_suffix(b"\r").unwrap_or(payload);
    if payload.len() > MAX_COMMAND_BYTES {
        write_response(&mut write_half, "error превышен допустимый размер команды").await?;
        return Ok(());
    }

    let line = match std::str::from_utf8(payload) {
        Ok(line) => line,
        Err(_) => {
            write_response(&mut write_half, "error команда должна быть в UTF-8").await?;
            return Ok(());
        }
    };

    let command = match IpcCommand::parse(line) {
        Ok(command) => command,
        Err(err) => {
            write_response(&mut write_half, &format!("error {err}")).await?;
            return Ok(());
        }
    };

    let (reply, reply_rx) = oneshot::channel();
    let request = IpcRequest { command, reply };
    match timeout(ENQUEUE_TIMEOUT, sender.send(request)).await {
        Ok(Ok(())) => {}
        Ok(Err(_)) | Err(_) => {
            write_response(&mut write_half, "error основной процесс недоступен").await?;
            return Ok(());
        }
    }

    wake();

    match timeout(APP_REPLY_TIMEOUT, reply_rx).await {
        Ok(Ok(response)) => write_response(&mut write_half, &response).await?,
        Ok(Err(_)) => {
            write_response(&mut write_half, "error обработчик команды недоступен").await?;
        }
        Err(_) => {
            write_response(&mut write_half, "error таймаут обработки команды").await?;
        }
    }

    Ok(())
}

async fn write_response(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    response: &str,
) -> Result<(), IpcError> {
    writer.write_all(response.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.shutdown().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tempfile::TempDir;

    use super::*;

    fn temp_socket() -> (TempDir, std::path::PathBuf) {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("orbitfm.sock");
        (dir, path)
    }

    #[tokio::test]
    async fn serves_request_and_returns_app_response() {
        let (_dir, path) = temp_socket();
        let mut server = IpcServer::try_bind_at(&path).await.expect("bind");
        let mut commands = server.commands();

        let wake_count = Arc::new(AtomicUsize::new(0));
        let wake_counter = Arc::clone(&wake_count);
        let wake: WakeFn = Arc::new(move || {
            wake_counter.fetch_add(1, Ordering::SeqCst);
        });

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(server.run(wake, shutdown_rx));
        let client_path = path.clone();
        let client = tokio::spawn(async move { client::send(&client_path, "status").await });

        let request = commands.recv().await.expect("request");
        assert_eq!(request.command(), &IpcCommand::Status);
        assert!(request.try_respond("status stopped\nwindow visible"));

        let response = client.await.expect("client task").expect("response");
        assert_eq!(response, "status stopped\nwindow visible");
        assert_eq!(wake_count.load(Ordering::SeqCst), 1);

        shutdown_tx.send(()).expect("shutdown");
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("сервер должен остановиться")
            .expect("join");
        assert!(!path.exists(), "сокет должен удаляться при завершении");
    }

    #[tokio::test]
    async fn drains_pending_reply_after_shutdown_signal() {
        let (_dir, path) = temp_socket();
        let mut server = IpcServer::try_bind_at(&path).await.expect("bind");
        let mut commands = server.commands();

        let wake: WakeFn = Arc::new(|| {});
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(server.run(wake, shutdown_rx));

        let client_path = path.clone();
        let client = tokio::spawn(async move { client::send(&client_path, "quit").await });

        let request = commands.recv().await.expect("request");
        shutdown_tx.send(()).expect("shutdown");
        assert!(request.try_respond("ok"));

        let response = client.await.expect("client task").expect("response");
        assert_eq!(response, "ok");
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("сервер должен остановиться")
            .expect("join");
    }

    #[tokio::test]
    async fn rejects_invalid_input_without_delivering_it() {
        let (_dir, path) = temp_socket();
        let mut server = IpcServer::try_bind_at(&path).await.expect("bind");
        let mut commands = server.commands();

        let wake: WakeFn = Arc::new(|| {});
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(server.run(wake, shutdown_rx));

        let mut stream = UnixStream::connect(&path).await.expect("connect");
        let payload = vec![b'a'; MAX_COMMAND_BYTES + 1024];
        stream.write_all(&payload).await.expect("write payload");
        stream.write_all(b"\n").await.expect("write newline");
        stream.shutdown().await.expect("shutdown");
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .await
            .expect("read response");
        assert!(response.starts_with("error"), "ответ: {response}");

        let mut stream = UnixStream::connect(&path).await.expect("reconnect");
        stream.write_all(&[0xff, b'\n']).await.expect("write");
        stream.shutdown().await.expect("shutdown");
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .await
            .expect("read response");
        assert!(response.contains("UTF-8"), "ответ: {response}");

        assert!(
            commands.try_recv().is_err(),
            "некорректные команды не должны попадать в очередь"
        );

        shutdown_tx.send(()).expect("shutdown");
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("сервер должен остановиться")
            .expect("join");
    }

    #[tokio::test]
    async fn reports_already_running_for_live_socket() {
        let (_dir, path) = temp_socket();
        let mut first = IpcServer::try_bind_at(&path).await.expect("first bind");
        let _commands = first.commands();

        let result = IpcServer::try_bind_at(&path).await;
        assert!(matches!(result, Err(IpcError::AlreadyRunning)));
    }

    #[tokio::test]
    async fn replaces_stale_socket_file() {
        let (_dir, path) = temp_socket();
        let stale = std::os::unix::net::UnixListener::bind(&path).expect("stale bind");
        drop(stale);
        assert!(path.exists());

        let server = IpcServer::try_bind_at(&path).await.expect("rebind");
        drop(server);
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn socket_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let (_dir, path) = temp_socket();
        let server = IpcServer::try_bind_at(&path).await.expect("bind");
        let mode = std::fs::metadata(&path)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        drop(server);
    }

    #[tokio::test]
    async fn client_reports_not_running_without_socket() {
        let (_dir, path) = temp_socket();
        let result = client::send(&path, "status").await;
        assert!(matches!(result, Err(IpcError::NotRunning)));
    }
}
