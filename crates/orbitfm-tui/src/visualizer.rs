//! Визуализаторы: модель уровней и отрисовка четырёх режимов.
//!
//! Модель одна на все режимы; полотно — ячейки терминала, без аллокаций в
//! кадре (`[f32; LEVELS]`). `docs/ui/tui-design-system.md` §5.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::model::{SpectrumFrame, VisualizerKind};
use crate::theme::Theme;

/// Число полос модели (фиксированный буфер, без аллокаций).
pub const LEVELS: usize = 64;

/// Модель уровней: attack ≈ 28 мс, гравитация 1.8/с, peak-hold 0.42/с.
#[derive(Clone, Debug)]
pub struct VisualizerModel {
    pub levels: [f32; LEVELS],
    pub level: f32,
    pub peak: f32,
    phase: f32,
}

impl VisualizerModel {
    pub fn new() -> Self {
        Self {
            levels: [0.0; LEVELS],
            level: 0.0,
            peak: 0.0,
            phase: 0.0,
        }
    }

    /// Шаг модели. `frame` — reactive-кадр; иначе procedural при `playing`.
    pub fn tick(&mut self, dt: f32, frame: Option<&SpectrumFrame>, playing: bool) {
        let dt = dt.clamp(0.0, 0.1);
        self.phase = (self.phase + dt) % 1000.0;
        let base = match frame {
            Some(frame) => frame.level.clamp(0.0, 1.0),
            None if playing => procedural(self.phase),
            None => (0.03 + 0.02 * self.phase.sin()).max(0.0),
        };
        let bands = frame.and_then(|frame| frame.bands);
        for index in 0..LEVELS {
            let position = index as f32 / (LEVELS - 1) as f32;
            let target = match bands {
                Some(bands) => {
                    let band = if position < 0.33 {
                        bands.bass
                    } else if position < 0.66 {
                        bands.mid
                    } else {
                        bands.high
                    };
                    base * (0.35 + 0.65 * band.clamp(0.0, 1.0))
                }
                None => {
                    let wobble = 0.5 + 0.5 * (self.phase * 3.0 + index as f32 * 0.55).sin().abs();
                    base * (0.35 + 0.65 * wobble)
                }
            };
            let value = &mut self.levels[index];
            if target > *value {
                *value += (target - *value) * (dt / 0.028).min(1.0);
            } else {
                *value = (*value - 1.8 * dt).max(target);
            }
        }
        self.level = self.levels.iter().copied().fold(0.0_f32, f32::max);
        self.peak = (self.peak - 0.42 * dt).max(self.level);
    }

    fn sample(&self, column: usize, columns: usize) -> f32 {
        if columns == 0 {
            return 0.0;
        }
        self.levels[(column * LEVELS / columns).min(LEVELS - 1)]
    }
}

impl Default for VisualizerModel {
    fn default() -> Self {
        Self::new()
    }
}

fn procedural(phase: f32) -> f32 {
    (0.40 + 0.28 * (phase * 1.3).sin() + 0.14 * (phase * 3.1).sin()).clamp(0.0, 1.0)
}

/// Отрисовать визуализатор в зоне (пустая/малая зона — ничего).
pub fn render(
    frame: &mut Frame,
    area: Rect,
    kind: VisualizerKind,
    model: &VisualizerModel,
    theme: &Theme,
) {
    if area.height < 3 || area.width < 8 {
        return;
    }
    match kind {
        VisualizerKind::Bars => render_bars(frame, area, model, theme),
        VisualizerKind::Pulse => render_pulse(frame, area, model, theme),
        VisualizerKind::Scope => render_scope(frame, area, model, theme),
        VisualizerKind::Ring => render_ring(frame, area, model, theme),
    }
}

fn render_bars(frame: &mut Frame, area: Rect, model: &VisualizerModel, theme: &Theme) {
    let height = area.height as usize;
    let width = area.width as usize;
    let mut lines = Vec::with_capacity(height);
    for row in 0..height {
        let mut spans = Vec::with_capacity(width);
        for column in 0..width {
            let value = model.sample(column, width);
            let column_height = (value * height as f32).round() as usize;
            let from_bottom = height - 1 - row;
            if from_bottom < column_height {
                spans.push(Span::styled(
                    theme.glyphs.gauge_filled().to_owned(),
                    zone_style(from_bottom, height, theme),
                ));
            } else {
                spans.push(Span::raw(" "));
            }
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn render_pulse(frame: &mut Frame, area: Rect, model: &VisualizerModel, theme: &Theme) {
    let width = area.width.saturating_sub(2) as usize;
    let filled = (model.level.clamp(0.0, 1.0) * width as f32).round() as usize;
    let mut spans = vec![Span::styled(
        format!("{} ", theme.glyphs.live()),
        theme.playing,
    )];
    for column in 0..width {
        let (glyph, style) = if column < filled {
            (
                theme.glyphs.gauge_filled(),
                zone_style(column, width.max(1), theme),
            )
        } else {
            (theme.glyphs.gauge_empty(), theme.label)
        };
        spans.push(Span::styled(glyph.to_owned(), style));
    }
    let [_, middle, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(area);
    frame.render_widget(Paragraph::new(Line::from(spans)), middle);
}

fn render_scope(frame: &mut Frame, area: Rect, model: &VisualizerModel, theme: &Theme) {
    let height = area.height as usize;
    let width = area.width as usize;
    let mut grid = vec![vec![' '; width]; height];
    for column in 0..width {
        let value = model.sample(column, width).clamp(0.0, 1.0);
        let row = height - 1 - (value * (height - 1) as f32).round() as usize;
        if let Some(line) = grid.get_mut(row.min(height - 1))
            && let Some(cell) = line.get_mut(column)
        {
            *cell = '*';
        }
    }
    let lines: Vec<Line> = grid
        .into_iter()
        .map(|row| Line::from(row.into_iter().collect::<String>()))
        .collect();
    frame.render_widget(Paragraph::new(lines).style(theme.accent), area);
}

fn render_ring(frame: &mut Frame, area: Rect, model: &VisualizerModel, theme: &Theme) {
    let width = area.width as usize;
    let height = area.height as usize;
    let center_x = width as f32 / 2.0;
    let center_y = height as f32 / 2.0;
    let radius = (width.min(height * 2) as f32 * 0.3) * (0.6 + 0.4 * model.level.clamp(0.0, 1.0));
    let thickness = 0.5 + model.peak;
    let mut grid = vec![vec![' '; width]; height];
    for (row, line) in grid.iter_mut().enumerate() {
        for (column, cell) in line.iter_mut().enumerate() {
            let dx = column as f32 - center_x;
            let dy = (row as f32 - center_y) * 2.0;
            let distance = (dx * dx + dy * dy).sqrt();
            if (distance - radius).abs() <= thickness {
                *cell = '*';
            }
        }
    }
    let lines: Vec<Line> = grid
        .into_iter()
        .map(|row| Line::from(row.into_iter().collect::<String>()))
        .collect();
    frame.render_widget(Paragraph::new(lines).style(theme.viz_mid), area);
}

fn zone_style(from_bottom: usize, height: usize, theme: &Theme) -> Style {
    let ratio = from_bottom as f32 / height.max(1) as f32;
    if ratio < 0.5 {
        theme.viz_low
    } else if ratio < 0.8 {
        theme.viz_mid
    } else {
        theme.viz_high
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procedural_breathing_when_idle() {
        let mut model = VisualizerModel::new();
        model.tick(0.1, None, false);
        assert!(model.level <= 0.1);
    }

    #[test]
    fn attack_then_release_is_monotonic_within_limits() {
        let mut model = VisualizerModel::new();
        let frame = SpectrumFrame {
            level: 1.0,
            ..SpectrumFrame::default()
        };
        model.tick(0.1, Some(&frame), true);
        let raised = model.level;
        assert!(raised > 0.0);
        // Без кадров уровень падает (гравитация), но не мгновенно.
        model.tick(0.05, None, false);
        assert!(model.level <= raised);
        assert!(model.peak >= model.level);
    }

    #[test]
    fn sample_is_within_levels() {
        let mut model = VisualizerModel::new();
        for _ in 0..10 {
            model.tick(0.05, None, true);
        }
        assert!(model.sample(0, 40) <= 1.0);
        assert!(model.sample(39, 40) <= 1.0);
        assert_eq!(model.sample(0, 0), 0.0);
    }
}
