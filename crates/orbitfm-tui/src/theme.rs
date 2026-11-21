//! Токены дизайна TUI — единственный источник цвета, глифов и стилей.
//!
//! Спецификация: `docs/ui/tui-design-system.md`. Отрисовщики не используют
//! `Color::*` напрямую (ADR-0047 ревизия 3); только ANSI-16, `NO_COLOR`,
//! `glyphs = unicode | ascii`.

use ratatui::style::{Color, Modifier, Style};

use crate::app::PlayState;

/// Набор глифов: Unicode (по умолчанию) или ASCII-совместимый.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlyphSet {
    Unicode,
    Ascii,
}

impl GlyphSet {
    /// Автоопределение по локали: UTF-8 → Unicode, иначе ASCII
    /// (`docs/ui/tui-design-system.md` §7.4).
    pub fn auto() -> Self {
        let locale = std::env::var("LC_ALL")
            .ok()
            .filter(|v| !v.is_empty())
            .or_else(|| std::env::var("LC_CTYPE").ok().filter(|v| !v.is_empty()))
            .or_else(|| std::env::var("LANG").ok().filter(|v| !v.is_empty()));
        match locale {
            Some(locale) => {
                let locale = locale.to_ascii_lowercase();
                if locale.contains("utf-8") || locale.contains("utf8") {
                    Self::Unicode
                } else {
                    Self::Ascii
                }
            }
            None => Self::Ascii,
        }
    }

    /// Глиф состояния эфира.
    pub fn state(self, state: PlayState) -> &'static str {
        match (self, state) {
            (Self::Unicode, PlayState::Playing) => "▶",
            (Self::Unicode, PlayState::Paused) => "‖",
            (Self::Unicode, PlayState::Stopped) => "■",
            (Self::Unicode, PlayState::Buffering) => "...",
            (Self::Unicode, PlayState::Reconnecting) => "↻",
            (Self::Unicode, PlayState::Error) => "!",
            (Self::Ascii, PlayState::Playing) => ">",
            (Self::Ascii, PlayState::Paused) => "II",
            (Self::Ascii, PlayState::Stopped) => "#",
            (Self::Ascii, PlayState::Buffering) => "...",
            (Self::Ascii, PlayState::Reconnecting) => "@",
            (Self::Ascii, PlayState::Error) => "!",
        }
    }

    /// Маркер «эфир идёт».
    pub fn live(self) -> &'static str {
        match self {
            Self::Unicode => "●",
            Self::Ascii => "*",
        }
    }

    /// Маркер «тишина».
    pub fn idle(self) -> &'static str {
        match self {
            Self::Unicode => "○",
            Self::Ascii => "o",
        }
    }

    /// Маркер избранного.
    pub fn favorite(self, on: bool) -> &'static str {
        match (self, on) {
            (Self::Unicode, true) => "★",
            (Self::Unicode, false) => "☆",
            (Self::Ascii, true) => "*",
            (Self::Ascii, false) => "o",
        }
    }

    /// Кадр спиннера (10 кадров).
    pub fn spinner(self, tick: u64) -> &'static str {
        const UNICODE: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        const ASCII: [&str; 4] = ["|", "/", "-", "\\"];
        match self {
            Self::Unicode => UNICODE[(tick % UNICODE.len() as u64) as usize],
            Self::Ascii => ASCII[(tick % ASCII.len() as u64) as usize],
        }
    }
}

/// Тема интерфейса: один акцент плюс неизменная семантика статусов.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeKind {
    OrbitDark,
    Ice,
    Amber,
    Green,
    Magenta,
    Mono,
}

impl ThemeKind {
    /// Разбор имени темы из конфига/окружения.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "orbit-dark" => Some(Self::OrbitDark),
            "ice" => Some(Self::Ice),
            "amber" => Some(Self::Amber),
            "green" => Some(Self::Green),
            "magenta" => Some(Self::Magenta),
            "mono" => Some(Self::Mono),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::OrbitDark => "orbit-dark",
            Self::Ice => "ice",
            Self::Amber => "amber",
            Self::Green => "green",
            Self::Magenta => "magenta",
            Self::Mono => "mono",
        }
    }

    fn accent(self) -> Color {
        match self {
            Self::OrbitDark => Color::Cyan,
            Self::Ice => Color::Blue,
            Self::Amber => Color::Yellow,
            Self::Green => Color::Green,
            Self::Magenta => Color::Magenta,
            Self::Mono => Color::Reset,
        }
    }
}

/// Разрешённые токены отрисовки. UI обращается только к ним.
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub name: &'static str,
    pub color: bool,
    pub glyphs: GlyphSet,
    pub accent: Style,
    pub border: Style,
    pub border_focus: Style,
    pub text: Style,
    pub muted: Style,
    pub label: Style,
    pub playing: Style,
    pub paused: Style,
    pub stopped: Style,
    pub buffering: Style,
    pub reconnect: Style,
    pub error: Style,
    pub favorite: Style,
}

impl Theme {
    /// Собрать тему из вида, включён ли цвет и набор глифов.
    pub fn new(kind: ThemeKind, color: bool, glyphs: GlyphSet) -> Self {
        let color = color && !matches!(kind, ThemeKind::Mono);
        let accent_color = kind.accent();
        let styled = |c: Color, m: Modifier| {
            let mut style = Style::new();
            if color {
                style = style.fg(c);
            }
            if m != Modifier::empty() {
                style = style.add_modifier(m);
            }
            style
        };
        let plain = |c: Color| styled(c, Modifier::empty());
        let dim = |c: Color| styled(c, Modifier::DIM);
        let bold = |c: Color| styled(c, Modifier::BOLD);

        Self {
            name: kind.as_str(),
            color,
            glyphs,
            accent: bold(accent_color),
            border: plain(Color::DarkGray),
            border_focus: plain(accent_color),
            text: Style::new(),
            muted: dim(Color::Gray),
            label: plain(Color::DarkGray),
            playing: plain(Color::Green),
            paused: plain(Color::Yellow),
            stopped: plain(Color::DarkGray),
            buffering: plain(Color::Cyan),
            reconnect: plain(Color::Yellow),
            error: plain(Color::Red),
            favorite: plain(Color::Yellow),
        }
    }

    /// Тема из конфига TUI с учётом окружения.
    ///
    /// Порядок разрешения (ADR-0047 рев. 3): `tui.toml` → `NO_COLOR`/
    /// `TERM=dumb` → локаль → defaults. Явные `theme`/`glyphs` из конфига
    /// главнее локали; `glyphs = auto` оставляет решение локали.
    pub fn from_config(theme: &str, glyphs: &str) -> Self {
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        let dumb = std::env::var("TERM").is_ok_and(|t| t == "dumb");
        let glyphs = match glyphs {
            "unicode" => GlyphSet::Unicode,
            "ascii" => GlyphSet::Ascii,
            _ if dumb => GlyphSet::Ascii,
            _ => GlyphSet::auto(),
        };
        let kind = ThemeKind::parse(theme).unwrap_or(ThemeKind::OrbitDark);
        Self::new(kind, !no_color && !dumb, glyphs)
    }

    /// Стиль семантического состояния (не зависит от темы).
    pub fn status_style(&self, state: PlayState) -> Style {
        match state {
            PlayState::Playing => self.playing,
            PlayState::Paused => self.paused,
            PlayState::Stopped => self.stopped,
            PlayState::Buffering => self.buffering,
            PlayState::Reconnecting => self.reconnect,
            PlayState::Error => self.error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_kind_roundtrip() {
        let all = [
            ThemeKind::OrbitDark,
            ThemeKind::Ice,
            ThemeKind::Amber,
            ThemeKind::Green,
            ThemeKind::Magenta,
            ThemeKind::Mono,
        ];
        for kind in all {
            assert_eq!(ThemeKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(ThemeKind::parse("nope"), None);
    }

    #[test]
    fn mono_ignores_color() {
        let theme = Theme::new(ThemeKind::Mono, true, GlyphSet::Unicode);
        assert!(!theme.color);
        assert_eq!(theme.playing, Style::new());
    }

    #[test]
    fn no_color_keeps_semantics_distinct() {
        let theme = Theme::new(ThemeKind::OrbitDark, false, GlyphSet::Ascii);
        assert!(!theme.color);
        // В отсутствие цвета состояния всё равно различимы стилями/глифами.
        assert_eq!(theme.glyphs.state(PlayState::Playing), ">");
        assert_eq!(theme.glyphs.state(PlayState::Error), "!");
    }

    #[test]
    fn state_styles_are_semantic() {
        let theme = Theme::new(ThemeKind::OrbitDark, true, GlyphSet::Unicode);
        assert_ne!(
            theme.status_style(PlayState::Playing),
            theme.status_style(PlayState::Error)
        );
        assert_eq!(theme.status_style(PlayState::Playing), theme.playing);
    }

    #[test]
    fn from_config_accepts_theme_and_glyphs() {
        let theme = Theme::from_config("amber", "ascii");
        assert_eq!(theme.name, "amber");
        assert_eq!(theme.glyphs, GlyphSet::Ascii);
        // Неизвестная тема → default, а не паника.
        let fallback = Theme::from_config("nope", "auto");
        assert_eq!(fallback.name, "orbit-dark");
    }
}
