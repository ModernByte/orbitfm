//! Поток событий состояния `events` (ADR-0039, интерфейс 13).
//!
//! Core публикует последний полный кадр `status --json` в watch-канал, а
//! сервер раздаёт его подписчикам в формате NDJSON. Журнала нет: медленный
//! клиент видит последнее состояние, а не очередь промежуточных.

use std::time::Duration;

use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::watch;

/// Версия конверта потока. Растёт при смене формы кадров, а не полей `status`.
pub(crate) const EVENTS_SCHEMA: u32 = 1;
/// Маркер тишины: столько секунд без изменений — и подписчик получает
/// `keepalive`, чтобы отличать простой от зависшего соединения.
pub(crate) const KEEPALIVE: Duration = Duration::from_secs(15);
/// Потолок одновременных подписок. Общий лимит IPC-соединений выше, поэтому
/// one-shot командам всегда остаётся не меньше 48 слотов.
pub(crate) const MAX_SUBSCRIBERS: usize = 16;

/// Публикатор состояния: хранит последний кадр и будит подписчиков.
pub(crate) struct StatusFeed {
    sender: watch::Sender<Option<String>>,
}

impl StatusFeed {
    pub(crate) fn new() -> Self {
        let (sender, _) = watch::channel(None);
        Self { sender }
    }

    /// Новый получатель видит текущий кадр сразу; `None` — Core ещё не
    /// опубликовал первый снимок, сервер дождётся его.
    pub(crate) fn subscribe(&self) -> watch::Receiver<Option<String>> {
        self.sender.subscribe()
    }

    /// Публикует полный кадр. Повтор того же кадра подписчиков не будит.
    pub(crate) fn publish(&self, status: String) {
        self.sender.send_if_modified(|current| {
            if current.as_deref() == Some(status.as_str()) {
                false
            } else {
                *current = Some(status);
                true
            }
        });
    }
}

/// Публикатор результатов companion-поиска (ADR-0039, ревизия 0.16).
///
/// В отличие от [`StatusFeed`] кадр не повторяется новому подписчику: попап
/// получает только ответы на свои запросы, а не чужую прошлую выдачу.
pub(crate) struct SearchFeed {
    sender: watch::Sender<Option<String>>,
}

impl SearchFeed {
    pub(crate) fn new() -> Self {
        let (sender, _) = watch::channel(None);
        Self { sender }
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<Option<String>> {
        self.sender.subscribe()
    }

    /// Публикует кадр `search`; новый id всегда меняет значение.
    pub(crate) fn publish(&self, frame: String) {
        self.sender.send_replace(Some(frame));
    }
}

/// Один кадр NDJSON. `status` уже сериализован `serde_json`, поэтому строка
/// собирается без повторного разбора.
pub(crate) fn frame(kind: &str, status: Option<&str>) -> String {
    let mut line = format!("{{\"ev\":{EVENTS_SCHEMA},\"type\":\"{kind}\"");
    if let Some(status) = status {
        line.push_str(",\"status\":");
        line.push_str(status);
    }
    line.push('}');
    line
}

/// Пишет снимок, затем изменения и keepalive, пока клиент не отключится или
/// сервер не попросит остановиться.
pub(super) async fn stream<W>(
    writer: &mut W,
    status: watch::Receiver<Option<String>>,
    mut search: watch::Receiver<Option<String>>,
    mut stop: watch::Receiver<bool>,
    keepalive: Duration,
) -> Result<(), std::io::Error>
where
    W: AsyncWrite + Unpin,
{
    let mut status = status;
    if status.wait_for(Option::is_some).await.is_err() {
        return Ok(());
    }
    let snapshot = status
        .borrow_and_update()
        .clone()
        .expect("значение проверено wait_for");
    write_line(writer, &frame("snapshot", Some(&snapshot))).await?;

    loop {
        tokio::select! {
            changed = status.changed() => {
                if changed.is_err() {
                    return Ok(());
                }
                let state = status.borrow_and_update().clone();
                let Some(state) = state else { continue };
                write_line(writer, &frame("state", Some(&state))).await?;
            }
            changed = search.changed() => {
                if changed.is_err() {
                    return Ok(());
                }
                let event = search.borrow_and_update().clone();
                let Some(event) = event else { continue };
                write_line(writer, &event).await?;
            }
            _ = tokio::time::sleep(keepalive) => {
                write_line(writer, &frame("keepalive", None)).await?;
            }
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow_and_update() {
                    return Ok(());
                }
            }
        }
    }
}

async fn write_line<W>(writer: &mut W, line: &str) -> Result<(), std::io::Error>
where
    W: AsyncWrite + Unpin,
{
    writer.write_all(line.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::sync::watch;

    use super::*;

    #[test]
    fn frames_carry_envelope_and_status() {
        assert_eq!(
            frame("snapshot", Some("{\"schema\":1}")),
            "{\"ev\":1,\"type\":\"snapshot\",\"status\":{\"schema\":1}}"
        );
        assert_eq!(
            frame("keepalive", None),
            "{\"ev\":1,\"type\":\"keepalive\"}"
        );
    }

    #[test]
    fn publishing_the_same_frame_does_not_notify() {
        let feed = StatusFeed::new();
        let mut receiver = feed.subscribe();
        feed.publish("{\"schema\":1}".to_owned());
        assert!(receiver.has_changed().unwrap());
        receiver.borrow_and_update();
        feed.publish("{\"schema\":1}".to_owned());
        assert!(!receiver.has_changed().unwrap());
        feed.publish("{\"schema\":1,\"volume\":0.5}".to_owned());
        assert!(receiver.has_changed().unwrap());
    }

    #[tokio::test]
    async fn stream_starts_with_snapshot_and_emits_state_and_keepalive() {
        let feed = StatusFeed::new();
        feed.publish("{\"schema\":1}".to_owned());
        let (stop_tx, stop_rx) = watch::channel(false);
        let status = feed.subscribe();
        let search_feed = SearchFeed::new();
        let search = search_feed.subscribe();
        let (mut client, mut server) = tokio::io::duplex(4096);
        let task = tokio::spawn(async move {
            stream(
                &mut server,
                status,
                search,
                stop_rx,
                Duration::from_millis(30),
            )
            .await
        });

        let mut reader = BufReader::new(&mut client);
        let mut line = String::new();
        reader.read_line(&mut line).await.expect("snapshot");
        assert!(
            line.starts_with("{\"ev\":1,\"type\":\"snapshot\""),
            "{line}"
        );

        feed.publish("{\"schema\":1,\"state\":\"playing\"}".to_owned());
        line.clear();
        reader.read_line(&mut line).await.expect("state");
        assert!(line.starts_with("{\"ev\":1,\"type\":\"state\""), "{line}");
        assert!(line.contains("\"state\":\"playing\""), "{line}");

        line.clear();
        reader.read_line(&mut line).await.expect("keepalive");
        assert_eq!(line, "{\"ev\":1,\"type\":\"keepalive\"}\n");

        stop_tx.send(true).expect("stop");
        drop(reader);
        drop(client);
        let _ = task.await;
    }

    #[tokio::test]
    async fn stream_waits_for_the_first_published_frame() {
        let feed = StatusFeed::new();
        let (stop_tx, stop_rx) = watch::channel(false);
        let status = feed.subscribe();
        let search_feed = SearchFeed::new();
        let search = search_feed.subscribe();
        let (mut client, mut server) = tokio::io::duplex(4096);
        let task = tokio::spawn(async move {
            stream(
                &mut server,
                status,
                search,
                stop_rx,
                Duration::from_secs(60),
            )
            .await
        });

        let mut reader = BufReader::new(&mut client);
        let mut line = String::new();
        assert!(
            tokio::time::timeout(Duration::from_millis(50), reader.read_line(&mut line))
                .await
                .is_err(),
            "без первого кадра сервер молчит"
        );

        feed.publish("{\"schema\":1,\"state\":\"stopped\"}".to_owned());
        line.clear();
        reader.read_line(&mut line).await.expect("snapshot");
        assert!(line.contains("\"type\":\"snapshot\""), "{line}");

        stop_tx.send(true).expect("stop");
        drop(reader);
        drop(client);
        let _ = task.await;
    }

    #[tokio::test]
    async fn stream_forwards_search_frames_without_replaying_old_ones() {
        let feed = StatusFeed::new();
        feed.publish("{\"schema\":1}".to_owned());
        let search_feed = SearchFeed::new();
        search_feed.publish("{\"ev\":1,\"type\":\"search\",\"id\":1}".to_owned());
        let (stop_tx, stop_rx) = watch::channel(false);
        let status = feed.subscribe();
        let search = search_feed.subscribe();
        let (mut client, mut server) = tokio::io::duplex(4096);
        let task = tokio::spawn(async move {
            stream(
                &mut server,
                status,
                search,
                stop_rx,
                Duration::from_secs(60),
            )
            .await
        });

        let mut reader = BufReader::new(&mut client);
        let mut line = String::new();
        reader.read_line(&mut line).await.expect("snapshot");
        assert!(line.contains("\"type\":\"snapshot\""), "{line}");
        assert!(
            tokio::time::timeout(Duration::from_millis(50), reader.read_line(&mut line))
                .await
                .is_err(),
            "прошлый кадр поиска не переигрывается"
        );

        search_feed.publish("{\"ev\":1,\"type\":\"search\",\"id\":2}".to_owned());
        line.clear();
        reader.read_line(&mut line).await.expect("search");
        assert!(line.contains("\"type\":\"search\""), "{line}");
        assert!(line.contains("\"id\":2"), "{line}");

        stop_tx.send(true).expect("stop");
        drop(reader);
        drop(client);
        let _ = task.await;
    }
}
