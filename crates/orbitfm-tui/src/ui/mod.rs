//! Отрисовка кадра.
//!
//! Волна 1: header, тело (две панели-заглушки), строка статуса и сообщение о
//! малом терминале. Экраны Now Playing/Search/Library/Help — волна 3.
//!
//! Отрисовщики используют только токены `Theme` (`docs/ui/tui-design-system.md`).

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, PlayState};
use crate::theme::Theme;
use crate::version;

/// Минимальный поддерживаемый размер (`docs/ui/tui-design-system.md` §3).
pub const MIN_WIDTH: u16 = 60;
pub const MIN_HEIGHT: u16 = 18;

/// Нарисовать полный кадр.
pub fn draw(frame: &mut Frame, app: &App, theme: &Theme) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        draw_too_small(frame, theme);
        return;
    }

    let [header, body, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_header(frame, header, app, theme);
    draw_body(frame, body, app, theme);
    draw_status(frame, status, app, theme);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let (glyph, word, style) = if app.state == PlayState::Playing {
        (theme.glyphs.live(), " live", theme.playing)
    } else {
        (theme.glyphs.idle(), " idle", theme.muted)
    };

    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(18)]).areas(area);

    let left = Line::from(vec![
        Span::styled(" ORBITFM", theme.accent),
        Span::raw(" "),
        Span::styled(glyph, style),
        Span::styled(word, style),
    ]);
    let right = Line::from(Span::styled(format!("v{} ", version::short()), theme.muted))
        .alignment(Alignment::Right);

    frame.render_widget(Paragraph::new(left), left_area);
    frame.render_widget(Paragraph::new(right), right_area);
}

fn draw_body(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(area);

    let station = app
        .station
        .as_deref()
        .filter(|name| !name.is_empty())
        .unwrap_or("Nothing playing");
    let now_playing = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(theme.glyphs.state(app.state), theme.status_style(app.state)),
            Span::raw(" "),
            Span::styled(station, theme.text),
            Span::raw(" "),
            Span::styled(theme.glyphs.favorite(app.favorite), theme.favorite),
        ]),
        Line::from(Span::styled("press / to search", theme.muted)),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border_focus)
            .title(Span::styled(" Now Playing ", theme.accent)),
    );
    frame.render_widget(now_playing, left);

    let spinner = theme.glyphs.spinner(app.tick);
    let core = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(format!("{spinner} "), theme.buffering),
            Span::styled("core: not connected", theme.text),
        ]),
        Line::from(Span::styled("wave 2: CoreClient", theme.muted)),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border)
            .title(Span::styled(" Core ", theme.label)),
    );
    frame.render_widget(core, right);
}

fn draw_status(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let style = theme.status_style(app.state);
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(36)]).areas(area);

    let left = Line::from(vec![
        Span::styled(theme.glyphs.state(app.state), style),
        Span::raw(" "),
        Span::styled(app.state.word(), style),
        Span::raw("  "),
        Span::styled(
            "q quit · space play/pause · a favorite · 1-6 demo",
            theme.muted,
        ),
    ]);

    let mut right_text = app
        .config_note
        .clone()
        .unwrap_or_else(|| format!("theme: {}", theme.name));
    if !theme.color {
        right_text.push_str(" (no color)");
    }
    let right = Line::from(Span::styled(right_text, theme.label)).alignment(Alignment::Right);

    frame.render_widget(Paragraph::new(left), left_area);
    frame.render_widget(Paragraph::new(right), right_area);
}

fn draw_too_small(frame: &mut Frame, theme: &Theme) {
    let area = frame.area();
    let text = Paragraph::new(vec![
        Line::from("Terminal too small"),
        Line::from(format!("{MIN_WIDTH}x{MIN_HEIGHT} minimum")),
    ])
    .alignment(Alignment::Center)
    .style(theme.muted);
    let [_, middle, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(2),
        Constraint::Fill(1),
    ])
    .areas(area);
    frame.render_widget(text, middle);
}
