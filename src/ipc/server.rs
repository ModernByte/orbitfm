use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{Semaphore, mpsc, oneshot, watch};
use tokio::task::JoinSet;
use tokio::time::timeout;
use tracing::warn;

use super::client;
use super::events;
use super::protocol::{IpcCommand, IpcError, MAX_COMMAND_BYTES};
use super::transport::{self, Listener, SocketGuard, Stream};

pub(crate) use super::client::Subscription as EventSubscription;

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
    listener: Listener,
    _socket_guard: SocketGuard,
    sender: mpsc::Sender<IpcRequest>,
    receiver: Option<mpsc::Receiver<IpcRequest>>,
}

impl IpcServer {
    /// Занимает сокет по умолчанию.
    pub async fn try_bind() -> Result<Self, IpcError> {
        let path = transport::default_path()?;
        Self::try_bind_at(&path).await
    }

    pub(super) async fn try_bind_at(path: &Path) -> Result<Self, IpcError> {
        let (listener, socket_guard) = transport::bind(path).await?;
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
        let path = transport::default_path()?;
        client::send(&path, command).await
    }

    /// Подписывается на поток событий уже работающего экземпляра (ADR-0039).
    pub async fn subscribe_to_existing() -> Result<(), IpcError> {
        let path = transport::default_path()?;
        client::subscribe(&path).await
    }

    /// Подписка с каналом кадров — для native messaging host (ADR-0038).
    pub(crate) async fn subscribe_frames_to_existing() -> Result<EventSubscription, IpcError> {
        let path = transport::default_path()?;
        client::subscribe_frames(&path).await
    }

    /// Принимает соединения до сигнала shutdown, затем дожидается ответов.
    ///
    /// `status` — канал публикации состояния для подписок `events` (ADR-0039).
    pub async fn run(
        self,
        wake: WakeFn,
        status: watch::Receiver<Option<String>>,
        mut shutdown: oneshot::Receiver<()>,
    ) {
        let Self {
            mut listener,
            _socket_guard,
            sender,
            receiver: _,
        } = self;

        let permits = Arc::new(Semaphore::new(MAX_CONNECTIONS));
        let event_permits = Arc::new(Semaphore::new(events::MAX_SUBSCRIBERS));
        let (stop_tx, stop_rx) = watch::channel(false);
        let mut connections = JoinSet::new();

        loop {
            tokio::select! {
                biased;
                _ = &mut shutdown => break,
                accepted = listener.accept() => {
                    let stream = match accepted {
                        Ok(stream) => stream,
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
                    let status = status.clone();
                    let stop = stop_rx.clone();
                    let event_permits = Arc::clone(&event_permits);
                    connections.spawn(async move {
                        let _permit = permit;
                        if let Err(err) =
                            handle_connection(stream, sender, wake, status, event_permits, stop)
                                .await
                        {
                            warn!(error = %err, "IPC-соединение завершилось с ошибкой");
                        }
                    });
                }
            }
        }

        drop(listener);
        // Подписки живут до отключения клиента: без сигнала drain ждал бы их
        // до таймаута и обрывал соединения.
        let _ = stop_tx.send(true);

        let drain = async { while connections.join_next().await.is_some() {} };
        if timeout(DRAIN_TIMEOUT, drain).await.is_err() {
            warn!("не все IPC-соединения завершились, принудительная остановка");
            connections.shutdown().await;
        }
    }
}

async fn handle_connection(
    stream: Stream,
    sender: mpsc::Sender<IpcRequest>,
    wake: WakeFn,
    status: watch::Receiver<Option<String>>,
    event_permits: Arc<Semaphore>,
    stop: watch::Receiver<bool>,
) -> Result<(), IpcError> {
    let (read_half, mut write_half) = tokio::io::split(stream);
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

    let Ok(line) = std::str::from_utf8(payload) else {
        write_response(&mut write_half, "error команда должна быть в UTF-8").await?;
        return Ok(());
    };

    let command = match IpcCommand::parse(line) {
        Ok(command) => command,
        Err(err) => {
            write_response(&mut write_half, &format!("error {err}")).await?;
            return Ok(());
        }
    };

    if command == IpcCommand::Events {
        let Ok(permit) = event_permits.try_acquire_owned() else {
            write_response(&mut write_half, "error слишком много подписок на события").await?;
            return Ok(());
        };
        let _permit = permit;
        return match events::stream(&mut write_half, status, stop, events::KEEPALIVE).await {
            Ok(()) => Ok(()),
            Err(err) if is_disconnect(&err) => Ok(()),
            Err(err) => Err(IpcError::Io(err)),
        };
    }

    let (reply, reply_rx) = oneshot::channel();
    let request = IpcRequest { command, reply };
    let delivered = timeout(ENQUEUE_TIMEOUT, sender.send(request))
        .await
        .is_ok_and(|queued| queued.is_ok());
    if !delivered {
        write_response(&mut write_half, "error основной процесс недоступен").await?;
        return Ok(());
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

/// Клиент закрыл подписку или оборвал соединение — это штатное завершение.
fn is_disconnect(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::NotConnected
            | std::io::ErrorKind::UnexpectedEof
    )
}

async fn write_response<W>(writer: &mut W, response: &str) -> Result<(), IpcError>
where
    W: AsyncWrite + Unpin,
{
    writer.write_all(response.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.shutdown().await?;
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tempfile::TempDir;
    use tokio::net::UnixStream;

    use super::events::StatusFeed;
    use super::*;

    fn temp_socket() -> (TempDir, std::path::PathBuf) {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("orbitfm.sock");
        (dir, path)
    }

    /// Канал состояния с готовым первым кадром; публикатор держим живым.
    fn status_channel(status: &str) -> (StatusFeed, watch::Receiver<Option<String>>) {
        let feed = StatusFeed::new();
        feed.publish(status.to_owned());
        let receiver = feed.subscribe();
        (feed, receiver)
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

        let (_feed, status) = status_channel("{\"schema\":1}");
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(server.run(wake, status, shutdown_rx));
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
    async fn streams_snapshot_and_state_without_enqueuing_a_command() {
        let (_dir, path) = temp_socket();
        let mut server = IpcServer::try_bind_at(&path).await.expect("bind");
        let mut commands = server.commands();
        let (feed, status) = status_channel("{\"schema\":1,\"state\":\"stopped\"}");

        let wake: WakeFn = Arc::new(|| {});
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(server.run(wake, status, shutdown_rx));

        let mut stream = UnixStream::connect(&path).await.expect("connect");
        stream.write_all(b"events\n").await.expect("subscribe");
        stream.shutdown().await.expect("shutdown write");
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await.expect("snapshot");
        assert!(
            line.starts_with("{\"ev\":1,\"type\":\"snapshot\""),
            "{line}"
        );
        assert!(line.contains("\"state\":\"stopped\""), "{line}");
        assert!(
            commands.try_recv().is_err(),
            "подписка не должна попадать в очередь команд"
        );

        feed.publish("{\"schema\":1,\"state\":\"playing\"}".to_owned());
        line.clear();
        reader.read_line(&mut line).await.expect("state");
        assert!(line.starts_with("{\"ev\":1,\"type\":\"state\""), "{line}");
        assert!(line.contains("\"state\":\"playing\""), "{line}");

        drop(reader);
        shutdown_tx.send(()).expect("shutdown");
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("сервер должен остановиться")
            .expect("join");
    }

    #[tokio::test]
    async fn rejects_subscription_beyond_budget() {
        let (_dir, path) = temp_socket();
        let mut server = IpcServer::try_bind_at(&path).await.expect("bind");
        let mut commands = server.commands();
        let (_feed, status) = status_channel("{\"schema\":1}");

        let wake: WakeFn = Arc::new(|| {});
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(server.run(wake, status, shutdown_rx));

        let mut subscribers = Vec::new();
        for _ in 0..events::MAX_SUBSCRIBERS {
            let mut stream = UnixStream::connect(&path).await.expect("connect");
            stream.write_all(b"events\n").await.expect("subscribe");
            stream.shutdown().await.expect("shutdown write");
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).await.expect("snapshot");
            assert!(line.contains("snapshot"), "{line}");
            subscribers.push(reader);
        }

        let mut extra = UnixStream::connect(&path).await.expect("connect");
        extra.write_all(b"events\n").await.expect("subscribe");
        extra.shutdown().await.expect("shutdown write");
        let mut response = String::new();
        extra
            .read_to_string(&mut response)
            .await
            .expect("rejection");
        assert!(response.contains("слишком много подписок"), "{response}");
        assert!(commands.try_recv().is_err());

        drop(subscribers);
        shutdown_tx.send(()).expect("shutdown");
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("сервер должен остановиться")
            .expect("join");
    }

    #[tokio::test]
    async fn drains_pending_reply_after_shutdown_signal() {
        let (_dir, path) = temp_socket();
        let mut server = IpcServer::try_bind_at(&path).await.expect("bind");
        let mut commands = server.commands();

        let wake: WakeFn = Arc::new(|| {});
        let (_feed, status) = status_channel("{\"schema\":1}");
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(server.run(wake, status, shutdown_rx));

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
        let (_feed, status) = status_channel("{\"schema\":1}");
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(server.run(wake, status, shutdown_rx));

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
