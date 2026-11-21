//! Состояние приложения TUI: соединение с ядром, экраны, ввод и действия.
//!
//! `App` не запускает процессы — он принимает `AppEvent`, отдаёт `Action`, а
//! исполняет их `Runtime` в `main.rs`. Состояние тестируется без живого ядра и
//! терминала.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::core_client::{CoreCommand, CoreError, CoreEvent, CoreReply};
use crate::model::{LibraryPayload, SearchFrame, SpectrumFrame, Station, StatusSnapshot};

pub use crate::model::PlayState;

/// Экран.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    NowPlaying,
    Search,
    Favorites,
    History,
    Help,
}

/// Оверлей поверх экрана.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Overlay {
    None,
    QuitConfirm,
    SleepPrompt,
}

/// Состояние связи с ядром.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreLink {
    Connecting,
    Live { version: String },
    Disconnected { message: String },
}

/// Событие рантайма для приложения.
#[derive(Debug)]
pub enum AppEvent {
    Core(CoreEvent),
    Command(Result<CoreReply, CoreError>),
}

/// Действие, которое исполняет рантайм.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Command(CoreCommand),
    /// `play-result` с однократным fallback на `play` при ошибке.
    PlayResult(String),
    /// Переподключиться немедленно.
    Reconnect,
    /// Выйти из TUI.
    Quit,
}

const MAX_BACKOFF: Duration = Duration::from_secs(30);
const MIN_BACKOFF: Duration = Duration::from_secs(1);

/// Состояние приложения.
#[derive(Debug)]
pub struct App {
    pub core: CoreLink,
    pub status: Option<StatusSnapshot>,
    pub compat_error: Option<String>,
    pub status_note: Option<String>,
    pub config_note: Option<String>,
    pub screen: Screen,
    pub overlay: Overlay,
    pub search_input: String,
    pub search_editing: bool,
    pub pending_search: Option<u64>,
    pub search: Option<SearchFrame>,
    pub library: LibraryPayload,
    pub spectrum: Option<SpectrumFrame>,
    pub selection: usize,
    pub sleep_input: String,
    pub should_quit: bool,
    /// Закрыть TUI после ответа на `quit` ядра (`Q` + `y`).
    pub quit_after_command: bool,
    pub tick: u64,
    backoff: Duration,
    retry_at: Option<Instant>,
}

impl App {
    pub fn new() -> Self {
        Self {
            core: CoreLink::Connecting,
            status: None,
            compat_error: None,
            status_note: None,
            config_note: None,
            screen: Screen::NowPlaying,
            overlay: Overlay::None,
            search_input: String::new(),
            search_editing: false,
            pending_search: None,
            search: None,
            library: LibraryPayload::default(),
            spectrum: None,
            selection: 0,
            sleep_input: String::new(),
            should_quit: false,
            quit_after_command: false,
            tick: 0,
            backoff: MIN_BACKOFF,
            retry_at: None,
        }
    }

    /// Список активного экрана.
    pub fn active_list(&self) -> &[Station] {
        match self.screen {
            Screen::Favorites => &self.library.favorites,
            Screen::History => &self.library.history,
            Screen::Search => self
                .search
                .as_ref()
                .map_or(&[], |frame| frame.results.as_slice()),
            _ => &[],
        }
    }

    /// Выбранная станция активного экрана.
    pub fn selected_station(&self) -> Option<&Station> {
        self.active_list().get(self.selection)
    }

    /// Ограничить выбор размером списка.
    pub fn clamp_selection(&mut self) {
        let len = self.active_list().len();
        self.selection = self.selection.min(len.saturating_sub(1));
    }

    /// Обработать событие рантайма.
    pub fn apply_event(&mut self, event: AppEvent) {
        if self.quit_after_command {
            match &event {
                AppEvent::Command(Ok(_)) | AppEvent::Command(Err(CoreError::NotRunning)) => {
                    self.should_quit = true;
                    return;
                }
                AppEvent::Command(Err(error)) => {
                    self.quit_after_command = false;
                    self.note(&error.to_string());
                    return;
                }
                _ => {}
            }
        }
        match event {
            AppEvent::Core(CoreEvent::State(snapshot)) => {
                self.compat_error = None;
                self.core = CoreLink::Live {
                    version: snapshot.version.clone(),
                };
                self.status = Some(*snapshot);
                self.backoff = MIN_BACKOFF;
                self.retry_at = None;
            }
            AppEvent::Core(CoreEvent::Search(frame)) => {
                if self.pending_search == Some(frame.id) {
                    self.search = Some(frame);
                    self.pending_search = None;
                    self.selection = 0;
                }
            }
            AppEvent::Core(CoreEvent::Spectrum(frame)) => self.spectrum = Some(frame),
            AppEvent::Core(CoreEvent::Keepalive) => {}
            AppEvent::Core(CoreEvent::Incompatible { message }) => self.set_compat_error(message),
            AppEvent::Core(CoreEvent::Disconnected { message }) => {
                self.disconnected(message, Instant::now());
            }
            AppEvent::Command(Ok(CoreReply::Library(payload))) => {
                self.library = payload;
                self.clamp_selection();
            }
            AppEvent::Command(Ok(CoreReply::Search { id })) => self.pending_search = Some(id),
            AppEvent::Command(Ok(CoreReply::Favorite { favorite })) => self.note(if favorite {
                "added to favorites"
            } else {
                "removed from favorites"
            }),
            AppEvent::Command(Ok(CoreReply::Ok { text })) => {
                if !text.is_empty() {
                    self.note(&text);
                }
            }
            AppEvent::Command(Err(CoreError::NotRunning)) => {
                self.disconnected("core not running".to_owned(), Instant::now());
            }
            AppEvent::Command(Err(error)) => self.note(&error.to_string()),
        }
    }

    /// Установить ошибку совместимости и разорвать управление.
    pub fn set_compat_error(&mut self, message: String) {
        self.status_note = Some(message.clone());
        self.compat_error = Some(message);
    }

    fn disconnected(&mut self, message: String, now: Instant) {
        self.retry_at = Some(now + self.backoff);
        self.backoff = (self.backoff * 2).min(MAX_BACKOFF);
        self.status_note = Some(message.clone());
        self.core = CoreLink::Disconnected { message };
    }

    /// Пора переподключаться?
    pub fn should_reconnect(&self, now: Instant) -> bool {
        self.retry_at.is_some_and(|at| now >= at)
    }

    /// Отметить начало переподключения.
    pub fn begin_reconnect(&mut self) {
        self.retry_at = None;
        self.core = CoreLink::Connecting;
    }

    fn note(&mut self, text: &str) {
        self.status_note = Some(text.to_owned());
    }

    pub fn on_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    /// Обработать клавишу и вернуть действия для рантайма.
    pub fn on_key(&mut self, key: KeyEvent) -> Vec<Action> {
        match self.overlay.clone() {
            Overlay::QuitConfirm => return self.on_quit_confirm(key),
            Overlay::SleepPrompt => return self.on_sleep_prompt(key),
            Overlay::None => {}
        }

        if self.search_editing {
            return self.on_search_input(key);
        }

        // Help-экран закрывается `?`/`Esc`; `q` продолжает работать.
        if self.screen == Screen::Help && matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
            self.screen = Screen::NowPlaying;
            return Vec::new();
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => vec![Action::Quit],
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                vec![Action::Quit]
            }
            KeyCode::Char('Q') => {
                self.overlay = Overlay::QuitConfirm;
                Vec::new()
            }
            KeyCode::Char('?') => {
                self.screen = Screen::Help;
                Vec::new()
            }
            KeyCode::Char('R') => vec![Action::Reconnect],
            KeyCode::Char('p') | KeyCode::Char(' ') => {
                vec![Action::Command(CoreCommand::PlayPause)]
            }
            KeyCode::Char('s') => vec![Action::Command(CoreCommand::Stop)],
            KeyCode::Char('r') => vec![Action::Command(CoreCommand::Roulette)],
            KeyCode::Char('+') | KeyCode::Char('=') => {
                vec![Action::Command(CoreCommand::VolumeUp)]
            }
            KeyCode::Char('-') => vec![Action::Command(CoreCommand::VolumeDown)],
            KeyCode::Char('[') => vec![Action::Command(CoreCommand::Previous)],
            KeyCode::Char(']') => vec![Action::Command(CoreCommand::Next)],
            KeyCode::Char('f') => {
                self.screen = Screen::Favorites;
                self.selection = 0;
                vec![Action::Command(CoreCommand::Library)]
            }
            KeyCode::Char('h') => {
                self.screen = Screen::History;
                self.selection = 0;
                vec![Action::Command(CoreCommand::Library)]
            }
            KeyCode::Char('/') => {
                self.screen = Screen::Search;
                self.search_editing = true;
                self.search_input.clear();
                Vec::new()
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.move_selection(1);
                Vec::new()
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_selection(-1);
                Vec::new()
            }
            KeyCode::Char('g') => {
                self.selection = 0;
                Vec::new()
            }
            KeyCode::Char('G') => {
                self.selection = self.active_list().len().saturating_sub(1);
                Vec::new()
            }
            KeyCode::Enter => self.play_selected(),
            KeyCode::Char('a') => self.toggle_favorite(),
            KeyCode::Char('t') => {
                self.sleep_input.clear();
                self.overlay = Overlay::SleepPrompt;
                Vec::new()
            }
            KeyCode::Char('T') => vec![Action::Command(CoreCommand::ClearSleep)],
            _ => Vec::new(),
        }
    }

    fn on_quit_confirm(&mut self, key: KeyEvent) -> Vec<Action> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.overlay = Overlay::None;
                self.quit_after_command = true;
                vec![Action::Command(CoreCommand::Quit)]
            }
            _ => {
                self.overlay = Overlay::None;
                Vec::new()
            }
        }
    }

    fn on_sleep_prompt(&mut self, key: KeyEvent) -> Vec<Action> {
        match key.code {
            KeyCode::Esc => {
                self.overlay = Overlay::None;
                Vec::new()
            }
            KeyCode::Enter => {
                self.overlay = Overlay::None;
                match self.sleep_input.trim().parse::<u16>() {
                    Ok(minutes) if (1..=720).contains(&minutes) => {
                        vec![Action::Command(CoreCommand::Sleep(minutes))]
                    }
                    _ => {
                        self.note("sleep: enter 1..720");
                        Vec::new()
                    }
                }
            }
            KeyCode::Backspace => {
                self.sleep_input.pop();
                Vec::new()
            }
            KeyCode::Char(ch) if ch.is_ascii_digit() => {
                if self.sleep_input.len() < 3 {
                    self.sleep_input.push(ch);
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn on_search_input(&mut self, key: KeyEvent) -> Vec<Action> {
        match key.code {
            KeyCode::Esc => {
                self.search_editing = false;
                Vec::new()
            }
            KeyCode::Enter => {
                self.search_editing = false;
                let query = self.search_input.trim().to_owned();
                if query.is_empty() || query.chars().count() > 128 {
                    Vec::new()
                } else {
                    vec![Action::Command(CoreCommand::Search(query))]
                }
            }
            KeyCode::Backspace => {
                self.search_input.pop();
                Vec::new()
            }
            KeyCode::Char(ch) => {
                if self.search_input.chars().count() < 128 {
                    self.search_input.push(ch);
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.active_list().len();
        if len == 0 {
            self.selection = 0;
            return;
        }
        let next = self.selection as isize + delta;
        self.selection = next.clamp(0, len as isize - 1) as usize;
    }

    fn play_selected(&mut self) -> Vec<Action> {
        let Some(station) = self.selected_station() else {
            return Vec::new();
        };
        let uuid = station.uuid.clone();
        if uuid.is_empty() {
            return Vec::new();
        }
        match self.screen {
            Screen::Search => vec![Action::PlayResult(uuid)],
            Screen::Favorites | Screen::History => {
                vec![Action::Command(CoreCommand::PlayLibrary(uuid))]
            }
            _ => vec![Action::Command(CoreCommand::Play(uuid))],
        }
    }

    fn toggle_favorite(&mut self) -> Vec<Action> {
        let uuid = self
            .selected_station()
            .map(|station| station.uuid.clone())
            .filter(|uuid| !uuid.is_empty());
        vec![Action::Command(CoreCommand::Favorite(uuid))]
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// `M:SS` / `H:MM:SS` без новых зависимостей.
pub fn format_remaining(secs: u64) -> String {
    let (hours, minutes, seconds) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// Состояние эфира из снимка (или `Stopped` без данных).
pub fn play_state(status: Option<&StatusSnapshot>) -> PlayState {
    status.map_or(PlayState::Stopped, StatusSnapshot::play_state)
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use super::*;
    use crate::model::Can;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn station(uuid: &str, name: &str) -> Station {
        Station {
            uuid: uuid.to_owned(),
            name: name.to_owned(),
            ..Station::default()
        }
    }

    fn snapshot(state: &str) -> StatusSnapshot {
        StatusSnapshot {
            schema: 1,
            app: "orbitfm".to_owned(),
            version: "0.23.0".to_owned(),
            state: state.to_owned(),
            station: None,
            volume: 0.5,
            can: Can::default(),
            sleep: None,
        }
    }

    #[test]
    fn q_quits_without_touching_playback() {
        let mut app = App::new();
        assert_matches!(app.on_key(key(KeyCode::Char('q')))[0], Action::Quit);
    }

    #[test]
    fn q_upper_asks_confirmation_then_quits_core() {
        let mut app = App::new();
        assert!(app.on_key(key(KeyCode::Char('Q'))).is_empty());
        assert_eq!(app.overlay, Overlay::QuitConfirm);
        // Любая клавиша, кроме `y`, отменяет.
        assert!(app.on_key(key(KeyCode::Char('n'))).is_empty());
        assert_eq!(app.overlay, Overlay::None);
        app.on_key(key(KeyCode::Char('Q')));
        assert_eq!(
            app.on_key(key(KeyCode::Char('y'))),
            vec![Action::Command(CoreCommand::Quit)]
        );
    }

    #[test]
    fn transport_keys_map_to_commands() {
        let mut app = App::new();
        assert_eq!(
            app.on_key(key(KeyCode::Char(' '))),
            vec![Action::Command(CoreCommand::PlayPause)]
        );
        assert_eq!(
            app.on_key(key(KeyCode::Char('s'))),
            vec![Action::Command(CoreCommand::Stop)]
        );
        assert_eq!(
            app.on_key(key(KeyCode::Char('+'))),
            vec![Action::Command(CoreCommand::VolumeUp)]
        );
        assert_eq!(
            app.on_key(key(KeyCode::Char('['))),
            vec![Action::Command(CoreCommand::Previous)]
        );
    }

    #[test]
    fn search_input_does_not_trigger_shortcuts() {
        let mut app = App::new();
        app.on_key(key(KeyCode::Char('/')));
        assert!(app.search_editing);
        app.on_key(key(KeyCode::Char('j')));
        assert_eq!(app.search_input, "j");
        let actions = app.on_key(key(KeyCode::Enter));
        assert_eq!(
            actions,
            vec![Action::Command(CoreCommand::Search("j".to_owned()))]
        );
        assert!(!app.search_editing);
    }

    #[test]
    fn enter_plays_selected_per_screen() {
        let mut app = App::new();
        app.screen = Screen::Search;
        app.search = Some(SearchFrame {
            id: 1,
            query: "q".to_owned(),
            results: vec![station("u1", "A")],
            error: None,
        });
        assert_eq!(
            app.on_key(key(KeyCode::Enter)),
            vec![Action::PlayResult("u1".to_owned())]
        );

        app.screen = Screen::Favorites;
        app.library.favorites = vec![station("u2", "B")];
        assert_eq!(
            app.on_key(key(KeyCode::Enter)),
            vec![Action::Command(CoreCommand::PlayLibrary("u2".to_owned()))]
        );
    }

    #[test]
    fn selection_moves_and_clamps() {
        let mut app = App::new();
        app.screen = Screen::Favorites;
        app.library.favorites = vec![station("a", "A"), station("b", "B")];
        app.on_key(key(KeyCode::Char('j')));
        assert_eq!(app.selection, 1);
        app.on_key(key(KeyCode::Char('j')));
        assert_eq!(app.selection, 1);
        app.on_key(key(KeyCode::Char('g')));
        assert_eq!(app.selection, 0);
        app.on_key(key(KeyCode::Char('G')));
        assert_eq!(app.selection, 1);
    }

    #[test]
    fn favorite_uses_selected_uuid() {
        let mut app = App::new();
        app.screen = Screen::Favorites;
        app.library.favorites = vec![station("u9", "N")];
        assert_eq!(
            app.on_key(key(KeyCode::Char('a'))),
            vec![Action::Command(CoreCommand::Favorite(Some(
                "u9".to_owned()
            )))]
        );
    }

    #[test]
    fn sleep_prompt_validates_range() {
        let mut app = App::new();
        app.on_key(key(KeyCode::Char('t')));
        for ch in ['3', '0'] {
            app.on_key(key(KeyCode::Char(ch)));
        }
        assert_eq!(
            app.on_key(key(KeyCode::Enter)),
            vec![Action::Command(CoreCommand::Sleep(30))]
        );

        app.on_key(key(KeyCode::Char('t')));
        app.on_key(key(KeyCode::Char('9')));
        app.on_key(key(KeyCode::Char('9')));
        app.on_key(key(KeyCode::Char('9')));
        assert!(app.on_key(key(KeyCode::Enter)).is_empty());
        assert!(
            app.status_note
                .as_deref()
                .unwrap_or("")
                .starts_with("sleep")
        );
    }

    #[test]
    fn state_event_makes_link_live() {
        let mut app = App::new();
        app.apply_event(AppEvent::Core(CoreEvent::State(Box::new(snapshot(
            "playing",
        )))));
        assert_matches!(app.core, CoreLink::Live { .. });
        assert_eq!(play_state(app.status.as_ref()), PlayState::Playing);
    }

    #[test]
    fn search_frame_requires_matching_id() {
        let mut app = App::new();
        app.pending_search = Some(7);
        let frame = SearchFrame {
            id: 8,
            query: "x".to_owned(),
            results: vec![station("u", "N")],
            error: None,
        };
        app.apply_event(AppEvent::Core(CoreEvent::Search(frame.clone())));
        assert!(app.search.is_none());
        app.apply_event(AppEvent::Core(CoreEvent::Search(SearchFrame {
            id: 7,
            ..frame
        })));
        assert!(app.search.is_some());
        assert!(app.pending_search.is_none());
    }

    #[test]
    fn disconnect_schedules_growing_backoff() {
        let mut app = App::new();
        let now = Instant::now();
        app.apply_event(AppEvent::Core(CoreEvent::Disconnected {
            message: "boom".to_owned(),
        }));
        assert!(!app.should_reconnect(now));
        assert!(app.should_reconnect(now + Duration::from_secs(2)));
        assert!(matches!(app.core, CoreLink::Disconnected { .. }));
    }

    #[test]
    fn format_remaining_forms() {
        assert_eq!(format_remaining(59), "0:59");
        assert_eq!(format_remaining(3600), "1:00:00");
        assert_eq!(format_remaining(43200), "12:00:00");
    }

    #[test]
    fn compat_error_is_surfaced() {
        let mut app = App::new();
        app.set_compat_error("schema 2".to_owned());
        assert!(app.compat_error.is_some());
        assert!(
            app.status_note
                .as_deref()
                .is_some_and(|n| n.contains("schema"))
        );
    }
}
