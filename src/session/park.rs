//! Простой без окна: `tokio::select!` по всем внешним источникам событий
//! (ADR-0018 п. 4, ADR-0020 п. 3).
//!
//! Пока root-вьюпорт закрыт, UI нет, но приложение обязано оставаться
//! управляемым: IPC, MPRIS, трей и ответ фоновой рулетки сводятся здесь в
//! очередь событий, а исполняются обычными синхронными методами `Session` —
//! аудио-ядро зовёт `Handle::block_on`, поэтому исполнение нельзя оставлять
//! внутри async-ветки.

use std::time::Instant;

use tokio::sync::{broadcast, mpsc};

use crate::audio::AudioEvent;
use crate::ipc::{IpcCommand, IpcRequest};
use crate::media::{MediaCommand, MediaControls};
use crate::services::stream::{ResolvedStream, StreamError};
use crate::services::{RadioError, Station};
use crate::tray::{TrayCommand, TrayHandle};

use super::UiIntent;
use super::companion::CompanionResponse;
use super::stream_import::PendingStream;

/// Что делать главному циклу после простоя.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParkOutcome {
    /// Нужно снова открыть окно.
    Show,
    /// Приложение попросило выйти.
    Quit,
    /// Открыть окно настроек вместо стола (ADR-0053 §6b).
    Settings,
    /// Открыть окно предупреждения таймера сна вместо стола (ADR-0058).
    Warn,
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
    /// Завершился резолв `play-url`: клиент ждёт ответа с открытым соединением.
    Stream(Option<Result<ResolvedStream, StreamError>>),
    /// Ответ поиска попапа: `None` — фоновая задача исчезла.
    CompanionSearch(Option<CompanionResponse>),
    /// IPC-сервер закрыл очередь: держать процесс без смысла нечем.
    QueueClosed,
    /// Дедлайн таймера сна: эфир останавливается без кадра окна.
    Sleep,
    /// Пора сбросить статистику прослушивания: секунды считаются и в простое
    /// без окна (ADR-0049 § 4).
    Stats,
}

impl super::Session {
    /// Простой без окна: ждём IPC, MPRIS, трей, аудио-события и результат
    /// рулетки, пока не попросят показать окно или выйти.
    pub(crate) fn park(&mut self) -> ParkOutcome {
        loop {
            let sleep_deadline = self.sleep_next_event();
            let stats_deadline = self.stats_deadline();
            let event = self.runtime.block_on(wait_on(
                &mut self.commands,
                &mut self.audio_events,
                &mut self.media,
                &mut self.tray,
                &mut self.search.roulette_rx,
                &mut self.pending_stream,
                &mut self.companion.rx,
                sleep_deadline,
                stats_deadline,
            ));
            match event {
                Parked::Ipc(request) => {
                    // Импорт потока не блокирует простой: ответ уйдёт после
                    // резолва, а окно при этом может оставаться скрытым.
                    if matches!(request.command(), IpcCommand::PlayUrl { .. }) {
                        self.begin_play_url(request);
                    } else {
                        let response = self.execute(request.command());
                        request.try_respond(response);
                    }
                }
                Parked::Audio(event) => self.apply_audio_event(&event),
                Parked::Media(command) => self.execute_media_command(command),
                Parked::Tray(command) => self.execute_tray_command(command),
                Parked::Roulette(outcome) => self.apply_roulette(*outcome),
                Parked::Stream(outcome) => self.finish_stream_import(outcome),
                Parked::CompanionSearch(response) => {
                    self.apply_companion_response(response);
                }
                Parked::QueueClosed => self.quit(),
                Parked::Sleep => self.poll_sleep_events(),
                Parked::Stats => {}
            }
            // Секунды эфира считаются и без кадра окна, иначе спрятанное окно
            // обнулило бы счётчики прослушанного (ADR-0049 § 1).
            self.poll_stats();
            self.sync_media();
            self.sync_tray();
            // Подписки `events` получают изменения и без окна (ADR-0039).
            self.publish_status();
            self.sync_inhibition();
            if self.is_quitting() {
                return ParkOutcome::Quit;
            }
            match self.window.take_intent() {
                Some(UiIntent::Show) => return ParkOutcome::Show,
                Some(UiIntent::Settings) => return ParkOutcome::Settings,
                Some(UiIntent::Warn) => return ParkOutcome::Warn,
                Some(UiIntent::Hide) | Some(UiIntent::Close) | None => {}
            }
        }
    }
}

#[allow(clippy::too_many_arguments)] // все ветки select живут в одном месте
async fn wait_on(
    commands: &mut mpsc::Receiver<IpcRequest>,
    audio_events: &mut broadcast::Receiver<AudioEvent>,
    media: &mut Option<MediaControls>,
    tray: &mut Option<TrayHandle>,
    roulette: &mut Option<mpsc::UnboundedReceiver<Result<Station, RadioError>>>,
    stream: &mut Option<PendingStream>,
    companion: &mut Option<mpsc::UnboundedReceiver<CompanionResponse>>,
    sleep_deadline: Option<Instant>,
    stats_deadline: Option<Instant>,
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
            outcome = stream_recv(stream) => Parked::Stream(outcome),
            response = companion_recv(companion) => Parked::CompanionSearch(response),
            () = sleep_until(sleep_deadline) => Parked::Sleep,
            () = sleep_until(stats_deadline) => Parked::Stats,
        };
        return event;
    }
}

/// Таймер сна: без дедлайна ветка не готова никогда.
async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => {
            tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
        }
        None => std::future::pending().await,
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

/// Результат резолва `play-url`; без активного импорта ветка не готова никогда.
async fn stream_recv(
    pending: &mut Option<PendingStream>,
) -> Option<Result<ResolvedStream, StreamError>> {
    match pending {
        Some(pending) => pending.rx.recv().await,
        None => std::future::pending().await,
    }
}

/// Ответ поиска попапа; без запущенного поиска ветка не готова никогда.
async fn companion_recv(
    companion: &mut Option<mpsc::UnboundedReceiver<CompanionResponse>>,
) -> Option<CompanionResponse> {
    match companion {
        Some(receiver) => receiver.recv().await,
        None => std::future::pending().await,
    }
}
