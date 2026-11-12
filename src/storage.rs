//! Надёжная запись небольших файлов состояния.
//!
//! Схема: временный файл в том же каталоге → `fsync` данных → `rename` →
//! `fsync` каталога (Unix). После успешного возврата на диске лежит либо
//! прежняя, либо новая версия целиком: обрезанный JSON после сбоя питания не
//! появится (ADR-0013, ред. 2; ADR-0031, ред. 2).

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

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

/// Максимум попыток подобрать свободное имя рядом с повреждённым файлом.
const QUARANTINE_ATTEMPTS: u32 = 100;

/// Отодвигает повреждённый файл в сторону, не затирая прежние копии:
/// `<path>.corrupt`, затем `<path>.corrupt-1`, `<path>.corrupt-2`…
///
/// Возвращает путь, куда переехал файл. Ошибка означает, что карантин не
/// удался: вызывающий обязан оставить стор временным, иначе первая же запись
/// уничтожила бы пользовательские данные (ADR-0062).
pub(crate) fn quarantine(path: &Path) -> io::Result<PathBuf> {
    for attempt in 0..QUARANTINE_ATTEMPTS {
        let candidate = quarantine_path(path, attempt);
        // `rename` на Unix молча заменяет цель, поэтому занятое имя
        // пропускаем: прежний карантин — тоже пользовательские данные.
        if candidate.exists() {
            continue;
        }
        match fs::rename(path, &candidate) {
            Ok(()) => return Ok(candidate),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "не удалось подобрать свободное имя для карантина",
    ))
}

fn quarantine_path(path: &Path, attempt: u32) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".corrupt");
    if attempt > 0 {
        name.push(format!("-{attempt}"));
    }
    PathBuf::from(name)
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

    #[test]
    fn quarantine_moves_damaged_file_without_clobbering() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("library.json");
        fs::write(&path, b"broken").expect("damaged file");

        let first = quarantine(&path).expect("first quarantine");
        assert_eq!(first, dir.path().join("library.json.corrupt"));
        assert_eq!(fs::read(&first).unwrap(), b"broken");
        assert!(!path.exists(), "повреждённый файл уехал из рабочего пути");

        fs::write(&path, b"broken again").expect("damaged file again");
        let second = quarantine(&path).expect("second quarantine");
        assert_eq!(second, dir.path().join("library.json.corrupt-1"));
        assert_eq!(fs::read(&first).unwrap(), b"broken", "прежняя копия цела");
        assert_eq!(fs::read(&second).unwrap(), b"broken again");
    }

    #[test]
    fn quarantine_reports_missing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(quarantine(&dir.path().join("absent.json")).is_err());
    }
}
