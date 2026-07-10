//! Reusable rendering helpers shared across screens.

pub mod confirm;
pub mod form;
pub mod password;
pub mod spinner;
pub mod toast;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Paragraph;

/// Centre a fixed-size rectangle inside `area` for modal overlays.
pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [row] = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .areas(area);
    let [cell] = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .areas(row);
    cell
}

pub fn table_header_style() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

/// Draw a one-line, centred empty-state message inside a bordered table.
pub fn empty_state(frame: &mut Frame, area: Rect, text: impl Into<String>) {
    let inner = Rect::new(
        area.x.saturating_add(1),
        area.y.saturating_add(2),
        area.width.saturating_sub(2),
        area.height.saturating_sub(3),
    );
    let line = Rect::new(
        inner.x,
        inner.y.saturating_add(inner.height.saturating_sub(1) / 2),
        inner.width,
        inner.height.min(1),
    );
    frame.render_widget(
        Paragraph::new(text.into())
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray)),
        line,
    );
}
