//! Состояние приложения TUI.
//!
//! Волна 1 — каркас: состояние эфира, выход и заметка о конфиге. Подключение
//! к ядру (`CoreClient`, `events`, worker) добавляет волна 2; конкретные
//! экраны — волна 3.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Состояние эфира — подмножество `status.state` схемы 1
/// (`docs/interfaces/16-tui.md` §5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayState {
    Playing,
    Paused,
    Stopped,
    Buffering,
    Reconnecting,
    Error,
}

impl PlayState {
    /// Английское слово состояния (служебные подписи TUI v1 — английские).
    pub fn word(self) -> &'static str {
        match self {
            Self::Playing => "playing",
            Self::Paused => "paused",
            Self::Stopped => "stopped",
            Self::Buffering => "buffering",
            Self::Reconnecting => "reconnecting",
            Self::Error => "error",
        }
    }
}

/// Минимальное состояние приложения.
#[derive(Debug)]
pub struct App {
    pub state: PlayState,
    pub station: Option<String>,
    pub favorite: bool,
    pub should_quit: bool,
    pub tick: u64,
    /// Заметка о происхождении конфига для строки статуса.
    pub config_note: Option<String>,
}

impl App {
    pub fn new() -> Self {
        Self {
            state: PlayState::Stopped,
            station: None,
            favorite: false,
            should_quit: false,
            tick: 0,
            config_note: None,
        }
    }

    /// Обработка нажатия клавиши. Волна 1: выход и демонстрация состояний.
    pub fn on_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            KeyCode::Char(' ') => {
                self.state = match self.state {
                    PlayState::Playing => PlayState::Paused,
                    _ => PlayState::Playing,
                };
            }
            KeyCode::Char('a') => self.favorite.toggle(),
            KeyCode::Char('1') => self.state = PlayState::Stopped,
            KeyCode::Char('2') => self.state = PlayState::Playing,
            KeyCode::Char('3') => self.state = PlayState::Paused,
            KeyCode::Char('4') => self.state = PlayState::Buffering,
            KeyCode::Char('5') => self.state = PlayState::Reconnecting,
            KeyCode::Char('6') => self.state = PlayState::Error,
            _ => {}
        }
    }

    pub fn on_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn q_quits_and_does_not_touch_playback() {
        let mut app = App::new();
        app.state = PlayState::Playing;
        app.on_key(key(KeyCode::Char('q')));
        assert!(app.should_quit);
        assert_matches!(app.state, PlayState::Playing);
    }

    #[test]
    fn ctrl_c_quits() {
        let mut app = App::new();
        app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.should_quit);
    }

    #[test]
    fn space_toggles_playing_paused() {
        let mut app = App::new();
        app.state = PlayState::Playing;
        app.on_key(key(KeyCode::Char(' ')));
        assert_matches!(app.state, PlayState::Paused);
        app.on_key(key(KeyCode::Char(' ')));
        assert_matches!(app.state, PlayState::Playing);
    }

    #[test]
    fn a_toggles_favorite() {
        let mut app = App::new();
        app.on_key(key(KeyCode::Char('a')));
        assert!(app.favorite);
        app.on_key(key(KeyCode::Char('a')));
        assert!(!app.favorite);
    }

    #[test]
    fn demo_keys_switch_states() {
        let mut app = App::new();
        for (ch, expected) in [
            ('1', PlayState::Stopped),
            ('2', PlayState::Playing),
            ('3', PlayState::Paused),
            ('4', PlayState::Buffering),
            ('5', PlayState::Reconnecting),
            ('6', PlayState::Error),
        ] {
            app.on_key(key(KeyCode::Char(ch)));
            assert_eq!(app.state, expected);
        }
    }
}
