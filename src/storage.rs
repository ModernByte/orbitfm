//! Надёжная запись небольших файлов состояния.
//!
//! Схема: временный файл в том же каталоге → `fsync` данных → `rename` →
//! `fsync` каталога (Unix). После успешного возврата на диске лежит либо
//! прежняя, либо новая версия целиком: обрезанный JSON после сбоя питания не
//! появится (ADR-0013, ред. 2; ADR-0031, ред. 2).

use std::fs;
use std::io::{self, Write};
use std::path::Path;

pub(crate) fn write_atomic(path: &Path, tmp_name: &str, bytes: &[u8]) -> io::Result<()> {
    let directory = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "путь без родительского каталога",
        )
    })?;
    fs::create_dir_all(directory)?;

    let tmp = path.with_file_name(tmp_name);
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)?;
        sync_directory(directory)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// `fsync` каталога фиксирует сам `rename`. На Windows каталог так не
/// открывается, поэтому там вызов пуст.
#[cfg(unix)]
fn sync_directory(directory: &Path) -> io::Result<()> {
    fs::File::open(directory)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_directory: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_replaces_atomically() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("state.json");
        write_atomic(&path, "state.json.tmp", b"{\"a\":1}").expect("first write");
        assert_eq!(fs::read(&path).unwrap(), b"{\"a\":1}");
        write_atomic(&path, "state.json.tmp", b"{\"a\":2}").expect("replace");
        assert_eq!(fs::read(&path).unwrap(), b"{\"a\":2}");
        assert!(
            !dir.path().join("state.json.tmp").exists(),
            "временный файл убран"
        );
    }

    #[test]
    fn missing_directory_is_reported() {
        let path = Path::new("");
        assert!(write_atomic(path, "x.tmp", b"{}").is_err());
    }
}
