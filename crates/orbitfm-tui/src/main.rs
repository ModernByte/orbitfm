//! `orbitfm-tui` — лёгкий терминальный клиент к ядру OrbitFM (ADR-0047).
//!
//! Волна 1: CLI (`--version`/`--help`), чтение `tui.toml`, терминальный
//! жизненный цикл (raw mode, alternate screen, guard, panic hook), тема и
//! каркас кадра. Аудио, MPRIS и библиотека остаются за ядром `orbitfm`; TUI
//! не содержит второго плеера.

mod app;
mod config;
mod theme;
mod ui;
mod version;

use std::io::{self, Stdout};
use std::panic;
use std::time::Duration;

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use crate::app::App;
use crate::theme::Theme;

type Backend = CrosstermBackend<Stdout>;

/// Частота тика волны 1: до визуализаторов — 4 FPS-подобная, без перегрева.
const TICK: Duration = Duration::from_millis(250);

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
    let mut app = App::new();
    app.config_note = loaded.note();

    while !app.should_quit {
        terminal.draw(|frame| ui::draw(frame, &app, &theme))?;

        if event::poll(TICK)?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.on_key(key);
        }
        app.on_tick();
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
         KEYS (wave 1):\n\
         \x20   q / Ctrl-C  quit TUI (radio keeps playing)\n\
         \x20   space       play/pause\n\
         \x20   a           toggle favorite\n\
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
