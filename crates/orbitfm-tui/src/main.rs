//! `orbitfm-tui` — лёгкий терминальный клиент к ядру OrbitFM (ADR-0047).
//!
//! Аудио, MPRIS и библиотека остаются за ядром `orbitfm`; TUI не содержит
//! второго плеера. Здесь — терминальный жизненный цикл, CLI и event loop;
//! состояние в `App`, эффекты в `Runtime`.

mod app;
mod config;
mod core_client;
mod model;
mod runtime;
mod text;
mod theme;
mod ui;
mod version;
mod visualizer;

use std::io::{self, Stdout};
use std::panic;
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use crate::app::{App, AppEvent};
use crate::runtime::Runtime;
use crate::theme::Theme;

type Backend = CrosstermBackend<Stdout>;

fn main() -> io::Result<()> {
    if handle_cli() {
        return Ok(());
    }

    install_panic_hook();
    let _guard = TerminalGuard::enter()?;

    let backend = Backend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let loaded = config::load();
    let theme = Theme::from_config(&loaded.config.ui.theme, &loaded.config.ui.glyphs);
    let mut app = App::with_config(loaded.config.clone());
    app.config_note = loaded.note();

    let mut runtime = Runtime::start();

    while !app.should_quit {
        terminal.draw(|frame| ui::draw(frame, &app, &theme))?;

        let tick = Duration::from_millis(app.motion.tick_ms());
        if event::poll(tick)?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            let actions = app.on_key(key);
            runtime.dispatch(actions, &mut app);
        }

        while let Some(event) = runtime.try_core() {
            app.apply_event(AppEvent::Core(event));
        }
        while let Some(event) = runtime.try_event() {
            app.apply_event(event);
        }

        if app.should_reconnect(Instant::now()) {
            app.begin_reconnect();
            runtime.connect();
        }

        // Отложенная запись конфига (debounce 500 мс).
        if app.pending_save(Instant::now()) {
            if config::save(&app.config).is_err() {
                app.status_note = Some("config: save failed".to_owned());
            }
            app.mark_saved();
        }

        app.on_tick(tick.as_secs_f32());
    }

    Ok(())
}

/// Обрабатывает `--version`/`--help`. `true` — TUI не запускается.
fn handle_cli() -> bool {
    match std::env::args().nth(1).as_deref() {
        Some("-V" | "--version") => {
            print!("{}", version::long());
            true
        }
        Some("-h" | "--help") => {
            print!("{}", help_text());
            true
        }
        Some(other) => {
            eprintln!("orbitfm-tui: unknown argument `{other}`");
            eprintln!("try `orbitfm-tui --help`");
            std::process::exit(2);
        }
        None => false,
    }
}

fn help_text() -> String {
    format!(
        "orbitfm-tui {} — lightweight terminal client for OrbitFM\n\
         \n\
         USAGE:\n\
         \x20   orbitfm-tui\n\
         \n\
         OPTIONS:\n\
         \x20   -h, --help       Print help\n\
         \x20   -V, --version    Print version and compatibility\n\
         \n\
         KEYS:\n\
         \x20   j/k move · enter play · p/space pause · s stop\n\
         \x20   [/] prev/next · r roulette · +/- volume\n\
         \x20   / search · f favorites · h history · a favorite\n\
         \x20   t/T sleep timer · R reconnect · ? help\n\
         \x20   q quit TUI (radio keeps playing) · Q quit the core\n\
         \n\
         The core `orbitfm` must already be running; this client does not start it.\n",
        version::semver()
    )
}

/// Восстанавливает терминал при выходе (в т.ч. по панике).
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

/// Паника не должна оставлять терминал в raw mode.
fn install_panic_hook() {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        previous(info);
    }));
}
