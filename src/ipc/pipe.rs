use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};

use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};

use super::protocol::IpcError;

const PIPE_PATH: &str = r"\\.\pipe\orbitfm";

pub(super) type Stream = NamedPipeServer;
pub(super) type ClientStream = NamedPipeClient;

/// Слушающий named pipe: держит ровно один несоединённый экземпляр.
pub(super) struct Listener {
    name: OsString,
    next: Option<NamedPipeServer>,
}

/// На Windows отдельного ресурса для очистки нет: pipe исчезает при закрытии handle.
pub(super) struct SocketGuard;

pub(super) fn default_path() -> Result<PathBuf, IpcError> {
    Ok(PathBuf::from(PIPE_PATH))
}

pub(super) async fn bind(path: &Path) -> Result<(Listener, SocketGuard), IpcError> {
    let first = ServerOptions::new()
        .first_pipe_instance(true)
        .reject_remote_clients(true)
        .create(path.as_os_str())
        .map_err(|err| match err.kind() {
            io::ErrorKind::PermissionDenied | io::ErrorKind::AlreadyExists => {
                IpcError::AlreadyRunning
            }
            _ => IpcError::Io(err),
        })?;

    Ok((
        Listener {
            name: path.as_os_str().to_owned(),
            next: Some(first),
        },
        SocketGuard,
    ))
}

impl Listener {
    pub(super) async fn accept(&mut self) -> io::Result<Stream> {
        let instance = match self.next.take() {
            Some(instance) => instance,
            None => create_instance(&self.name)?,
        };
        instance.connect().await?;
        self.next = Some(create_instance(&self.name)?);
        Ok(instance)
    }
}

pub(super) async fn connect(path: &Path) -> io::Result<ClientStream> {
    ClientOptions::new().open(path.as_os_str())
}

fn create_instance(name: &OsStr) -> io::Result<NamedPipeServer> {
    ServerOptions::new()
        .reject_remote_clients(true)
        .create(name)
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

    use super::*;

    #[tokio::test]
    async fn named_pipe_round_trips_a_line() {
        let path = PathBuf::from(format!(r"\\.\pipe\orbitfm-test-{}", std::process::id()));
        let (mut listener, _guard) = bind(&path).await.expect("bind");
        let client = tokio::spawn({
            let path = path.clone();
            async move { connect(&path).await }
        });

        let mut server = listener.accept().await.expect("accept");
        let mut client = client.await.expect("client task").expect("connect");

        client.write_all(b"status\n").await.expect("write");
        let mut line = String::new();
        BufReader::new(&mut server)
            .read_line(&mut line)
            .await
            .expect("read command");
        assert_eq!(line, "status\n");

        server
            .write_all(b"status stopped\n")
            .await
            .expect("write response");
        server.shutdown().await.expect("shutdown");
        drop(server);

        let mut response = String::new();
        client
            .read_to_string(&mut response)
            .await
            .expect("read response");
        assert_eq!(response, "status stopped\n");
    }
}
