use std::io::IsTerminal;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

mod app;
mod audio;
mod geo;
mod i18n;
mod integration;
mod ipc;
mod library;
mod media;
mod native_host;
mod render;
mod services;
mod session;
mod settings;
mod station_filter;
mod storage;
mod text;
mod tray;
mod ui;

use app::OrbitApp;
use audio::AudioEngine;
use geo::GeoService;
use ipc::{IpcCommand, IpcError, IpcServer, SearchFeed, StatusFeed};
use library::StationLibrary;
use media::MediaControls;
use services::RadioBrowserClient;
use session::{AppServices, ParkOutcome, Session, WINDOW_TITLE, WakeHandle, WindowMode};
use settings::SettingsStore;

const APP_ID: &str = "org.modernbyte.OrbitFM";
/// Секция About модального окна настроек (ADR-0031): версия и лицензия из манифеста.
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const APP_LICENSE: &str = env!("CARGO_PKG_LICENSE");
const SERVER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const RETRY_ATTEMPTS: usize = 5;
const RETRY_DELAY: Duration = Duration::from_millis(50);
const HELP: &str = "\
OrbitFM — 3D Interactive Global Radio Player

Использование:
  orbitfm [КОМАНДА]

Команды:
  toggle               Показать/скрыть окно (по умолчанию)
  show                 Показать окно
  hide                 Скрыть окно
  panel                Показать/скрыть выпадающую панель (бар-виджет)
  overlay              Показать/скрыть капсулу Overlay (CLI)
  status               Состояние приложения
  status --json        Состояние приложения в JSON (для интеграций)
  events               Поток состояния NDJSON (подписка для интеграций)
  integration status [цель]
                       Состояние интеграций оболочек и баров
  integration install <цель> [--force]
                       Установить интеграцию в каталоги пользователя
  integration uninstall <цель> [--force]
                       Удалить файлы интеграции по квитанции
  native-host          stdio-хост для расширений браузеров (служебная)
  mode [режим]         Текущий режим окна или focus/overlay/listening/panel
  play <uuid>          Воспроизвести станцию по UUID (Radio Browser)
  play-url <url>       Импортировать внешний поток: .m3u, .pls или страница
  search <запрос>      Поиск для браузерного попапа
  library              Избранное и история (JSON для расширения)
  favorite [uuid]      Переключить избранное текущей станции или по UUID
  play-result <uuid>   Играть станцию из последней выдачи попапа
  play-library <uuid>  Играть станцию из избранного или истории
  playpause            Пуск/пауза
  next                 Следующая станция
  previous             Предыдущая станция
  roulette             Радиорулетка
  stop                 Остановить воспроизведение
  volume <0.0-1.0>     Установить громкость
  volume-up            Громче на один шаг
  volume-down          Тише на один шаг
  theme <режим>        Тема: system, dark или light
  settings             Открыть настройки
  quit                 Завершить процесс

Опции:
  -h, --help        Показать эту справку
  -V, --version     Показать версию

Без аргументов команда toggle применяется к запущенному экземпляру;
если он отсутствует, OrbitFM запускается с видимым окном.
Команда mode без аргумента спрашивает режим; mode <режим> при отсутствии
запущенного экземпляра запускает OrbitFM сразу в этом режиме.
Команды search, library, favorite, play-result, play-library и play-url
обслуживают браузерное расширение и не запускают новый экземпляр.";

enum Invocation {
    Help,
    Version,
    Integration(integration::Command),
    NativeHost,
    Command { command: IpcCommand, explicit: bool },
}

fn main() -> Result<()> {
    init_tracing();

    let (command, explicit) = match parse_invocation()? {
        Invocation::Help | Invocation::Version => return Ok(()),
        Invocation::Integration(command) => return integration::run(command),
        Invocation::NativeHost => return native_host::run(),
        Invocation::Command { command, explicit } => (command, explicit),
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("orbitfm-async")
        .enable_all()
        .build()
        .context("не удалось создать async runtime")?;

    let result = route_command(&runtime, &command, explicit);
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
        Some("integration") => integration::parse(&args[1..]).map(Invocation::Integration),
        Some("native-host") => {
            if args.len() > 1 {
                bail!("native-host не принимает аргументов: его запускает браузер");
            }
            Ok(Invocation::NativeHost)
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
    command: &IpcCommand,
    explicit: bool,
) -> Result<()> {
    // Подписка живёт до отключения клиента, поэтому у неё отдельный путь:
    // `send_to_existing` ждёт один ответ и закрыл бы поток (ADR-0039).
    if matches!(command, IpcCommand::Events) {
        return match runtime.block_on(IpcServer::subscribe_to_existing()) {
            Ok(()) => Ok(()),
            Err(IpcError::NotRunning) => bail!("OrbitFM не запущен"),
            Err(err @ IpcError::SocketPath(_)) => {
                Err(err).context("не удалось определить путь IPC-сокета")
            }
            Err(err) => Err(err).context("не удалось подписаться на события OrbitFM"),
        };
    }

    let wire = command.to_wire();
    match runtime.block_on(IpcServer::send_to_existing(&wire)) {
        Ok(response) => return handle_response(&response, explicit),
        Err(IpcError::NotRunning) => {}
        Err(err @ IpcError::SocketPath(_)) => {
            return Err(err).context("не удалось определить путь IPC-сокета");
        }
        Err(err) => return Err(err).context("не удалось связаться с запущенным OrbitFM"),
    }

    if !matches!(
        command,
        IpcCommand::Toggle
            | IpcCommand::Show
            | IpcCommand::Panel
            | IpcCommand::Overlay
            | IpcCommand::SetMode { .. }
            | IpcCommand::Settings
    ) {
        bail!("OrbitFM не запущен");
    }

    launch(runtime, command, explicit)
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
    let geo = GeoService::from_embedded().context("не удалось загрузить границы стран")?;
    // `ksni` через feature-unification включает tokio-бэкенд у zbus, а zbus при
    // регистрации object server вызывает `tokio::task::spawn`, которому нужен
    // контекст рантайма на текущем потоке. Без `enter` старт падает паникой.
    let _media_runtime = runtime.enter();
    let media = match MediaControls::new(wake.waker()) {
        Ok(media) => Some(media),
        Err(err) => {
            warn!(error = %err, "медиа-контролы недоступны, продолжаем без MPRIS");
            None
        }
    };
    drop(_media_runtime);
    // Отдельный `std::thread` внутри: блокирующий ksni входит в свой общий
    // process-global current-thread runtime, поэтому вызов с воркера `runtime`
    // упал бы паникой (ADR-0020, п. 4). Без SNI-хоста возвращает `None`.
    let tray = tray::spawn(APP_ID, WINDOW_TITLE, wake.waker());
    let radio = RadioBrowserClient::new();
    let library = match StationLibrary::load_default() {
        Ok(library) => {
            info!(path = ?library.path(), "библиотека станций загружена");
            library
        }
        Err(err) => {
            warn!(
                error = %err,
                "не удалось загрузить избранное и историю; используется временная библиотека"
            );
            StationLibrary::ephemeral()
        }
    };
    let settings = match SettingsStore::load_default() {
        Ok(settings) => {
            info!(path = ?settings.path(), "настройки оформления загружены");
            settings
        }
        Err(err) => {
            warn!(
                error = %err,
                "не удалось загрузить настройки оформления; используются значения по умолчанию"
            );
            SettingsStore::ephemeral()
        }
    };
    let warm_radio = radio.clone();
    runtime.spawn(async move {
        if let Err(err) = warm_radio.refresh_servers().await {
            warn!(error = %err, "не удалось прогреть пул серверов Radio Browser");
        }
    });

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

    let status = StatusFeed::new();
    let status_rx = status.subscribe();
    let search = SearchFeed::new();
    let search_rx = search.subscribe();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let mut server_task =
        runtime.spawn(server.run(wake.waker(), status_rx, search_rx, shutdown_rx));

    let services = AppServices {
        audio,
        geo,
        radio,
        media,
        tray,
        library,
        settings,
        status,
        search,
        runtime: runtime.handle().clone(),
    };
    // При незапущенном экземпляре `mode <режим>` задаёт режим первого окна (ADR-0021 п. 5),
    // а `panel` и `overlay` открывают сразу свою поверхность (ADR-0033).
    let first_mode = match command {
        IpcCommand::SetMode { mode } => WindowMode::from(*mode),
        IpcCommand::Panel => WindowMode::Panel,
        IpcCommand::Overlay => WindowMode::Overlay,
        _ => WindowMode::default(),
    };
    // Модалка настроек не выражается через режим окна: флаг передаётся в цикл
    // отдельно и открывает её поверх первого показа (ADR-0037).
    let open_settings = matches!(command, IpcCommand::Settings);
    let gui_result = run_session(commands, wake, services, first_mode, open_settings);

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

/// Цикл окна (ADR-0018): показ → скрытие реальным закрытием root-вьюпорта →
/// простой без окна, в котором живут IPC, MPRIS и аудио. Новый показ — это новый
/// прогон `eframe::run_native` из главного потока, поэтому `Session` владеет всем
/// долгоживущим состоянием, а `OrbitApp` создаётся заново каждый раз.
fn run_session(
    commands: tokio::sync::mpsc::Receiver<ipc::IpcRequest>,
    wake: WakeHandle,
    services: AppServices,
    mode: WindowMode,
    open_settings: bool,
) -> Result<()> {
    let mut session = Session::new(commands, wake, services);
    session.set_window_mode(mode);
    if open_settings {
        session.open_settings();
    }

    loop {
        let size = session.restore_size();
        session.begin_window(size);
        session.settle_analysis();
        run_window(&mut session, size)?;
        session.end_window();

        if session.is_quitting() {
            return Ok(());
        }
        if session.take_reopen() {
            // Смена режима при открытом окне: то же закрытие вьюпорта, но без
            // парковки — следующий прогон уже с геометрией нового режима (ADR-0021 п. 3).
            continue;
        }
        // Окно настроек живёт только внутри одного показа окна: hide, WM-close и парковка
        // его закрывают, а mode-switch (строка выше) сохраняет (ADR-0031 п. 4).
        session.close_settings();
        session.settle_analysis();
        if matches!(session.park(), ParkOutcome::Quit) {
            return Ok(());
        }
    }
}

fn run_window(session: &mut Session, size: [f32; 2]) -> Result<()> {
    let title = session.window_mode().title();
    // Капсулы и выпадающая панель — поверхности без полей окна: CSD-полоса
    // заголовка и системная рамка ломали бы стеклянную капсулу (ADR-0021 п. 4).
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title(title)
        .with_app_id(APP_ID)
        .with_inner_size(size)
        .with_min_inner_size(session.window_min_size())
        .with_transparent(true);
    if session.window_mode().is_borderless() {
        viewport = viewport.with_decorations(false);
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        title,
        options,
        Box::new(|cc| Ok(Box::new(OrbitApp::new(cc, session)))),
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
        .with_ansi(std::io::stderr().is_terminal())
        .with_writer(std::io::stderr)
        .init();
}
