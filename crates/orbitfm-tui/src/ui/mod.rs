//! Отрисовка кадра: каркас, экраны, оверлеи.
//!
//! Отрисовщики используют только токены `Theme` (`docs/ui/tui-design-system.md`)
//! и не знают о процессах. Экраны: Now Playing, Search, Favorites, History, Help.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, CoreLink, Overlay, Screen, format_remaining, play_state};
use crate::model::{LayoutPreset, PlayState, Station};
use crate::text;
use crate::theme::Theme;
use crate::version;
use crate::visualizer;

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

    if app.overlay != Overlay::None {
        draw_overlay(frame, area, app, theme);
    }
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let state = play_state(app.status.as_ref());
    let (glyph, word, style) = if state == PlayState::Playing {
        (theme.glyphs.live(), " live", theme.playing)
    } else {
        (theme.glyphs.idle(), " idle", theme.muted)
    };

    let version = match &app.core {
        CoreLink::Live { version } => version.clone(),
        CoreLink::Connecting => "connecting".to_owned(),
        CoreLink::Disconnected { .. } => "offline".to_owned(),
    };

    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(28)]).areas(area);

    let left = Line::from(vec![
        Span::styled(" ORBITFM", theme.accent),
        Span::raw(" "),
        Span::styled(glyph, style),
        Span::styled(word, style),
    ]);
    let right = Line::from(Span::styled(
        format!("tui {} · core {} ", version::short(), version),
        theme.muted,
    ))
    .alignment(Alignment::Right);

    frame.render_widget(Paragraph::new(left), left_area);
    frame.render_widget(Paragraph::new(right), right_area);
}

fn draw_body(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    if app.compat_error.is_some() {
        let message = Paragraph::new(vec![
            Line::from(Span::styled("! core compatibility", theme.error)),
            Line::from(Span::styled(
                app.compat_error.clone().unwrap_or_default(),
                theme.muted,
            )),
            Line::from(Span::styled(
                "update orbitfm-tui or the core before managing playback",
                theme.muted,
            )),
        ])
        .alignment(Alignment::Center)
        .block(panel(" Compatibility ", true, theme));
        frame.render_widget(message, area);
        return;
    }

    match app.layout {
        LayoutPreset::Desk => draw_desk(frame, area, app, theme),
        LayoutPreset::Focus => draw_focus(frame, area, app, theme),
        LayoutPreset::Minimal => draw_minimal(frame, area, app, theme),
    }
}

fn draw_desk(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let [now, rest] = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(area);
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(rest);
    draw_now_panel(frame, now, app, theme);
    draw_visualizer(frame, left, app, theme);
    draw_screen_panel(frame, right, app, theme);
}

fn draw_focus(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let [viz, now] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
    draw_visualizer(frame, viz, app, theme);
    draw_now_line(frame, now, app, theme);
}

fn draw_minimal(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let [now, rest] = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
    draw_now_line(frame, now, app, theme);
    if app.screen != Screen::NowPlaying {
        draw_screen_panel(frame, rest, app, theme);
    }
}

fn draw_screen_panel(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    match app.screen {
        Screen::Search => draw_list(frame, area, app, theme, " Search ", true),
        Screen::Favorites => draw_list(frame, area, app, theme, " Favorites (20 max) ", false),
        Screen::History => draw_list(frame, area, app, theme, " History (20 max) ", false),
        Screen::Help => draw_help(frame, area, app, theme),
        Screen::NowPlaying => draw_core_panel(frame, area, app, theme),
    }
}

fn draw_visualizer(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    if !app.visualizer_enabled {
        return;
    }
    let block = panel(" Visualizer ", false, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height >= 3 && inner.width >= 8 {
        visualizer::render(frame, inner, app.visualizer, &app.viz, theme);
    }
}

fn draw_now_panel(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    frame.render_widget(
        Paragraph::new(now_playing_lines(app, theme, area.width as usize)).block(panel(
            " Now Playing ",
            true,
            theme,
        )),
        area,
    );
}

fn draw_now_line(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let line = now_playing_lines(app, theme, area.width as usize)
        .into_iter()
        .next()
        .unwrap_or_default();
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_core_panel(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    frame.render_widget(
        Paragraph::new(core_lines(app, theme)).block(panel(" Core ", false, theme)),
        area,
    );
}

fn now_playing_lines(app: &App, theme: &Theme, width: usize) -> Vec<Line<'static>> {
    let state = play_state(app.status.as_ref());
    let station = app
        .status
        .as_ref()
        .and_then(|snapshot| snapshot.current_station());
    let mut lines = Vec::new();
    match station {
        Some(station) => {
            lines.push(Line::from(vec![
                Span::styled(theme.glyphs.state(state), theme.status_style(state)),
                Span::raw(" "),
                Span::styled(text::sanitize(&station.name), theme.text),
                Span::raw(" "),
                Span::styled(theme.glyphs.favorite(station.favorite), theme.favorite),
            ]));
            lines.push(Line::from(Span::styled(meta_line(station), theme.muted)));
            if !station.tags.is_empty() {
                lines.push(Line::from(Span::styled(
                    text::truncate(&text::sanitize(&station.tags), width),
                    theme.label,
                )));
            }
        }
        None => {
            lines.push(Line::from(Span::styled("Nothing playing", theme.muted)));
            lines.push(Line::from(Span::styled(
                "press / to search · f favorites · h history",
                theme.muted,
            )));
        }
    }
    lines
}

fn core_lines(app: &App, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(vec![
        Span::styled("volume ", theme.muted),
        Span::styled(volume_bar(app, theme), theme.text),
    ])];
    if let Some(snapshot) = &app.status {
        if let Some(sleep) = &snapshot.sleep {
            lines.push(Line::from(vec![
                Span::styled("sleep ", theme.muted),
                Span::styled(format_remaining(sleep.remaining_secs), theme.warning),
            ]));
        }
        lines.push(Line::from(Span::styled(
            format!(
                "skip [ {} · ] {}",
                if snapshot.can.previous { "on" } else { "off" },
                if snapshot.can.next { "on" } else { "off" }
            ),
            theme.muted,
        )));
    }
    if let Some(spectrum) = &app.spectrum {
        lines.push(Line::from(Span::styled(
            format!("viz level {:.2}", spectrum.level),
            theme.label,
        )));
    }
    lines
}

fn draw_list(frame: &mut Frame, area: Rect, app: &App, theme: &Theme, title: &str, editing: bool) {
    let [input, list] = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);

    if editing {
        let line = Line::from(vec![
            Span::styled("Search: ", theme.muted),
            Span::styled(text::sanitize(&app.search_input), theme.text),
            Span::styled("_", theme.accent),
        ]);
        frame.render_widget(Paragraph::new(line), input);
    } else if app.screen == Screen::Search {
        let query = app
            .search
            .as_ref()
            .map_or(String::new(), |frame| frame.query.clone());
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("last query: {}", text::sanitize(&query)),
                theme.muted,
            ))),
            input,
        );
    }

    let playing_uuid = app
        .status
        .as_ref()
        .and_then(|snapshot| snapshot.current_station())
        .map(|station| station.uuid.as_str());

    let list_area = if app.screen == Screen::Search {
        list
    } else {
        area
    };
    let mut lines: Vec<Line> = app
        .active_list()
        .iter()
        .enumerate()
        .map(|(index, station)| station_row(station, index == app.selection, playing_uuid, theme))
        .collect();

    if lines.is_empty() {
        lines.push(Line::from(Span::styled("Nothing here yet", theme.muted)));
    }
    if let Some(frame) = &app.search
        && let Some(error) = &frame.error
        && app.screen == Screen::Search
    {
        lines.push(Line::from(Span::styled(
            format!("! {}", text::sanitize(error)),
            theme.error,
        )));
    }

    frame.render_widget(
        Paragraph::new(lines).block(panel(title, true, theme)),
        list_area,
    );
}

fn draw_help(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let core = match &app.core {
        CoreLink::Live { version } => version.clone(),
        CoreLink::Connecting => "connecting".to_owned(),
        CoreLink::Disconnected { message } => format!("offline ({})", text::sanitize(message)),
    };
    let lines = vec![
        help_line("j/k", "move selection", theme),
        help_line("enter", "play selected", theme),
        help_line("p / space", "play/pause", theme),
        help_line("s", "stop", theme),
        help_line("[ / ]", "previous / next", theme),
        help_line("+ / -", "volume", theme),
        help_line("/", "search", theme),
        help_line("f / h", "favorites / history", theme),
        help_line("a", "toggle favorite", theme),
        help_line("t / T", "sleep timer / clear", theme),
        help_line("R", "reconnect", theme),
        help_line("q", "quit TUI (radio keeps playing)", theme),
        help_line("Q", "quit the core (confirm)", theme),
        Line::from(""),
        Line::from(Span::styled(
            format!("tui {} · core {}", version::short(), core),
            theme.muted,
        )),
        Line::from(Span::styled(
            "v1: 20 library entries, no mouse, English labels",
            theme.label,
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(panel(" Help ", true, theme)),
        area,
    );
}

fn draw_status(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let state = play_state(app.status.as_ref());
    let style = theme.status_style(state);
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(36)]).areas(area);

    let glyph = if state == PlayState::Buffering {
        theme.glyphs.spinner(app.tick)
    } else {
        theme.glyphs.state(state)
    };
    let message = app
        .status_note
        .clone()
        .unwrap_or_else(|| "ready".to_owned());
    let left = Line::from(vec![
        Span::styled(glyph, style),
        Span::raw(" "),
        Span::styled(state.word(), style),
        Span::raw("  "),
        Span::styled(message, theme.muted),
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

fn draw_overlay(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let text_lines = match app.overlay {
        Overlay::QuitConfirm => vec![
            Line::from("Stop playback and quit the core?"),
            Line::from(""),
            Line::from(vec![
                Span::styled("[y] yes, stop and quit", theme.danger),
                Span::raw("    "),
                Span::styled("[n] cancel", theme.muted),
            ]),
        ],
        Overlay::SleepPrompt => vec![
            Line::from("Sleep timer (minutes, 1..720)"),
            Line::from(""),
            Line::from(Span::styled(
                format!("> {}_", text::sanitize(&app.sleep_input)),
                theme.text,
            )),
            Line::from(Span::styled("enter apply · esc cancel", theme.muted)),
        ],
        _ => return,
    };

    let width = 46.min(area.width);
    let height = (text_lines.len() as u16 + 2).min(area.height);
    let [_, middle, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height),
        Constraint::Fill(1),
    ])
    .areas(area);
    let [_, center, _] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(width),
        Constraint::Fill(1),
    ])
    .areas(middle);

    let style = if app.overlay == Overlay::QuitConfirm {
        theme.danger
    } else {
        theme.accent
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(style)
        .title(Span::styled(
            if app.overlay == Overlay::QuitConfirm {
                " Quit OrbitFM? "
            } else {
                " Sleep "
            },
            style,
        ));
    frame.render_widget(Paragraph::new(text_lines).block(block), center);
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

fn panel(title: &str, focused: bool, theme: &Theme) -> Block<'static> {
    let style = if focused {
        theme.border_focus
    } else {
        theme.border
    };
    Block::default()
        .borders(Borders::ALL)
        .border_style(style)
        .title(Span::styled(title.to_owned(), style))
}

fn station_row(
    station: &Station,
    selected: bool,
    playing_uuid: Option<&str>,
    theme: &Theme,
) -> Line<'static> {
    let marker = if playing_uuid == Some(station.uuid.as_str()) {
        theme.glyphs.state(PlayState::Playing)
    } else if selected {
        theme.glyphs.selection()
    } else {
        " "
    };
    let name = text::clean(&station.name, 40);
    Line::from(vec![
        Span::raw(" "),
        Span::styled(marker.to_owned(), theme.accent),
        Span::raw(" "),
        Span::styled(name, theme.text),
        Span::raw("  "),
        Span::styled(text::sanitize(&station.countrycode), theme.muted),
        Span::raw("  "),
        Span::styled(format!("{:>4}k", station.bitrate), theme.muted),
        Span::raw("  "),
        Span::styled(
            theme.glyphs.favorite(station.favorite).to_owned(),
            theme.favorite,
        ),
    ])
}

fn meta_line(station: &Station) -> String {
    let parts = [
        text::sanitize(&station.countrycode),
        text::sanitize(&station.codec),
        format!("{}k", station.bitrate),
        text::sanitize(&station.source),
    ];
    parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

fn volume_bar(app: &App, theme: &Theme) -> String {
    let percent = app
        .status
        .as_ref()
        .map_or(0, |snapshot| snapshot.volume_percent());
    let filled = (percent as usize * 20) / 100;
    format!(
        "{}{} {percent:>3}%",
        theme.glyphs.gauge_filled().repeat(filled),
        theme.glyphs.gauge_empty().repeat(20 - filled),
    )
}

fn help_line(keys: &str, description: &str, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {keys:<12}"), theme.accent),
        Span::styled(description.to_owned(), theme.text),
    ])
}
