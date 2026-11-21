//! CLI-адаптер ядра: долгоживущий `orbitfm events` и one-shot команды.
//!
//! Путь к бинарнику: `ORBITFM_BIN` → `orbitfm` из `PATH`. Аргументы передаются
//! в `Command` без shell (`docs/interfaces/16-tui.md` §3, ADR-0047 §3).

use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use super::parse;
use super::{CoreClient, CoreCommand, CoreError, CoreEvent, CoreReply, EventStream};

/// Таймаут one-shot команды (больше CLI-таймаута 12 с и покрывает резолв).
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);
/// Шаг опроса `try_wait` при ожидании команды.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Клиент к ядру через установленный `orbitfm` в `PATH`/`ORBITFM_BIN`.
#[derive(Debug, Clone)]
pub struct CliCoreClient {
    bin: PathBuf,
}

impl CliCoreClient {
    /// Путь к бинарнику: `ORBITFM_BIN`, иначе `orbitfm` из `PATH`.
    pub fn discover() -> Self {
        match std::env::var_os("ORBITFM_BIN").filter(|value| !value.is_empty()) {
            Some(bin) => Self {
                bin: PathBuf::from(bin),
            },
            None => Self {
                bin: PathBuf::from("orbitfm"),
            },
        }
    }

    /// Клиент с явным путём (тесты, нестандартная установка).
    #[cfg(test)]
    pub fn with_bin(bin: impl Into<PathBuf>) -> Self {
        Self { bin: bin.into() }
    }

    /// Запуск команды с таймаутом; возвращает (код, stdout, stderr).
    fn run(&self, args: &[String]) -> Result<(Option<i32>, String, String), CoreError> {
        let mut child =
            spawn_retrying(&self.bin, args).map_err(|error| CoreError::Io(error.to_string()))?;

        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| CoreError::Io("нет stdout".to_owned()))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| CoreError::Io("нет stderr".to_owned()))?;

        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    if started.elapsed() >= COMMAND_TIMEOUT {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(CoreError::Timeout);
                    }
                    thread::sleep(POLL_INTERVAL);
                }
                Err(error) => return Err(CoreError::Io(error.to_string())),
            }
        };

        let mut out = String::new();
        let mut err = String::new();
        let _ = stdout.read_to_string(&mut out);
        let _ = stderr.read_to_string(&mut err);
        Ok((status.code(), out, err))
    }
}

/// `spawn` с коротким ретраем на `ETXTBSY` (26): редкая гонка, когда файл
/// только что записан и сразу запускается (тесты, точечные сборки).
fn spawn_retrying(bin: &std::path::Path, args: &[String]) -> std::io::Result<std::process::Child> {
    const ETXTBSY: i32 = 26;
    for attempt in 0..5 {
        match Command::new(bin)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => return Ok(child),
            Err(error) if error.raw_os_error() == Some(ETXTBSY) && attempt < 4 => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("последняя итерация всегда возвращает")
}

impl CoreClient for CliCoreClient {
    fn start_events(&mut self, spectrum: bool) -> Result<EventStream, CoreError> {
        let mut args = vec!["events".to_owned()];
        if spectrum {
            args.push("--spectrum".to_owned());
        }

        let mut child =
            spawn_retrying(&self.bin, &args).map_err(|error| CoreError::Io(error.to_string()))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| CoreError::Io("нет stdout".to_owned()))?;

        // stderr не должен заполнить пайп: вычитываем в отдельном потоке и
        // храним последнюю строку для сообщения о разрыве.
        let last_error = Arc::new(Mutex::new(String::new()));
        if let Some(mut stderr) = child.stderr.take() {
            let sink = Arc::clone(&last_error);
            thread::spawn(move || {
                let mut buffer = String::new();
                if stderr.read_to_string(&mut buffer).is_ok() {
                    let line = buffer.lines().last().unwrap_or("").trim().to_owned();
                    if let Ok(mut slot) = sink.lock() {
                        *slot = line;
                    }
                }
            });
        }

        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            let mut malformed = 0_u32;
            for line in reader.lines() {
                let Ok(line) = line else { break };
                match parse::parse_event(&line) {
                    Ok(parse::Parsed::Event(event)) => {
                        malformed = 0;
                        if sender.send(event).is_err() {
                            break;
                        }
                    }
                    Ok(parse::Parsed::Ignored) => {}
                    Err(parse::ParseError::Incompatible(message)) => {
                        if sender.send(CoreEvent::Incompatible { message }).is_err() {
                            break;
                        }
                    }
                    // Несколько битых строк подряд — соединение под подозрением.
                    Err(parse::ParseError::Malformed(_)) => {
                        malformed += 1;
                        if malformed >= 5 {
                            break;
                        }
                    }
                }
            }
            let message = last_error
                .lock()
                .map(|guard| guard.clone())
                .unwrap_or_default();
            let _ = sender.send(CoreEvent::Disconnected { message });
            let _ = child.kill();
            let _ = child.wait();
        });

        Ok(receiver)
    }

    fn execute(&mut self, command: CoreCommand) -> Result<CoreReply, CoreError> {
        let args = command.args();
        let (code, stdout, stderr) = self.run(&args)?;
        parse::parse_reply(&command, code, &stdout, &stderr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Фиктивный `orbitfm` без настоящего ядра: печатает то, что просят.
    ///
    /// `fs::write` закрывает fd до `exec`, иначе возможен ETXTBSY.
    fn fake_bin(dir: &std::path::Path, body: &str) -> PathBuf {
        let path = dir.join("orbitfm-fake");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        path
    }

    #[test]
    fn execute_parses_ok_reply() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bin = fake_bin(dir.path(), "echo ok");
        let mut client = CliCoreClient::with_bin(bin);
        assert_eq!(
            client.execute(CoreCommand::Stop),
            Ok(CoreReply::Ok {
                text: "ok".to_owned()
            })
        );
    }

    #[test]
    fn execute_maps_not_running() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bin = fake_bin(dir.path(), "echo 'OrbitFM не запущен' >&2; exit 1");
        let mut client = CliCoreClient::with_bin(bin);
        assert_eq!(
            client.execute(CoreCommand::Stop),
            Err(CoreError::NotRunning)
        );
    }

    #[test]
    fn missing_binary_is_io_error() {
        let mut client = CliCoreClient::with_bin("/nonexistent/orbitfm-xyz");
        assert!(matches!(
            client.execute(CoreCommand::Stop),
            Err(CoreError::Io(_))
        ));
    }

    #[test]
    fn start_events_streams_frames_and_disconnect() {
        let dir = tempfile::tempdir().expect("tempdir");
        let frame =
            r#"{"ev":1,"type":"snapshot","status":{"schema":1,"app":"orbitfm","state":"playing"}}"#;
        let bin = fake_bin(dir.path(), &format!("echo '{frame}'"));
        let mut client = CliCoreClient::with_bin(bin);
        let stream = client.start_events(false).expect("events");
        let first = stream.recv().expect("first");
        assert!(matches!(first, CoreEvent::State(_)));
        let second = stream.recv().expect("disconnect");
        assert!(matches!(second, CoreEvent::Disconnected { .. }));
    }

    #[test]
    fn start_events_unknown_argument_falls_back_by_caller() {
        // Адаптер не решает про fallback сам: ядро просто отклоняет флаг.
        let dir = tempfile::tempdir().expect("tempdir");
        let bin = fake_bin(dir.path(), "echo 'unknown argument' >&2; exit 1");
        let mut client = CliCoreClient::with_bin(bin);
        let stream = client.start_events(true).expect("spawn ok");
        let event = stream.recv().expect("disconnect");
        assert!(matches!(event, CoreEvent::Disconnected { .. }));
    }
}
