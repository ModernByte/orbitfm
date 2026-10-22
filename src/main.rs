use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use tracing::warn;
use tracing_subscriber::EnvFilter;

mod app;
mod audio;
mod ipc;
mod media;

use app::{OrbitApp, WakeHandle};
use audio::AudioEngine;
use ipc::{IpcCommand, IpcError, IpcServer};
use media::MediaControls;

const APP_ID: &str = "org.modernbyte.OrbitFM";
const SERVER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const RETRY_ATTEMPTS: usize = 5;
const RETRY_DELAY: Duration = Duration::from_millis(50);
const HELP: &str = "\
OrbitFM — 3D Interactive Global Radio Player

Использование:
  orbitfm [КОМАНДА]

Команды:
  toggle            Показать/скрыть окно (по умолчанию)
  show              Показать окно
  hide              Скрыть окно
  status            Состояние приложения
  play <uuid>       Воспроизвести станцию (после подключения Radio Browser)
  stop              Остановить воспроизведение
  volume <0.0-1.0>  Установить громкость
  quit              Завершить процесс

Опции:
  -h, --help        Показать эту справку
  -V, --version     Показать версию

Без аргументов команда toggle применяется к запущенному экземпляру;
если он отсутствует, OrbitFM запускается с видимым окном.";

enum Invocation {
    Help,
    Version,
    Command { command: IpcCommand, explicit: bool },
}

fn main() -> Result<()> {
    init_tracing();

    let Invocation::Command { command, explicit } = parse_invocation()? else {
        return Ok(());
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("orbitfm-async")
        .enable_all()
        .build()
        .context("не удалось создать async runtime")?;

    let result = route_command(&runtime, command, explicit);
    drop(runtime);
    result
}

fn parse_invocation() -> Result<Invocation> {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|arg| {
            arg.into_string()
                .map_err(|_| anyhow!("аргументы командной строки должны быть в UTF-8"))
        })
        .collect::<Result<_>>()?;

    match args.first().map(String::as_str) {
        Some("-h" | "--help") => {
            println!("{HELP}");
            Ok(Invocation::Help)
        }
        Some("-V" | "--version") => {
            println!("orbitfm {}", env!("CARGO_PKG_VERSION"));
            Ok(Invocation::Version)
        }
        _ => {
            let explicit = !args.is_empty();
            let wire = if explicit {
                args.join(" ")
            } else {
                "toggle".to_owned()
            };
            match IpcCommand::parse(&wire) {
                Ok(IpcCommand::Raw(_)) => {
                    bail!("неизвестная команда: {wire}\n\n{HELP}")
                }
                Ok(command) => Ok(Invocation::Command { command, explicit }),
                Err(err) => bail!("{err}\n\n{HELP}"),
            }
        }
    }
}

fn route_command(
    runtime: &tokio::runtime::Runtime,
    command: IpcCommand,
    explicit: bool,
) -> Result<()> {
    let wire = command.to_wire();
    match runtime.block_on(IpcServer::send_to_existing(&wire)) {
        Ok(response) => return handle_response(&response, explicit),
        Err(IpcError::NotRunning) => {}
        Err(err @ IpcError::SocketPath(_)) => {
            return Err(err).context("не удалось определить путь IPC-сокета");
        }
        Err(err) => return Err(err).context("не удалось связаться с запущенным OrbitFM"),
    }

    if !matches!(&command, IpcCommand::Toggle | IpcCommand::Show) {
        bail!("OrbitFM не запущен");
    }

    launch(runtime, &command, explicit)
}

fn launch(runtime: &tokio::runtime::Runtime, command: &IpcCommand, explicit: bool) -> Result<()> {
    let wake = WakeHandle::default();
    let mut server = match runtime.block_on(IpcServer::try_bind()) {
        Ok(server) => server,
        Err(IpcError::AlreadyRunning) => {
            return resend_to_starting_instance(runtime, command, explicit);
        }
        Err(err) => return Err(err).context("не удалось занять IPC-сокет"),
    };

    let commands = server.commands();
    let audio = AudioEngine::new().context("не удалось инициализировать аудио-ядро")?;
    let media = match MediaControls::new(wake.waker()) {
        Ok(media) => Some(media),
        Err(err) => {
            warn!(error = %err, "медиа-контролы недоступны, продолжаем без MPRIS");
            None
        }
    };

    let mut audio_events = audio.subscribe();
    let wake_audio = wake.waker();
    runtime.spawn(async move {
        loop {
            match audio_events.recv().await {
                Ok(_) => wake_audio(),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    warn!(skipped, "часть аудио-событий пропущена");
                    wake_audio();
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let mut server_task = runtime.spawn(server.run(wake.waker(), shutdown_rx));

    let gui_result = run_gui(commands, wake, audio, media, runtime.handle().clone());

    let _ = shutdown_tx.send(());
    let server_exit = runtime
        .block_on(async { tokio::time::timeout(SERVER_SHUTDOWN_TIMEOUT, &mut server_task).await });
    match server_exit {
        Ok(Ok(())) => {}
        Ok(Err(err)) if err.is_cancelled() => {}
        Ok(Err(err)) => warn!(error = %err, "IPC-сервер завершился неожиданно"),
        Err(_) => {
            warn!("IPC-сервер не завершился вовремя, принудительная остановка");
            server_task.abort();
            let _ = runtime.block_on(&mut server_task);
        }
    }

    gui_result
}

fn resend_to_starting_instance(
    runtime: &tokio::runtime::Runtime,
    command: &IpcCommand,
    explicit: bool,
) -> Result<()> {
    let wire = command.to_wire();
    let mut last_error = None;

    for attempt in 0..RETRY_ATTEMPTS {
        match runtime.block_on(IpcServer::send_to_existing(&wire)) {
            Ok(response) => return handle_response(&response, explicit),
            Err(err @ (IpcError::NotRunning | IpcError::Connect(_))) => {
                last_error = Some(err);
                if attempt + 1 < RETRY_ATTEMPTS {
                    std::thread::sleep(RETRY_DELAY);
                }
            }
            Err(err) => {
                return Err(err).context("экземпляр появился во время запуска, но недоступен");
            }
        }
    }

    match last_error {
        Some(err) => Err(err).context("экземпляр появился во время запуска, но недоступен"),
        None => Err(anyhow!("не удалось связаться с запускаемым экземпляром")),
    }
}

fn run_gui(
    commands: tokio::sync::mpsc::Receiver<ipc::IpcRequest>,
    wake: WakeHandle,
    audio: AudioEngine,
    media: Option<MediaControls>,
    runtime: tokio::runtime::Handle,
) -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("OrbitFM")
            .with_app_id(APP_ID)
            .with_inner_size([920.0, 660.0])
            .with_min_inner_size([720.0, 480.0])
            .with_transparent(true),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "OrbitFM",
        options,
        Box::new(move |cc| {
            Ok(Box::new(OrbitApp::new(
                cc, commands, &wake, audio, media, runtime,
            )))
        }),
    )
    .map_err(|err| anyhow!("не удалось запустить GUI: {err}"))
}

fn handle_response(response: &str, print: bool) -> Result<()> {
    if response.trim_start().starts_with("error") {
        bail!("{response}");
    }
    if print {
        println!("{response}");
    }
    Ok(())
}

fn init_tracing() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("orbitfm=info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .with_writer(std::io::stderr)
        .init();
}
