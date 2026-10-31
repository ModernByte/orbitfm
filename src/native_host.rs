//! Native messaging host для расширений Chromium и Firefox (ADR-0038).
//!
//! Браузер запускает `orbitfm native-host` и общается с ним по stdio: кадр —
//! 4 байта длины в native-endian и UTF-8 JSON. Хост не поднимает GUI и не
//! открывает новых портов: команды уходят в тот же IPC-сокет, что и у CLI.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::warn;

use crate::ipc::{EventSubscription, IpcCommand, IpcServer};

/// Предел одного сообщения в обе стороны (ADR-0038).
pub(crate) const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
const OUTPUT_QUEUE: usize = 64;

const OVERSIZED_ERROR: &str = "сообщение больше 1 МиБ";
const OVERSIZED_REPLY: &str = r#"{"ok":false,"error":"ответ больше 1 МиБ"}"#;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    #[serde(default)]
    id: Option<Value>,
    command: String,
}

enum FrameRead {
    Eof,
    Message(Vec<u8>),
    TooLarge,
}

/// Запускает хост на отдельном current-thread runtime без GUI.
pub(crate) fn run() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("не удалось создать runtime native host")?;
    runtime.block_on(run_loop())
}

async fn run_loop() -> Result<()> {
    let (sender, receiver) = mpsc::channel(OUTPUT_QUEUE);
    let writer = tokio::spawn(writer_loop(receiver));
    let mut stdin = tokio::io::stdin();
    let mut events_task: Option<JoinHandle<()>> = None;
    let events_active = Arc::new(AtomicBool::new(false));

    loop {
        match read_frame(&mut stdin).await {
            Ok(FrameRead::Eof) => break,
            Ok(FrameRead::TooLarge) => {
                let _ = sender
                    .send(error_response(None, "oversized", OVERSIZED_ERROR))
                    .await;
                break;
            }
            Ok(FrameRead::Message(bytes)) => {
                if let Some(task) = handle_message(&bytes, &sender, &events_active).await {
                    events_task = Some(task);
                }
            }
            Err(err) => {
                warn!(error = %err, "native host: ошибка чтения stdin");
                break;
            }
        }
    }

    if let Some(task) = events_task {
        task.abort();
    }
    drop(sender);
    let _ = writer.await;
    Ok(())
}

async fn handle_message(
    bytes: &[u8],
    sender: &mpsc::Sender<Value>,
    events_active: &Arc<AtomicBool>,
) -> Option<JoinHandle<()>> {
    let request: Request = match serde_json::from_slice(bytes) {
        Ok(request) => request,
        Err(err) => {
            let _ = sender
                .send(error_response(
                    None,
                    "invalid-request",
                    &format!("некорректный запрос: {err}"),
                ))
                .await;
            return None;
        }
    };

    let command = match IpcCommand::parse(&request.command) {
        Ok(command) => command,
        Err(err) => {
            let _ = sender
                .send(error_response(
                    request.id.as_ref(),
                    "invalid-command",
                    &err.to_string(),
                ))
                .await;
            return None;
        }
    };

    if matches!(command, IpcCommand::Events) {
        return start_events(request.id, sender, events_active).await;
    }

    if let Some((code, reason)) = forbidden(&command) {
        let _ = sender
            .send(error_response(request.id.as_ref(), code, reason))
            .await;
        return None;
    }

    match IpcServer::send_to_existing(&command.to_wire()).await {
        Ok(response) => {
            let _ = sender
                .send(ok_response(request.id.as_ref(), &response))
                .await;
        }
        Err(err) => {
            let _ = sender
                .send(error_response(
                    request.id.as_ref(),
                    ipc_code(&err),
                    &err.to_string(),
                ))
                .await;
        }
    }
    None
}

async fn start_events(
    id: Option<Value>,
    sender: &mpsc::Sender<Value>,
    events_active: &Arc<AtomicBool>,
) -> Option<JoinHandle<()>> {
    if events_active.swap(true, Ordering::SeqCst) {
        let _ = sender
            .send(error_response(
                id.as_ref(),
                "events-active",
                "поток событий уже открыт",
            ))
            .await;
        return None;
    }

    let subscription = match IpcServer::subscribe_frames_to_existing().await {
        Ok(subscription) => subscription,
        Err(err) => {
            events_active.store(false, Ordering::SeqCst);
            let _ = sender
                .send(error_response(
                    id.as_ref(),
                    ipc_code(&err),
                    &err.to_string(),
                ))
                .await;
            return None;
        }
    };

    let _ = sender.send(ok_response(id.as_ref(), "events")).await;
    let sender = sender.clone();
    let events_active = Arc::clone(events_active);
    Some(tokio::spawn(async move {
        forward_events(subscription, &sender).await;
        events_active.store(false, Ordering::SeqCst);
    }))
}

async fn forward_events(mut subscription: EventSubscription, sender: &mpsc::Sender<Value>) {
    while let Some(frame) = subscription.lines.recv().await {
        match frame {
            Ok(line) => match serde_json::from_str::<Value>(&line) {
                Ok(event) => {
                    if sender
                        .send(json!({"ok": true, "event": event}))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                Err(err) => {
                    let _ = sender
                        .send(json!({
                            "ok": false,
                            "code": "invalid-frame",
                            "stream": "invalid",
                            "error": format!("некорректный кадр событий: {err}"),
                        }))
                        .await;
                    return;
                }
            },
            Err(err) => {
                let _ = sender
                    .send(json!({
                        "ok": false,
                        "code": "stream-closed",
                        "stream": "closed",
                        "error": err.to_string(),
                    }))
                    .await;
                return;
            }
        }
    }
    let _ = sender.send(json!({"ok": true, "event": null})).await;
}

/// Allowlist ADR-0038: расширение не может завершить процесс, играть станцию
/// по произвольному UUID из каталога или переключить режим окна. Играть
/// разрешено только результат своего поиска (`play-result`), элемент
/// библиотеки (`play-library`) и явно выбранный URL страницы (`play-url`).
fn forbidden(command: &IpcCommand) -> Option<(&'static str, &'static str)> {
    match command {
        IpcCommand::Quit => Some(("forbidden", "команда quit запрещена")),
        IpcCommand::Play { .. } => Some(("forbidden", "команда play запрещена")),
        IpcCommand::ModeQuery | IpcCommand::SetMode { .. } => {
            Some(("forbidden", "команда mode запрещена"))
        }
        IpcCommand::Raw(_) => Some(("forbidden", "неизвестная команда")),
        _ => None,
    }
}

fn ipc_code(err: &crate::ipc::IpcError) -> &'static str {
    match err {
        crate::ipc::IpcError::NotRunning => "not-running",
        crate::ipc::IpcError::Timeout(_) => "timeout",
        crate::ipc::IpcError::Subscription(_) => "subscription-rejected",
        _ => "ipc-error",
    }
}

fn ok_response(id: Option<&Value>, response: &str) -> Value {
    let mut message = serde_json::Map::new();
    if let Some(id) = id {
        message.insert("id".to_owned(), id.clone());
    }
    message.insert("ok".to_owned(), Value::Bool(true));
    message.insert("response".to_owned(), Value::String(response.to_owned()));
    Value::Object(message)
}

fn error_response(id: Option<&Value>, code: &str, error: &str) -> Value {
    let mut message = serde_json::Map::new();
    if let Some(id) = id {
        message.insert("id".to_owned(), id.clone());
    }
    message.insert("ok".to_owned(), Value::Bool(false));
    message.insert("code".to_owned(), Value::String(code.to_owned()));
    message.insert("error".to_owned(), Value::String(error.to_owned()));
    Value::Object(message)
}

async fn writer_loop(mut receiver: mpsc::Receiver<Value>) {
    let mut stdout = tokio::io::stdout();
    while let Some(message) = receiver.recv().await {
        let bytes =
            serde_json::to_vec(&message).unwrap_or_else(|_| OVERSIZED_REPLY.as_bytes().to_vec());
        let bytes = if bytes.len() > MAX_MESSAGE_BYTES {
            OVERSIZED_REPLY.as_bytes().to_vec()
        } else {
            bytes
        };
        if write_frame(&mut stdout, &bytes).await.is_err() {
            break;
        }
    }
}

async fn read_frame(reader: &mut (impl AsyncRead + Unpin)) -> std::io::Result<FrameRead> {
    let mut header = [0_u8; 4];
    match reader.read_exact(&mut header).await {
        Ok(_) => {}
        Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Ok(FrameRead::Eof);
        }
        Err(err) => return Err(err),
    }

    let length = u32::from_ne_bytes(header) as usize;
    if length == 0 {
        return Ok(FrameRead::Eof);
    }
    if length > MAX_MESSAGE_BYTES {
        return Ok(FrameRead::TooLarge);
    }

    let mut payload = vec![0_u8; length];
    reader.read_exact(&mut payload).await?;
    Ok(FrameRead::Message(payload))
}

async fn write_frame(
    writer: &mut (impl AsyncWrite + Unpin),
    payload: &[u8],
) -> std::io::Result<()> {
    let length = u32::try_from(payload.len())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, OVERSIZED_ERROR))?;
    writer.write_all(&length.to_ne_bytes()).await?;
    writer.write_all(payload).await?;
    writer.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    fn command(raw: &str) -> IpcCommand {
        IpcCommand::parse(raw).expect("команда должна разбираться")
    }

    #[test]
    fn allowlist_blocks_dangerous_commands() {
        assert!(forbidden(&command("quit")).is_some());
        assert!(forbidden(&command("play 9604b0f1-aaaa")).is_some());
        assert!(forbidden(&command("mode")).is_some());
        assert!(forbidden(&command("mode panel")).is_some());
        assert!(forbidden(&IpcCommand::Raw("future".into())).is_some());
    }

    #[test]
    fn allowlist_keeps_playback_and_window_commands() {
        for raw in [
            "toggle",
            "show",
            "hide",
            "status",
            "status --json",
            "playpause",
            "next",
            "previous",
            "roulette",
            "stop",
            "volume 0.5",
            "volume-up",
            "volume-down",
            "theme dark",
            "settings",
            "panel",
            "overlay",
            "search lofi jazz",
            "library",
            "favorite",
            "favorite 9604b0f1-aaaa",
            "play-result 9604b0f1-aaaa",
            "play-library 9604b0f1-aaaa",
            "play-url https://example.org/live.m3u",
        ] {
            assert!(forbidden(&command(raw)).is_none(), "{raw}");
        }
    }

    #[test]
    fn parses_requests_strictly() {
        let request: Request =
            serde_json::from_str(r#"{"id":7,"command":"status --json"}"#).expect("request");
        assert_eq!(request.command, "status --json");
        assert_eq!(request.id, Some(Value::from(7)));

        let bare: Request = serde_json::from_str(r#"{"command":"status"}"#).expect("request");
        assert_eq!(bare.id, None);

        assert!(serde_json::from_str::<Request>(r#"{"command":"status","extra":1}"#).is_err());
        assert!(serde_json::from_str::<Request>(r#"{"id":1}"#).is_err());
    }

    #[tokio::test]
    async fn reads_framed_message_and_eof() {
        let (mut client, mut host) = duplex(64);
        client
            .write_all(&5_u32.to_ne_bytes())
            .await
            .expect("length");
        client.write_all(b"hello").await.expect("payload");
        drop(client);

        match read_frame(&mut host).await.expect("read") {
            FrameRead::Message(bytes) => assert_eq!(bytes, b"hello"),
            _ => panic!("ожидался кадр"),
        }
        assert!(matches!(
            read_frame(&mut host).await.expect("eof"),
            FrameRead::Eof
        ));
    }

    #[tokio::test]
    async fn zero_length_frame_is_eof() {
        let (mut client, mut host) = duplex(8);
        client
            .write_all(&0_u32.to_ne_bytes())
            .await
            .expect("length");
        drop(client);
        assert!(matches!(
            read_frame(&mut host).await.expect("eof"),
            FrameRead::Eof
        ));
    }

    #[tokio::test]
    async fn oversized_frame_is_rejected_without_reading_payload() {
        let (mut client, mut host) = duplex(8);
        let length = u32::try_from(MAX_MESSAGE_BYTES + 1).expect("length fits");
        client
            .write_all(&length.to_ne_bytes())
            .await
            .expect("length");
        drop(client);
        assert!(matches!(
            read_frame(&mut host).await.expect("size"),
            FrameRead::TooLarge
        ));
    }

    #[tokio::test]
    async fn writes_native_endian_frame() {
        let (mut host, mut client) = duplex(64);
        write_frame(&mut host, b"{}").await.expect("write");

        let mut header = [0_u8; 4];
        client.read_exact(&mut header).await.expect("header");
        assert_eq!(u32::from_ne_bytes(header), 2);
        let mut payload = [0_u8; 2];
        client.read_exact(&mut payload).await.expect("payload");
        assert_eq!(&payload, b"{}");
    }
}
