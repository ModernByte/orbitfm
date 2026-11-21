//! Рантайм TUI: владеет клиентом ядра, потоком событий и worker'ом команд.
//!
//! Разделяет «состояние» (`App`) и «эффекты» (процессы, каналы). UI-код не
//! знает о процессах; `App` не знает о потоках.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::app::{Action, App, AppEvent};
use crate::core_client::{
    CliCoreClient, CoreClient, CoreCommand, CoreError, CoreEvent, CoreReply, EventStream,
};

/// Рантайм: клиент, поток событий и канал команд.
pub struct Runtime {
    client: CliCoreClient,
    app_tx: Sender<AppEvent>,
    app_rx: Receiver<AppEvent>,
    command_tx: Sender<CoreCommand>,
    events: Option<EventStream>,
}

impl Runtime {
    /// Поднять рантайм и сразу подключиться к ядру.
    pub fn start() -> Self {
        let client = CliCoreClient::discover();
        let (app_tx, app_rx) = mpsc::channel();
        let (command_tx, command_rx) = mpsc::channel::<CoreCommand>();
        spawn_worker(client.clone(), app_tx.clone(), command_rx);
        let mut runtime = Self {
            client,
            app_tx,
            app_rx,
            command_tx,
            events: None,
        };
        runtime.connect();
        runtime
    }

    /// Переподключиться: старый приёмник событий бросается (читающий поток
    /// завершится и убьёт свой дочерний процесс), затем стартует новый.
    pub fn connect(&mut self) {
        self.events = None;
        match self.client.start_events(false) {
            Ok(stream) => self.events = Some(stream),
            Err(error) => {
                let _ = self.app_tx.send(AppEvent::Command(Err(error)));
            }
        }
    }

    /// Неблокирующее событие ядра.
    pub fn try_core(&mut self) -> Option<CoreEvent> {
        self.events
            .as_ref()
            .and_then(|stream| stream.try_recv().ok())
    }

    /// Неблокирующий результат команды.
    pub fn try_event(&self) -> Option<AppEvent> {
        self.app_rx.try_recv().ok()
    }

    /// Отправить команду в worker (не блокирует UI).
    pub fn send_command(&self, command: CoreCommand) {
        let _ = self.command_tx.send(command);
    }

    /// Исполнить действия `App`.
    pub fn dispatch(&mut self, actions: Vec<Action>, app: &mut App) {
        for action in actions {
            match action {
                Action::Command(command) => self.send_command(command),
                Action::PlayResult(uuid) => self.send_command(CoreCommand::PlayResult(uuid)),
                Action::Reconnect => {
                    self.connect();
                    app.begin_reconnect();
                }
                Action::Quit => app.should_quit = true,
            }
        }
    }
}

fn spawn_worker(mut client: CliCoreClient, app_tx: Sender<AppEvent>, rx: Receiver<CoreCommand>) {
    thread::spawn(move || {
        while let Ok(command) = rx.recv() {
            let result = run_with_fallback(&mut client, &command);
            if app_tx.send(AppEvent::Command(result)).is_err() {
                break;
            }
        }
    });
}

/// `play-result` при ошибке станции повторяется один раз как `play <uuid>`.
fn run_with_fallback(
    client: &mut CliCoreClient,
    command: &CoreCommand,
) -> Result<CoreReply, CoreError> {
    let result = client.execute(command.clone());
    if let (CoreCommand::PlayResult(uuid), Err(CoreError::Command(_))) = (command, &result) {
        return client.execute(CoreCommand::Play(uuid.clone()));
    }
    result
}
