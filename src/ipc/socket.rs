use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::ErrorKind;
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(all(unix, not(target_os = "linux")))]
use std::fs::DirBuilder;
#[cfg(all(unix, not(target_os = "linux")))]
use std::os::unix::fs::DirBuilderExt;

use tokio::net::{UnixListener, UnixStream};
use tokio::time::timeout;
use tracing::warn;

use super::protocol::IpcError;

const PROBE_TIMEOUT: Duration = Duration::from_millis(500);
const SOCKET_FILE: &str = "orbitfm.sock";
const LOCK_FILE: &str = "orbitfm.lock";

pub(super) type Stream = UnixStream;
pub(super) type ClientStream = UnixStream;

/// Обёртка над `UnixListener` с общим для платформ интерфейсом.
pub(super) struct Listener(UnixListener);

impl Listener {
    pub(super) async fn accept(&mut self) -> std::io::Result<Stream> {
        self.0.accept().await.map(|(stream, _)| stream)
    }
}

pub(super) async fn connect(path: &Path) -> std::io::Result<ClientStream> {
    UnixStream::connect(path).await
}

/// Путь сокета по ADR-0005.
///
/// На всех Unix каталог сокета обязан принадлежать текущему пользователю и
/// быть закрытым от группы и остальных: иначе чужой процесс мог бы заранее
/// создать сокет или lock-файл. На Linux это XDG_RUNTIME_DIR, на остальных
/// Unix — приватный подкаталог `orbitfm-<uid>` в TMPDIR (сам TMPDIR может быть
/// общедоступным `/tmp`).
pub(super) fn default_path() -> Result<PathBuf, IpcError> {
    #[cfg(target_os = "linux")]
    {
        let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
            .ok_or_else(|| IpcError::SocketPath("переменная XDG_RUNTIME_DIR не задана".into()))?;
        let dir = PathBuf::from(runtime_dir);
        validate_socket_dir(&dir, "XDG_RUNTIME_DIR")?;
        Ok(dir.join(SOCKET_FILE))
    }

    #[cfg(all(unix, not(target_os = "linux")))]
    {
        let base = std::env::var_os("TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
        if !base.is_absolute() {
            return Err(IpcError::SocketPath(format!(
                "TMPDIR должен быть абсолютным путём: {}",
                base.display()
            )));
        }
        let dir = base.join(format!("orbitfm-{}", current_uid()));
        match DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => {}
            Err(err) if err.kind() == ErrorKind::AlreadyExists => {}
            Err(err) => return Err(IpcError::Io(err)),
        }
        validate_socket_dir(&dir, "каталог сокета")?;
        Ok(dir.join(SOCKET_FILE))
    }
}

fn validate_socket_dir(dir: &Path, label: &str) -> Result<(), IpcError> {
    if !dir.is_absolute() {
        return Err(IpcError::SocketPath(format!(
            "{label} должен быть абсолютным путём: {}",
            dir.display()
        )));
    }

    let metadata = fs::metadata(dir).map_err(IpcError::Io)?;
    if !metadata.is_dir() {
        return Err(IpcError::SocketPath(format!(
            "{label} не является каталогом: {}",
            dir.display()
        )));
    }

    if metadata.uid() != current_uid() {
        return Err(IpcError::SocketPath(format!(
            "{label} принадлежит другому пользователю: {}",
            dir.display()
        )));
    }

    let mode = metadata.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(IpcError::SocketPath(format!(
            "{label} должен иметь права 0700: {} ({mode:o})",
            dir.display()
        )));
    }

    Ok(())
}

/// Effective UID текущего процесса: каталог сокета — только свой.
fn current_uid() -> u32 {
    // SAFETY: `geteuid` не имеет предусловий и не может отказать.
    unsafe { libc::geteuid() }
}

/// Занимает сокет. «Зомби»-файл от завершившегося процесса удаляется.
///
/// Lock-файл сериализует проверку и очистку сокета между экземплярами,
/// чтобы два одновременных запуска не могли удалить живой сокет.
pub(super) async fn bind(path: &Path) -> Result<(Listener, SocketGuard), IpcError> {
    let lock = acquire_lock(path)?;

    match UnixListener::bind(path) {
        Ok(listener) => {
            secure_socket(path)?;
            Ok((Listener(listener), SocketGuard::new(path.to_owned(), lock)))
        }
        Err(err) if err.kind() == ErrorKind::AddrInUse => {
            match probe(path).await? {
                SocketState::Stale => remove_stale(path)?,
                SocketState::Alive | SocketState::Unknown => return Err(IpcError::AlreadyRunning),
            }

            match UnixListener::bind(path) {
                Ok(listener) => {
                    secure_socket(path)?;
                    Ok((Listener(listener), SocketGuard::new(path.to_owned(), lock)))
                }
                Err(err) if err.kind() == ErrorKind::AddrInUse => Err(IpcError::AlreadyRunning),
                Err(err) => Err(IpcError::Io(err)),
            }
        }
        Err(err) => Err(IpcError::Io(err)),
    }
}

enum SocketState {
    Alive,
    Stale,
    Unknown,
}

async fn probe(path: &Path) -> Result<SocketState, IpcError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_socket() => {}
        Ok(_) => {
            return Err(IpcError::SocketPath(format!(
                "{} уже существует и не является Unix-сокетом",
                path.display()
            )));
        }
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(SocketState::Stale),
        Err(err) => return Err(IpcError::Io(err)),
    }

    match timeout(PROBE_TIMEOUT, UnixStream::connect(path)).await {
        Ok(Ok(_stream)) => Ok(SocketState::Alive),
        Ok(Err(err))
            if matches!(
                err.kind(),
                ErrorKind::NotFound | ErrorKind::ConnectionRefused
            ) =>
        {
            Ok(SocketState::Stale)
        }
        Ok(Err(_)) | Err(_) => Ok(SocketState::Unknown),
    }
}

fn remove_stale(path: &Path) -> Result<(), IpcError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => Err(IpcError::Io(err)),
    }
}

fn secure_socket(path: &Path) -> Result<(), IpcError> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|err| {
        if let Err(cleanup_err) = fs::remove_file(path) {
            warn!(error = %cleanup_err, "не удалось удалить сокет после ошибки прав");
        }
        IpcError::Io(err)
    })
}

/// Эксклюзивная блокировка на время жизни процесса.
/// Файл не удаляется: unlink lock-файла создаёт гонку между экземплярами.
struct LockGuard {
    _file: File,
}

fn acquire_lock(path: &Path) -> Result<LockGuard, IpcError> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(lock_path(path))
        .map_err(IpcError::Io)?;

    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(IpcError::Io)?;

    match file.try_lock() {
        Ok(()) => Ok(LockGuard { _file: file }),
        Err(TryLockError::WouldBlock) => Err(IpcError::AlreadyRunning),
        Err(TryLockError::Error(err)) => Err(IpcError::Io(err)),
    }
}

fn lock_path(path: &Path) -> PathBuf {
    path.with_file_name(LOCK_FILE)
}

/// Удаляет файл сокета при завершении сервера, удерживая lock.
pub(super) struct SocketGuard {
    path: PathBuf,
    _lock: LockGuard,
}

impl SocketGuard {
    fn new(path: PathBuf, lock: LockGuard) -> Self {
        Self { path, _lock: lock }
    }
}

impl Drop for SocketGuard {
    fn drop(&mut self) {
        match fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => warn!(
                path = %self.path.display(),
                error = %err,
                "не удалось удалить файл сокета"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir_with_mode(mode: u32) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(mode)).expect("chmod");
        dir
    }

    #[test]
    fn socket_dir_must_be_private_and_owned() {
        let public = dir_with_mode(0o755);
        assert!(
            validate_socket_dir(public.path(), "TESTDIR").is_err(),
            "каталог, доступный группе и остальным, не годится"
        );

        let private = dir_with_mode(0o700);
        validate_socket_dir(private.path(), "TESTDIR").expect("0700 и владелец — свой");
    }

    #[test]
    fn socket_dir_must_be_absolute() {
        assert!(matches!(
            validate_socket_dir(Path::new("relative/dir"), "TESTDIR"),
            Err(IpcError::SocketPath(_))
        ));
    }
}
