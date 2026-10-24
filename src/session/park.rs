//! Простой без окна: `tokio::select!` по всем внешним источникам событий
//! (ADR-0018 п. 4, ADR-0020 п. 3).
//!
//! Пока root-вьюпорт закрыт, UI нет, но приложение обязано оставаться
//! управляемым: IPC, MPRIS, трей и ответ фоновой рулетки сводятся здесь в
//! очередь событий, а исполняются обычными синхронными методами `Session` —
//! аудио-ядро зовёт `Handle::block_on`, поэтому исполнение нельзя оставлять
//! внутри async-ветки.

use tokio::sync::{broadcast, mpsc};

use crate::audio::AudioEvent;
use crate::ipc::IpcRequest;
use crate::media::{MediaCommand, MediaControls};
use crate::services::{RadioError, Station};
use crate::tray::{TrayCommand, TrayHandle};

use super::UiIntent;

/// Что делать главному циклу после простоя.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParkOutcome {
    /// Нужно снова открыть окно.
    Show,
    /// Приложение попросило выйти.
    Quit,
}

/// Событие, разбудившее простаивающее приложение.
enum Parked {
    Ipc(IpcRequest),
    Audio(AudioEvent),
    Media(MediaCommand),
    Tray(TrayCommand),
    /// Ответ фоновой задачи рулетки: `None` — задача исчезла. `Station` велика,
    /// поэтому значение варианта убрано в `Box`.
    Roulette(Box<Option<Result<Station, RadioError>>>),
    /// IPC-сервер закрыл очередь: держать процесс без смысла нечем.
    QueueClosed,
}

impl super::Session {
    /// Простой без окна: ждём IPC, MPRIS, трей, аудио-события и результат
    /// рулетки, пока не попросят показать окно или выйти.
    pub(crate) fn park(&mut self) -> ParkOutcome {
        loop {
            let event = self.runtime.block_on(wait_on(
                &mut self.commands,
                &mut self.audio_events,
                &mut self.media,
                &mut self.tray,
                &mut self.search.roulette_rx,
            ));
            match event {
                Parked::Ipc(request) => {
                    let response = self.execute(request.command());
                    request.try_respond(response);
                }
                Parked::Audio(event) => self.apply_audio_event(event),
                Parked::Media(command) => self.execute_media_command(command),
                Parked::Tray(command) => self.execute_tray_command(command),
                Parked::Roulette(outcome) => self.apply_roulette(*outcome),
                Parked::QueueClosed => self.quit(),
            }
            self.sync_media();
            self.sync_tray();
            if self.is_quitting() {
                return ParkOutcome::Quit;
            }
            if self.window.intent() == Some(UiIntent::Show) {
                return ParkOutcome::Show;
            }
        }
    }
}

async fn wait_on(
    commands: &mut mpsc::Receiver<IpcRequest>,
    audio_events: &mut broadcast::Receiver<AudioEvent>,
    media: &mut Option<MediaControls>,
    tray: &mut Option<TrayHandle>,
    roulette: &mut Option<mpsc::UnboundedReceiver<Result<Station, RadioError>>>,
) -> Parked {
    loop {
        let event = tokio::select! {
            request = commands.recv() => match request {
                Some(request) => Parked::Ipc(request),
                None => return Parked::QueueClosed,
            },
            event = audio_events.recv() => match event {
                Ok(event) => Parked::Audio(event),
                Err(_) => continue,
            },
            command = media_recv(media) => match command {
                Some(command) => Parked::Media(command),
                // Отправитель живёт в `Session`, очередь закрыться не может.
                None => continue,
            },
            command = tray_recv(tray) => Parked::Tray(command),
            outcome = roulette_recv(roulette) => Parked::Roulette(Box::new(outcome)),
        };
        return event;
    }
}

/// Очередь MPRIS-команд; без медиа-контролов эта ветка не готова никогда.
async fn media_recv(media: &mut Option<MediaControls>) -> Option<MediaCommand> {
    match media {
        Some(media) => media.events().recv().await,
        None => std::future::pending().await,
    }
}

/// Очередь команд трея. Мёртвый трей (поток-владелец вышел) выключает ветку,
/// иначе `recv` возвращал бы `None` непрерывно и цикл закрутился бы вхолостую.
async fn tray_recv(tray: &mut Option<TrayHandle>) -> TrayCommand {
    loop {
        let Some(handle) = tray else {
            // Трея нет: ветка не готова никогда, `select!` ждёт остальные источники.
            std::future::pending::<()>().await;
            unreachable!()
        };
        match handle.commands.recv().await {
            Some(command) => return command,
            None => *tray = None,
        }
    }
}

/// Ответ задачи рулетки; без запущенной рулетки ветка не готова никогда.
async fn roulette_recv(
    roulette: &mut Option<mpsc::UnboundedReceiver<Result<Station, RadioError>>>,
) -> Option<Result<Station, RadioError>> {
    match roulette {
        Some(receiver) => receiver.recv().await,
        None => std::future::pending().await,
    }
}
